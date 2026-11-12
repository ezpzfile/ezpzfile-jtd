//! Pictures placed on the page (枠): `/Frame` lists frames, `/EmbedItems`
//! holds the pictures, and the text holds an anchor record per frame.
//!
//! Read from public files and checked against Ichitaro Viewer (2026-11-11):
//!
//! - `/Frame`: `00 01 00 04 00 02 00 01`, then records `u16 type, u16 len,
//!   payload`. Type `0102` (56 bytes) is a frame: words `[0] 0, [1] frame id,
//!   [2] kind (1 = embedded picture, 2 = text box, 4 = figure), [3] 0,
//!   [4] item number (Embedding n), [5] ?, [6] page, [7] 1`, then from byte
//!   26 of the record (with its 4-byte head) `x, y, w, h` as u32 in 1/100 mm.
//! - `x` is measured from the left edge of the text area, `y` from the top of
//!   the line that holds the anchor.
//! - Anchor: a record of class `0000` in `/DocumentText` with payload
//!   `[0, 30, ffff, 507, 12, frame id, 0]`.
//! - `/EmbedItems/EmbeddingInfo` maps a picture frame's id to its
//!   `Embedding n` storage (see [`embedding_map`]).
//! - `/EmbedItems/Embedding n/Contents`: a header with the original file
//!   path, then a `u32` length and the picture file itself (GIF, JPEG, PNG or
//!   BMP). `EmbeddedPress` (`JSSnapShot32`) is a preview we do not read.

use crate::cfb::Cfb;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Frame {
    pub id: u16,
    /// 1 = embedded picture, 2 = text box, 4 = figure
    pub kind: u16,
    /// `Embedding n` for pictures
    pub item: u16,
    /// 1/100 mm
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    /// Word [6] = 2: placed on the anchor's page, measured from the paper's
    /// top left, rather than from the anchor's line (word [6] = 1).
    #[serde(default)]
    pub fixed: bool,
    /// Word [5] = 1: measured from the text area rather than from the
    /// anchoring character (a 縦組み 学校だより's title scroll).
    #[serde(default)]
    pub on_text: bool,
    /// Word [25] has 0x100 or 0x200 and not 4: the text goes round the
    /// frame (回り込み): photos 0x300. With 4 (a picture behind a card's
    /// text, 0x304 or 4) the text runs under it. Text boxes (0xb80) do not
    /// push the text: a handout's lines land where 一太郎ビューア puts them
    /// (199 mm) only when they are left in place.
    #[serde(default)]
    pub wrap: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Picture {
    /// The frame id that shows this picture
    pub item: u16,
    /// For a figure kept with a text box's layout (`/FigureData/layout_data
    /// n`): that layout number n (the text box's number in
    /// `/LayoutBoxText`). Placed from the box's top left.
    #[serde(skip)]
    pub layout: Option<u32>,
    pub mime: &'static str,
    #[serde(skip)]
    pub data: Vec<u8>,
}

fn be16(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*b.get(o)?, *b.get(o + 1)?]))
}
fn be32(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_be_bytes([*b.get(o)?, *b.get(o + 1)?, *b.get(o + 2)?, *b.get(o + 3)?]))
}

/// Frames of one sheet (`base` is "" for the main sheet).
pub fn read_frames(cfb: &Cfb, base: &str) -> Vec<Frame> {
    let Some(b) = cfb.read(&format!("{base}/Frame")) else { return Vec::new() };
    let mut out = Vec::new();
    let mut o = 8;
    while o + 4 <= b.len() && out.len() < 4096 {
        let (Some(t), Some(l)) = (be16(&b, o), be16(&b, o + 2)) else { break };
        let l = l as usize;
        if l == 0 {
            break;
        }
        let r = &b[o..(o + 4 + l).min(b.len())];
        if t == 0x0102 && r.len() >= 42 {
            let w = |i: usize| be16(r, 4 + i * 2).unwrap_or(0);
            out.push(Frame {
                id: w(1),
                kind: w(2),
                item: w(4),
                x: be32(r, 26).unwrap_or(0) as i32,
                y: be32(r, 30).unwrap_or(0) as i32,
                w: be32(r, 34).unwrap_or(0),
                h: be32(r, 38).unwrap_or(0),
                fixed: w(6) == 2,
                on_text: w(5) == 1,
                wrap: w(25) & 0x300 != 0 && w(25) & 4 == 0,
            });
        }
        o += 4 + l;
    }
    out
}

