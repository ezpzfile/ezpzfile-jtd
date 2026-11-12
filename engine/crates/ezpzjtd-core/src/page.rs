//! Page setup (文書スタイル) from `/DocumentViewStyles`. Spec §10.
//!
//! The stream starts `0001 0002`, then record `1000` with a 32-bit length,
//! then records `(u16 tag, u16 length, bytes)`. The page records store only
//! the settings that differ from Ichitaro's built-in defaults: each group of
//! fields starts with a mask byte, and the fields of the set bits follow in
//! ascending bit order. A field whose size we do not know stops the parse of
//! that record, and the defaults stay.

use crate::layout::PageSetup;

/// Built-in defaults when a field is absent (1/100 mm, half-width columns).
const MARGIN: u32 = 3000;
const PAPER: (u32, u32) = (21000, 29700);
const HALF_CHARS: u32 = 80;
const LINES: u32 = 40;
const FONT: u32 = 370;

fn u16_at(b: &[u8], i: usize) -> Option<u32> {
    Some(u16::from_be_bytes([*b.get(i)?, *b.get(i + 1)?]) as u32)
}

fn u32_at(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_be_bytes([
        *b.get(i)?,
        *b.get(i + 1)?,
        *b.get(i + 2)?,
        *b.get(i + 3)?,
    ]))
}

/// Read one mask group: `sizes[bit]` is the byte size of that field (0 =
/// unknown). Returns the field values by bit and the offset after the group.
fn group(b: &[u8], at: usize, sizes: [u8; 8]) -> Option<([Option<u32>; 8], usize)> {
    let mask = *b.get(at)?;
    let mut i = at + 1;
    let mut out = [None; 8];
    for (bit, slot) in out.iter_mut().enumerate() {
        if mask & (1 << bit) == 0 {
            continue;
        }
        let v = match sizes[bit] {
            1 => *b.get(i)? as u32,
            2 => u16_at(b, i)?,
            4 => u32_at(b, i)?,
            _ => return None,
        };
        *slot = Some(v);
        i += sizes[bit] as usize;
    }
    Some((out, i))
}

fn records(b: &[u8]) -> Vec<(u16, &[u8])> {
    let mut out = Vec::new();
    if b.len() < 10 || b[..4] != [0, 1, 0, 2] || b[4..6] != [0x10, 0] {
        return out;
    }
    // only inside the first block; what follows it is not in this format
    let block_end = (10 + u32_at(b, 6).unwrap_or(0) as usize).min(b.len());
    let mut i = 10;
    while i + 4 <= block_end {
        let tag = u16::from_be_bytes([b[i], b[i + 1]]);
        let len = u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize;
        let end = (i + 4 + len).min(block_end);
        out.push((tag, &b[i + 4..end]));
        i += 4 + len;
    }
    out
}

/// Raw page values as stored (1/100 mm, half-width columns, lines).
#[derive(Debug, Clone, PartialEq)]
pub struct PageStyle {
    pub paper: (u32, u32),
    /// top, bottom, left, right
    pub margins: [u32; 4],
    pub half_chars: u32,
    pub lines: u32,
    /// 行間 as 0.1 % of the character size (`100B` bit 1), when stored
    pub gap: Option<u32>,
    pub font: u32,
    /// 縦組み (vertical writing)
    pub vertical: bool,
    /// 段組: columns and the gap between them (1/100 mm)
    pub columns: u32,
    pub col_gap: u32,
    /// 段組 2 on a B4 handout: the page is also cut across in two halves,
    /// each with its columns (top left, top right, bottom left, bottom
    /// right), and frames fixed on the page are placed from the top of their
    /// half (一太郎ビューア, a320)
    pub rows: u32,
    /// `1014` (00 00 04 ..): two pages on one landscape sheet. 一太郎ビューア
    /// shows each half as its own page (栃木 school newsletters on A4
    /// landscape with 2 段 in the style).
    pub two_up: bool,
}

