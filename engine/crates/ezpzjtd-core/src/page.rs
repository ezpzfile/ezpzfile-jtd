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
    pub font: u32,
    /// 縦組み (vertical writing)
    pub vertical: bool,
}

impl Default for PageStyle {
    fn default() -> Self {
        PageStyle {
            paper: PAPER,
            margins: [MARGIN; 4],
            half_chars: HALF_CHARS,
            lines: LINES,
            font: FONT,
            vertical: false,
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
            // paper: bit1 width, bit2 height (u32, 1/100 mm)
            0x1001 => {
                if let Some((f, _)) = group(p, 0, [0, 4, 4, 1, 0, 1, 1, 0]) {
                    if let (Some(w), Some(h)) = (f[1], f[2]) {
                        s.paper = (w, h);
                    }
                }
            }
            // margins: group 1 (unknown), group 2 bits 3-6 top, bottom, left,
            // right (u16, 1/100 mm), then group 3 bit 4 = 縦組み
            0x1002 => {
                if let Some((_, i)) = group(p, 0, [0, 0, 0, 0, 2, 0, 0, 2]) {
                    if let Some((f, i)) = group(p, i, [1, 0, 0, 2, 2, 2, 2, 2]) {
                        for k in 0..4 {
                            if let Some(v) = f[3 + k] {
                                s.margins[k] = v;
                            }
                        }
                        // group 3: bit4 (u8) 1 = 縦組み; the sizes of bits 0-3
                        // are unknown, so only read it when they are absent
                        if let Some(&m) = p.get(i) {
                            if m & 0x10 != 0 && m & 0x0f == 0 {
                                s.vertical = p.get(i + 1) == Some(&1);
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
                }
            }
            // bit1 lines per page
            0x100d => {
                if let Some((f, _)) = group(p, 0, [1, 2, 0, 0, 0, 0, 0, 0]) {
                    if let Some(v) = f[1].filter(|&v| v > 0) {
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
        PageSetup {
            width_mm: w,
            height_mm: h,
            margin_top_mm: t,
            margin_bottom_mm: b,
            margin_left_mm: l,
            margin_right_mm: r,
            chars_per_line: (self.half_chars / 2).clamp(4, 400),
            lines_per_page: self.lines.clamp(2, 400),
            font_pt: if (4.0..=100.0).contains(&font_pt) {
                font_pt
            } else {
                10.5
            },
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
        assert_eq!(s, PageStyle::default());
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