/// The picture file inside an `Embedding n/Contents` stream, if it is one we can show.
pub fn picture_in(contents: &[u8]) -> Option<(&'static str, Vec<u8>)> {
    let sigs: [(&[u8], &str); 4] = [
        (b"GIF8", "image/gif"),
        (b"\x89PNG", "image/png"),
        (b"\xff\xd8\xff", "image/jpeg"),
        (b"BM", "image/bmp"),
    ];
    // the picture starts after the header; its length is the u32 just before it
    let limit = contents.len().min(2048);
    for i in 4..limit {
        for (sig, mime) in sigs {
            if contents[i..].starts_with(sig) {
                let n = u32::from_le_bytes([contents[i - 4], contents[i - 3], contents[i - 2], contents[i - 1]]) as usize;
                if n > 16 && i + n <= contents.len() {
                    return Some((mime, contents[i..i + n].to_vec()));
                }
                if mime != "image/bmp" {
                    return Some((mime, contents[i..].to_vec()));
                }
            }
        }
    }
    None
}

/// `/EmbedItems/EmbeddingInfo`: which `Embedding n` storage each picture
/// frame shows, as (frame id, n). `u32 count`, then from byte 16 one entry
/// per storage (little-endian): `+8` the storage number, `+0x2a` the byte
/// length L of a UTF-16 class name (`JS.PictureCtrl.1`) that follows, then
/// the frame id (u32) and 0x4C more bytes.
pub fn embedding_map(b: &[u8]) -> Vec<(u16, u16)> {
    let le = |o: usize| b.get(o..o + 4).map(|x| u32::from_le_bytes([x[0], x[1], x[2], x[3]]));
    let Some(count) = le(0) else { return Vec::new() };
    let mut out = Vec::new();
    let mut o = 16usize;
    for _ in 0..(count as usize).min(4096) {
        let (Some(n), Some(l)) = (le(o + 8), le(o + 0x2a)) else { break };
        let l = l as usize;
        if l > 512 {
            break;
        }
        let Some(id) = le(o + 0x2e + l) else { break };
        out.push((id as u16, n as u16));
        o += 0x7e + l;
    }
    out
}

/// Pictures of the picture frames, by frame id (`Picture::item` holds the frame id).
pub fn read_pictures(cfb: &Cfb, frames: &[Frame], max_bytes: usize) -> Vec<Picture> {
    let map = cfb
        .read("/EmbedItems/EmbeddingInfo")
        .map(|b| embedding_map(&b))
        .unwrap_or_default();
    let mut out: Vec<Picture> = Vec::new();
    let mut total = 0usize;
    for f in frames.iter().filter(|f| f.kind == 1) {
        let Some(&(_, n)) = map.iter().find(|(id, _)| *id == f.id) else { continue };
        let found = cfb
            .read(&format!("/EmbedItems/Embedding {n}/Contents"))
            .and_then(|c| picture_in(&c))
            // a picture from another program (OLE): its vector preview
            .or_else(|| {
                let dir = format!("/EmbedItems/Embedding {n}/");
                let e = cfb.streams().find(|e| e.path.starts_with(&dir) && e.name.ends_with("EmbeddedPress"))?;
                let svg = crate::figure::snapshot_svg(&cfb.read(&e.path)?)?;
                Some(("image/svg+xml", svg.into_bytes()))
            });
        if let Some((mime, data)) = found {
            total += data.len();
            if total > max_bytes {
                break;
            }
            out.push(Picture { item: f.id, mime, data, layout: None });
        }
    }
    // figures (図形): each frame of kind 4 shows the figure of its size
    if frames.iter().any(|f| f.kind == 4) {
        let mut paths: Vec<&str> = cfb
            .streams()
            .filter(|e| e.path.starts_with("/FigureData/") && e.name == "FDMVector")
            .map(|e| e.path.as_str())
            .collect();
        // main_data first, then layout_data n by n
        paths.sort_by_key(|p| (!p.contains("/main_data/"), p.split("layout_data ").nth(1).and_then(|r| r.split('/').next()).and_then(|n| n.trim().parse::<u32>().ok()).unwrap_or(0)));
        let mut figs: Vec<Option<(crate::figure::Figure, Option<u32>)>> = Vec::new();
        for p in paths {
            let layout = p.split("layout_data ").nth(1).and_then(|r| r.split('/').next()).and_then(|n| n.trim().parse::<u32>().ok());
            if let Some(b) = cfb.read(p) {
                figs.extend(crate::figure::figures(&b).into_iter().map(|f| Some((f, layout))));
            }
        }
        let near = |a: u32, b: u32| a.abs_diff(b) <= 20.max(a / 50);
        for f in frames.iter().filter(|f| f.kind == 4) {
            let Some(slot) = figs.iter_mut().find(|g| g.as_ref().is_some_and(|(g, _)| near(g.w, f.w) && near(g.h, f.h))) else { continue };
            let (g, layout) = slot.take().unwrap();
            total += g.svg.len();
            if total > max_bytes {
                break;
            }
            out.push(Picture { item: f.id, mime: "image/svg+xml", data: g.svg.into_bytes(), layout });
        }
    }
    out
}