impl Default for PageStyle {
    fn default() -> Self {
        PageStyle {
            paper: PAPER,
            margins: [MARGIN; 4],
            half_chars: HALF_CHARS,
            lines: LINES,
            gap: None,
            font: FONT,
            vertical: false,
            columns: 1,
            col_gap: 0,
            rows: 1,
            two_up: false,
        }
    }
}

/// Parse `/DocumentViewStyles`. `None` when the stream does not have the
/// expected shape.
pub fn parse(b: &[u8]) -> Option<PageStyle> {
    let recs = records(b);
    if recs.is_empty() {
        return None;
    }
    let mut s = PageStyle::default();
    for (tag, p) in recs {
        match tag {
            0x1014 => s.two_up = p.get(2) == Some(&4),
            // paper: bit1 width, bit2 height (u32, 1/100 mm)
            0x1001 => {
                if let Some((f, _)) = group(p, 0, [0, 4, 4, 1, 0, 1, 1, 0]) {
                    // one left out keeps its default (an A3 landscape 卒業証書
                    // gives only its width, 420 mm)
                    if f[1].is_some() || f[2].is_some() {
                        s.paper = (f[1].unwrap_or(PAPER.0), f[2].unwrap_or(PAPER.1));
                    }
                }
            }
            // margins: group 1 (unknown), group 2 bits 3-6 top, bottom, left,
            // right (u16, 1/100 mm), then group 3 bit 4 = 縦組み
            0x1002 => {
                if let Some((_, i)) = group(p, 0, [0, 0, 0, 0, 2, 0, 0, 2]) {
                    // bit 2 is a u16 too (masks 0xbc, 0xfc on school newsletters written
                    // vertically: read as unknown, their 縦組み was missed)
                    if let Some((f, i)) = group(p, i, [1, 0, 2, 2, 2, 2, 2, 2]) {
                        for k in 0..4 {
                            if let Some(v) = f[3 + k] {
                                s.margins[k] = v;
                            }
                        }
                        // (group 2 bit 0 (u8) is 1 on school letters written
                        // across too: 学校だより with a calendar in rows, whose
                        // 縦書き comes from their text boxes. Not 縦組み.)
                        // group 3: bit1 (u8) 段組 (1 or 2: two columns), bit4 (u8) 1 = 縦組み, bit6 ten u16: the
                        // gaps between columns (1/100 mm). The sizes of bits
                        // 0, 2, 3 and 5 are unknown, so stop at them.
                        if let Some(&m) = p.get(i) {
                            let mut j = i + 1;
                            if m & 0x2d == 0 {
                                if m & 0x02 != 0 {
                                    // 1 on a two-card memo form and 2 on a B4 handout, both
                                    // in two columns in 一太郎ビューア; three columns not seen yet
                                    s.columns = if p.get(j).is_some_and(|&c| c >= 1) { 2 } else { 1 };
                                    s.rows = if p.get(j) == Some(&2) { 2 } else { 1 };
                                    j += 1;
                                }
                                if m & 0x10 != 0 {
                                    s.vertical = p.get(j) == Some(&1);
                                    j += 1;
                                }
                                if m & 0x40 != 0 {
                                    s.col_gap = u16_at(p, j).unwrap_or(0) as u32;
                                }
                            }
                        }
                    }
                }
            }
            // character grid: bit7 characters per line (half-width columns)
            0x100b => {
                if let Some((f, _)) = group(p, 0, [1, 2, 1, 0, 1, 0, 1, 2]) {
                    if let Some(v) = f[7].filter(|&v| v > 0) {
                        s.half_chars = v;
                    }
                    s.gap = f[1];
                }
            }
            // bit1 lines per page
            0x100d => {
                // bit1 (u16) comes first whatever follows: bit5 is a list of
                // line marks of unread size (0x22 on most school handouts, whose
                // 行数 of 43 to 70 was read as the default 40)
                if let Some(&m) = p.first().filter(|&&m| m & 0x02 != 0) {
                    // bit0 is a u8 before it
                    let at = 1 + (m & 0x01) as usize;
                    if let Some(v) = u16_at(p, at).map(|v| v as u32).filter(|&v| v > 0) {
                        s.lines = v;
                    }
                }
            }
            // bit0 base character size (u32, 1/100 mm)
            0x1006 if p.first().map(|m| m & 1 == 1).unwrap_or(false) => {
                if let Some(v) = u32_at(p, 1).filter(|&v| v > 0) {
                    s.font = v;
                }
            }
            _ => {}
        }
    }
    Some(s)
}

