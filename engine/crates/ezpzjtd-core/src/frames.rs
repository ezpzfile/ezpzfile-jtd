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
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Picture {
    /// The frame id that shows this picture
    pub item: u16,
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
        let Some(c) = cfb.read(&format!("/EmbedItems/Embedding {n}/Contents")) else { continue };
        if let Some((mime, data)) = picture_in(&c) {
            total += data.len();
            if total > max_bytes {
                break;
            }
            out.push(Picture { item: f.id, mime, data });
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
    fn anchor_record() {
        assert_eq!(anchor_id(&[0, 0x30, 0xffff, 0x507, 0x12, 5, 0]), Some(5));
        assert_eq!(anchor_id(&[0, 0x31, 0xffff, 0x507, 0x12, 5, 0]), None);
    }
}