/// The frame id of an anchor record (class `0000`), if it is one.
pub fn anchor_id(payload: &[u16]) -> Option<u16> {
    (payload.len() >= 6 && payload[1] == 0x30 && payload[3] == 0x507).then(|| payload[5])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picture_after_header() {
        let gif = b"GIF89a\x01\x00\x01\x00\x00\x00\x00;".to_vec();
        let mut c = vec![8, 0, 1, 0, 0, 0, 0, 0];
        c.extend_from_slice(b"C:\\x.gif\0");
        c.extend_from_slice(&(gif.len() as u32).to_le_bytes());
        c.extend_from_slice(&gif);
        let (mime, data) = picture_in(&c).unwrap();
        assert_eq!(mime, "image/gif");
        assert_eq!(data, gif);
    }

    #[test]
    fn embedding_info_entries() {
        // two entries with a 34-byte class name: frame 0 -> Embedding 1, frame 3 -> Embedding 4
        let mut b = vec![0u8; 16];
        b[0] = 2;
        for (n, id) in [(1u32, 0u32), (4, 3)] {
            let mut e = vec![0u8; 0x2e];
            e[8..12].copy_from_slice(&n.to_le_bytes());
            e[0x2a..0x2e].copy_from_slice(&34u32.to_le_bytes());
            e.extend(std::iter::repeat(0x41).take(34));
            e.extend_from_slice(&id.to_le_bytes());
            e.extend(std::iter::repeat(0).take(0x4c));
            b.extend(e);
        }
        assert_eq!(embedding_map(&b), vec![(0, 1), (3, 4)]);
    }

    #[test]
    fn box_direction_and_size() {
        // 0201 box; 6668 group with 6006 (flags 0d: direction 1, left and
        // right 1 mm) and 6004 (character size 11.99 mm)
        let mut b = vec![0u8; 8];
        b.extend_from_slice(&[0x02, 0x01, 0, 8, 0, 0, 0, 0, 0, 0, 0, 0]);
        let recs: Vec<u8> = [
            vec![0x60, 0x06, 0, 6, 0x0d, 1, 0, 100, 0, 100],
            vec![0x60, 0x04, 0, 7, 3, 1, 0xff, 0, 0, 0x04, 0xaf],
        ]
        .concat();
        b.extend_from_slice(&[0x66, 0x68, 0, 0]);
        b.extend_from_slice(&(recs.len() as u16).to_be_bytes());
        b.extend_from_slice(&recs);
        let st = box_styles(&b);
        assert_eq!(st.len(), 1);
        assert!(st[0].vertical);
        assert_eq!(st[0].pad, [100, 100, 0, 0]);
        assert_eq!(st[0].size, Some(1199));
    }

    #[test]
    fn anchor_record() {
        assert_eq!(anchor_id(&[0, 0x30, 0xffff, 0x507, 0x12, 5, 0]), Some(5));
        assert_eq!(anchor_id(&[0, 0x31, 0xffff, 0x507, 0x12, 5, 0]), None);
    }
}