impl PageStyle {
    /// 行数, unless it gives lines more than twice as far apart as 行間 does
    /// (a B4 handout says 20 lines, but its lines are 5.5 mm apart in
    /// 一太郎ビューア, as 42 % 行間 of 11 pt gives): then as many lines as
    /// 行間 fits. Below twice, 行数 wins (A4 forms of 40 lines whose 行間
    /// says 9 %).
    fn lines_for(&self, margins_mm: f32, height_mm: f32, font_pt: f32) -> u32 {
        let lines = self.lines.max(1);
        let Some(gap) = self.gap.filter(|&g| g > 0 && g < 5000) else { return lines };
        let text_pt = (height_mm - margins_mm).max(10.0) * 72.0 / 25.4;
        let want = font_pt * (1.0 + gap as f32 / 1000.0);
        if text_pt / lines as f32 > 2.0 * want {
            (text_pt / want).floor() as u32
        } else {
            lines
        }
    }

    /// Page geometry for layout.
    pub fn setup(&self) -> PageSetup {
        let mm = |v: u32| v as f32 / 100.0;
        let (mut w, mut h) = (mm(self.paper.0), mm(self.paper.1));
        if !(50.0..=1500.0).contains(&w) || !(50.0..=1500.0).contains(&h) {
            (w, h) = (210.0, 297.0);
        }
        let [t, b, l, r] = self.margins.map(mm);
        let ok = |a: f32, z: f32, total: f32| a >= 0.0 && z >= 0.0 && a + z < total * 0.9;
        let d = PageSetup::default();
        let (t, b) = if ok(t, b, h) {
            (t, b)
        } else {
            (d.margin_top_mm, d.margin_bottom_mm)
        };
        let (l, r) = if ok(l, r, w) {
            (l, r)
        } else {
            (d.margin_left_mm, d.margin_right_mm)
        };
        let font_pt = ((self.font as f32) / 35.2778 * 2.0).round() / 2.0;
        // two pages on the sheet: each page is half the sheet, one column
        let two_up = self.two_up && !self.vertical && w > h;
        if two_up {
            w /= 2.0;
        }
        // 縦組み: lay out on the page turned a quarter turn. Its top is the
        // paper's right edge, its left the paper's top (see PageSetup::vertical).
        let (w, h, t, b, l, r) = if self.vertical { (h, w, r, l, t, b) } else { (w, h, t, b, l, r) };
        PageSetup {
            width_mm: w,
            height_mm: h,
            margin_top_mm: t,
            margin_bottom_mm: b,
            margin_left_mm: l,
            margin_right_mm: r,
            chars_per_line: (self.half_chars / 2).clamp(4, 400),
            lines_per_page: self.lines_for(t + b, h, font_pt).clamp(2, 400),
            font_pt: if (4.0..=100.0).contains(&font_pt) {
                font_pt
            } else {
                10.5
            },
            vertical: self.vertical,
            columns: if two_up { 1 } else { self.columns.clamp(1, 10) },
            col_gap_mm: mm(self.col_gap.min(10000)),
            rows: if two_up { 1 } else { self.rows.clamp(1, 2) },
            short_cols: false,
            background: (None, false),
            ..PageSetup::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stream(recs: &[(u16, &[u8])]) -> Vec<u8> {
        let mut body = Vec::new();
        for (tag, p) in recs {
            body.extend_from_slice(&tag.to_be_bytes());
            body.extend_from_slice(&(p.len() as u16).to_be_bytes());
            body.extend_from_slice(p);
        }
        let mut b = vec![0, 1, 0, 2, 0x10, 0];
        b.extend_from_slice(&(body.len() as u32).to_be_bytes());
        b.extend(body);
        b.extend_from_slice(&[0x10, 0x01, 0, 2, 0x06, 0]); // after the block: ignored
        b
    }

    /// The records of a new document in the latest 一太郎: only what differs from the
    /// built-in defaults is stored.
    #[test]
    fn defaults() {
        let b = stream(&[
            (0x1001, &[0x00, 0x04, 0x01]),
            (
                0x1002,
                &[
                    0x00, 0xd8, 0x0b, 0xb8, 0x0b, 0xb8, 0x0b, 0xb8, 0x0b, 0xb8, 0x00,
                ],
            ),
            (0x100b, &[0x02, 0x02, 0x58, 0x00, 0x04, 0, 0, 0, 8]),
            (
                0x1006,
                &[0x1f, 0, 0, 0x01, 0x72, 0, 0, 0xff, 0xfe, 0xff, 0xfe],
            ),
            (0x100d, &[0x02, 0x00, 0x28]),
        ]);
        let s = parse(&b).unwrap();
        assert_eq!(s.gap, Some(600));
        assert_eq!(PageStyle { gap: None, ..s.clone() }, PageStyle::default());
        let p = s.setup();
        assert_eq!((p.width_mm, p.height_mm), (210.0, 297.0));
        assert_eq!(
            (p.chars_per_line, p.lines_per_page, p.font_pt),
            (40, 40, 10.5)
        );
    }

    /// Landscape A4 with 90 字 × 31 行 and its own margins, as in a public
    /// form (spec §10).
    #[test]
    fn landscape_form() {
        let mut m = vec![0x90, 0, 0, 0, 0, 0xf8];
        for v in [2190u16, 2000, 2290, 508, 0] {
            m.extend_from_slice(&v.to_be_bytes());
        }
        m.push(0x40);
        for _ in 0..10 {
            m.extend_from_slice(&700u16.to_be_bytes());
        }
        m.push(0);
        let b = stream(&[
            (
                0x1001,
                &[0x06, 0, 0, 0x74, 0x04, 0, 0, 0x52, 0x08, 0x04, 0x01],
            ),
            (0x1002, &m),
            (
                0x100b,
                &[
                    0x97, 0x01, 0x00, 0xad, 0x00, 0x00, 0x00, 0xb4, 0x40, 0x00, 0x04, 0, 0, 0, 8,
                ],
            ),
            (0x100d, &[0x03, 0x00, 0x00, 0x1f]),
        ]);
        let p = parse(&b).unwrap().setup();
        assert_eq!((p.width_mm, p.height_mm), (297.0, 210.0));
        assert_eq!(
            [
                p.margin_top_mm,
                p.margin_bottom_mm,
                p.margin_left_mm,
                p.margin_right_mm
            ],
            [21.9, 20.0, 22.9, 5.08]
        );
        assert_eq!((p.chars_per_line, p.lines_per_page), (90, 31));
    }

    /// 縦組み is group 3 bit 4 of the margin record.
    #[test]
    fn vertical() {
        let mut m = vec![0x00, 0xd8];
        for _ in 0..4 {
            m.extend_from_slice(&3000u16.to_be_bytes());
        }
        m.extend_from_slice(&[0x50, 0x01]);
        let s = parse(&stream(&[(0x1002, &m)])).unwrap();
        assert!(s.vertical);
    }

    #[test]
    fn not_a_view_style_stream() {
        assert_eq!(parse(b"\x00\x01\x00\x03\x10\x00\x00\x00\x00\x00"), None);
        assert_eq!(parse(b""), None);
    }
}