/// A text box: its text and the space between its edge and the text
/// (`/LayoutBox` record 6006, 1/100 mm; 0 when not found).
#[derive(Debug, Clone, Default)]
pub struct TextBox {
    pub blocks: Vec<crate::doc::Block>,
    /// left, right, top, bottom
    pub pad: [u16; 4],
    /// The box's frame line, when it has one.
    pub border: Option<Border>,
    /// The box's layout number (its index in the `/LayoutBoxText` directory).
    pub layout: u32,
    /// The box writes top to bottom (縦書き; `6006` bit 0 holds 1).
    pub vertical: bool,
    /// The box's character size in pt (`6004`), when given.
    pub size_pt: Option<f32>,
}

/// A text box's frame line (`/LayoutBox` records `6007` and `0401`).
#[derive(Debug, Clone, PartialEq)]
pub struct Border {
    /// `#rrggbb`
    pub color: String,
    pub width_mm: f32,
    /// Corner radius
    pub radius_mm: f32,
    pub dotted: bool,
    /// The line is drawn (a box can have only a fill).
    pub line: bool,
    /// Fill: none, one colour, or gradient stops (0-1, colour) at `angle`
    /// degrees (`0403` type 1 / 2 and `0404`).
    pub fill: Vec<(f32, String)>,
    pub angle: f32,
    /// Drop shadow: offset right and down (mm) and colour.
    pub shadow: Option<(f32, f32, String)>,
}

#[derive(Default)]
struct BoxStyle {
    pad: [u16; 4],
    vertical: bool,
    size: Option<u32>,
    line: Option<u16>,
    /// radius, width, line colour, shadow x, shadow y, shadow colour
    shape: Option<(u32, u32, String, u32, u32, String)>,
    fill_type: u32,
    fill_color: String,
    stops: Vec<(f32, String)>,
    angle: f32,
}

/// Inner margins of each box of `/LayoutBox`, in box order. Each box is
/// `0201` (8 bytes), `6668` + u16 0 + u16 length holding records `60xx`
/// (6006: u8 flags, then u16 left, right, top, bottom), then `0401`, `0403`.
fn box_styles(b: &[u8]) -> Vec<BoxStyle> {
    let mut out: Vec<BoxStyle> = Vec::new();
    let mut o = 8usize;
    let be32 = |q: usize| b.get(q..q + 4).map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]));
    while o + 4 <= b.len() && out.len() < 4096 {
        let tag = u16::from_be_bytes([b[o], b[o + 1]]);
        let len = u16::from_be_bytes([b[o + 2], b[o + 3]]) as usize;
        if tag == 0x0201 {
            out.push(BoxStyle::default());
            o += 4 + len;
        } else if tag == 0x6668 && o + 6 <= b.len() {
            let glen = u16::from_be_bytes([b[o + 4], b[o + 5]]) as usize;
            let (mut q, end) = (o + 6, (o + 6 + glen).min(b.len()));
            while q + 4 <= end {
                let t = u16::from_be_bytes([b[q], b[q + 1]]);
                let l = u16::from_be_bytes([b[q + 2], b[q + 3]]) as usize;
                if let Some(st) = out.last_mut() {
                    // 6006: u8 flags, then (bit 0) u8 writing direction
                    // (1 = 縦書き; the body box of a 縦組み 学校だより) and
                    // (bits 2-5) u16 left, right, top, bottom
                    if t == 0x6006 && l >= 1 && q + 4 + l <= end {
                        let f = b[q + 4];
                        let mut p = q + 5;
                        if f & 1 != 0 {
                            st.vertical = b[p] == 1;
                            p += 1;
                        }
                        if f & 2 == 0 {
                            for k in 0..4 {
                                if f & (4 << k) != 0 && p + 2 <= q + 4 + l {
                                    st.pad[k] = u16::from_be_bytes([b[p], b[p + 1]]).min(3000);
                                    p += 2;
                                }
                            }
                        }
                    }
                    // 6004: 03, u8, ff, then u32 the character size in
                    // 1/100 mm (370 for 10.5 pt; 1199 for a 34 pt 学校だより)
                    if t == 0x6004 && l >= 7 && q + 11 <= end && b[q + 4] == 3 {
                        let v = u32::from_be_bytes([b[q + 7], b[q + 8], b[q + 9], b[q + 10]]);
                        st.size = Some(v).filter(|v| (100..=10000).contains(v));
                    }
                    // 6007: 02, u16 line type (1-8 seen on boxes with a line)
                    if t == 0x6007 && l >= 3 && q + 7 <= end && b[q + 4] == 2 {
                        st.line = Some(u16::from_be_bytes([b[q + 5], b[q + 6]]));
                    }
                }
                q += 4 + l;
            }
            o = end;
        } else {
            // 0401: u32 ×5 (the second is the line width and the third the
            // corner radius, 1/100 mm), then the line colour 00 BB GG RR
            let col = |v: u32| {
                let c = v.to_be_bytes();
                format!("#{:02x}{:02x}{:02x}", c[3], c[2], c[1])
            };
            // the fourth and fifth are the shadow's offset, then the shadow colour
            if tag == 0x0401 && len >= 28 {
                if let (Some(st), Some(r), Some(w), Some(c), Some(sx), Some(sy), Some(sc)) =
                    (out.last_mut(), be32(o + 12), be32(o + 8), be32(o + 24), be32(o + 16), be32(o + 20), be32(o + 28))
                {
                    st.shape = Some((r, w, col(c), sx, sy, col(sc)));
                }
            }
            // 0403: u32 fill type (0 none, 1 one colour, 2 gradient), colour
            if tag == 0x0403 && len >= 8 {
                if let (Some(st), Some(t), Some(c)) = (out.last_mut(), be32(o + 4), be32(o + 8)) {
                    st.fill_type = t;
                    st.fill_color = col(c);
                }
            }
            // 0404: u32 ?, u32 angle, 8 bytes, u32 stop count, then (u32 % , colour)
            if tag == 0x0404 && len >= 20 {
                if let (Some(st), Some(a), Some(n)) = (out.last_mut(), be32(o + 8), be32(o + 20)) {
                    st.angle = a as f32;
                    for k in 0..(n as usize).min(16) {
                        if let (Some(p), Some(c)) = (be32(o + 24 + 8 * k), be32(o + 28 + 8 * k)) {
                            st.stops.push(((p as f32 / 100.0).clamp(0.0, 1.0), col(c)));
                        }
                    }
                }
            }
            o += 4 + len;
        }
    }
    out
}

/// The text boxes of a document, by box number.
pub fn read_boxes(cfb: &Cfb, style_budget: &mut usize) -> Vec<TextBox> {
    let Some(raw) = cfb.read("/LayoutBoxText") else {
        return Vec::new();
    };
    let Ok(subs) = crate::ssmg::substreams_indexed(&raw) else {
        return Vec::new();
    };
    let styles = cfb.read("/LayoutBox").map(|b| box_styles(&b)).unwrap_or_default();
    subs.iter()
        .take(2048)
        .enumerate()
        .map(|(i, (layout, s))| TextBox {
            layout: *layout,
            blocks: crate::doc::blocks_from_textv(s, style_budget).unwrap_or_default(),
            pad: styles.get(i).map_or([200, 200, 100, 100], |s| s.pad),
            vertical: styles.get(i).is_some_and(|s| s.vertical),
            // to the nearest half point (3.69 mm is 10.5 pt)
            size_pt: styles.get(i).and_then(|s| s.size).map(|v| (v as f32 / 100.0 * 72.0 / 25.4 * 2.0).round() / 2.0),
            border: styles.get(i).and_then(|s| {
                // line types seen against 一太郎ビューア: 1-4 dotted, 5 and 8
                // solid, 6 none (an empty box round a picture), larger none;
                // 4, 5 and 8 carry a shadow
                let line = s.line.filter(|&l| matches!(l, 1..=5 | 8));
                let fill: Vec<(f32, String)> = match s.fill_type {
                    1 => vec![(0.0, s.fill_color.clone())],
                    2 if !s.stops.is_empty() => s.stops.clone(),
                    _ => Vec::new(),
                };
                if line.is_none() && fill.is_empty() {
                    return None;
                }
                let (r, w, color, sx, sy, sc) = s.shape.clone()?;
                let shadow = line.filter(|l| matches!(l, 4 | 5 | 8)).map(|_| ((sx as f32 / 100.0).clamp(0.0, 5.0), (sy as f32 / 100.0).clamp(0.0, 5.0), sc));
                Some(Border {
                    color,
                    width_mm: (w as f32 / 100.0).clamp(0.1, 1.0),
                    radius_mm: (r as f32 / 100.0).clamp(0.0, 20.0),
                    dotted: line.is_some_and(|l| l <= 4),
                    line: line.is_some(),
                    fill,
                    angle: s.angle,
                    shadow,
                })
            }),
        })
        .collect()
}
