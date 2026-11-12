//! Drawings that Ichitaro keeps as vectors, turned into SVG pictures.
//!
//! - **Figures (図形, frames of kind 4)**: `/FigureData/main_data/FDMVector`
//!   and `/FigureData/layout_data n/FDMVector`. Each top-level record is one
//!   figure. A record is `u8 flag, u8, u16 type (LE), u16 length (BE, from
//!   the record start), u16 count (BE)`:
//!   - `600b` group: 12 bytes, bbox (4 × i32 BE: x1 y1 x2 y2), 16 bytes,
//!     then `count` u16 offsets of the children (from the group's start).
//!   - `600a` shape: u32, u32 fill pattern (0 = solid), u16 line width,
//!     2 bytes, bbox, fill colour, line colour (`00 BB GG RR`), then u16
//!     offsets (from the shape's start) of its outlines. Each outline starts
//!     with 2 flag bytes, then `u16 type (LE), u16 length (BE)`.
//!   - outlines: `6004` ellipse (centre i32 × 2 at +10, radii u16 × 2 at
//!     +18), `6006` polygon and `6009` curve (u32 point count at +12, the
//!     points from +16, then for a curve two control vectors per segment,
//!     relative to the segment's ends, and one flag byte per segment:
//!     0x30 curve, 0x02 an edge that is filled but not stroked).
//!   Units are 1/40 mm (a figure's bbox × 2.5 is its frame's size in
//!   1/100 mm, which is how a frame finds its figure).
//! - **Pictures from other programs (OLE, e.g. JustSystems 花子)**:
//!   `/EmbedItems/Embedding n/EmbeddedPress` (`JSSnapShot32`) is a preview
//!   in a GDI-like record list (`GCI`): from byte 0x80, records `u32 size,
//!   u32 type` (LE). Paths are `4c` begin, `d0` move, `d7` bezier-to
//!   (count, byte length, points), `d1` close, `4d` end, `46 1` fill and
//!   stroke; `82` makes a brush and `70` a pen (handle, colour), `60`
//!   selects one; `c3` polygon and `c6` bezier outside a path; `c7` (8-bit,
//!   with palette) and `c9` (24-bit with a 1-bit mask) are bitmaps. Units
//!   are 1/100 mm; the drawing's size is at 0x48.
//!
//! Read from public files and compared with 一太郎ビューア (2026-11-12).

use std::fmt::Write as _;

fn be16(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*b.get(o)?, *b.get(o + 1)?]))
}
fn le16(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*b.get(o)?, *b.get(o + 1)?]))
}
fn bei(b: &[u8], o: usize) -> Option<i32> {
    Some(i32::from_be_bytes([*b.get(o)?, *b.get(o + 1)?, *b.get(o + 2)?, *b.get(o + 3)?]))
}
fn lei(b: &[u8], o: usize) -> Option<i32> {
    Some(i32::from_le_bytes([*b.get(o)?, *b.get(o + 1)?, *b.get(o + 2)?, *b.get(o + 3)?]))
}

/// Upper bounds against crafted files: commands per drawing, nesting.
const MAX_CMDS: usize = 200_000;
const MAX_DEPTH: usize = 16;

/// A figure ready to show: its size (1/100 mm) and SVG text.
#[derive(Debug, Clone)]
pub struct Figure {
    pub w: u32,
    pub h: u32,
    pub svg: String,
    /// The same drawing in the box of the shape just before it, when it lies
    /// inside that shape: (w, h, svg).
    pub alt: Option<(u32, u32, String)>,
}

struct Out {
    s: String,
    cmds: usize,
    ids: usize,
}

impl Out {
    fn full(&self) -> bool {
        self.cmds > MAX_CMDS || self.s.len() > 8_000_000
    }
}

fn bgr(b: &[u8], o: usize) -> String {
    match (b.get(o + 1), b.get(o + 2), b.get(o + 3)) {
        (Some(bb), Some(g), Some(r)) => format!("#{r:02x}{g:02x}{bb:02x}"),
        _ => "#000000".into(),
    }
}

/// The outline at `q` (its type word) as SVG path data: the filled path and,
/// when some edges are not stroked, the stroked one.
fn outline(b: &[u8], q: usize, end: usize, out: &mut Out) -> Option<(String, Option<String>)> {
    // 0x63xx are the same outlines as 0x60xx (the petals of a 学校だより's
    // flowers: polygons 0x6306)
    let t = le16(b, q)? & 0xfcff;
    let l = be16(b, q + 2)? as usize;
    let lim = (q + l).min(end).min(b.len());
    match t {
        0x6004 => {
            let (cx, cy) = (bei(b, q + 14)?, bei(b, q + 18)?);
            let (rx, ry) = (be16(b, q + 22)? as i32, be16(b, q + 24)? as i32);
            out.cmds += 4;
            Some((format!("M{} {}a{rx} {ry} 0 1 0 {} 0a{rx} {ry} 0 1 0 {} 0Z", cx - rx, cy, 2 * rx, -2 * rx), None))
        }
        0x6006 | 0x6009 => {
            let n = bei(b, q + 12)?;
            if n < 2 || q + 16 + 8 * n as usize > lim {
                return None;
            }
            let n = n as usize;
            out.cmds += n;
            if out.full() {
                return None;
            }
            let pt = |i: usize| (bei(b, q + 16 + 8 * i).unwrap_or(0), bei(b, q + 20 + 8 * i).unwrap_or(0));
            let mut d = String::new();
            let p0 = pt(0);
            let _ = write!(d, "M{} {}", p0.0, p0.1);
            if t == 0x6006 {
                for i in 1..n {
                    let p = pt(i);
                    let _ = write!(d, "L{} {}", p.0, p.1);
                }
                return Some((d, None));
            }
            let v0 = q + 16 + 8 * n;
            let nv = 2 * (n - 1);
            if v0 + 8 * nv > lim {
                for i in 1..n {
                    let p = pt(i);
                    let _ = write!(d, "L{} {}", p.0, p.1);
                }
                return Some((d, None));
            }
            let v = |i: usize| (bei(b, v0 + 8 * i).unwrap_or(0), bei(b, v0 + 4 + 8 * i).unwrap_or(0));
            let fb = v0 + 8 * nv;
            let has_flags = fb + (n - 1) <= lim;
            let mut e = d.clone();
            let mut gaps = false;
            for i in 0..n - 1 {
                let (p, r) = (pt(i), pt(i + 1));
                let (a, c) = (v(2 * i), v(2 * i + 1));
                let f = if has_flags { b[fb + i] } else { 0x31 };
                let mut seg = String::new();
                if f & 0x30 != 0 && (a != (0, 0) || c != (0, 0)) {
                    let _ = write!(seg, "C{} {} {} {} {} {}", p.0 + a.0, p.1 + a.1, r.0 + c.0, r.1 + c.1, r.0, r.1);
                } else {
                    let _ = write!(seg, "L{} {}", r.0, r.1);
                }
                d.push_str(&seg);
                if f & 0x02 != 0 {
                    let _ = write!(e, "M{} {}", r.0, r.1);
                    gaps = true;
                } else {
                    e.push_str(&seg);
                }
            }
            Some((d, gaps.then_some(e)))
        }
        _ => None,
    }
}

fn figure_rec(b: &[u8], o: usize, depth: usize, out: &mut Out) {
    if depth > MAX_DEPTH || out.full() {
        return;
    }
    let (Some(ty), Some(ln), Some(n)) = (le16(b, o + 2), be16(b, o + 4), be16(b, o + 6)) else { return };
    let ln = ln as usize;
    if ln < 8 || o + ln > b.len() {
        return;
    }
    let end = o + ln;
    let body = o + 8;
    match ty {
        0x600b => {
            for k in 0..n as usize {
                let Some(c) = be16(b, body + 44 + 2 * k) else { break };
                let c = o + c as usize;
                if c > o && c + 8 <= end {
                    figure_rec(b, c, depth + 1, out);
                }
            }
        }
        0x600a => {
            let pat = lei(b, body + 4).unwrap_or(0);
            let lw = le16(b, body + 8).unwrap_or(8).clamp(1, 400);
            let (fill, line) = (bgr(b, body + 28), bgr(b, body + 32));
            let Some(first) = be16(b, body + 36) else { return };
            let mut offs = vec![first as usize];
            let mut r = body + 38;
            while r < o + first as usize && offs.len() < 64 {
                match be16(b, r) {
                    Some(x) => offs.push(x as usize),
                    None => break,
                }
                r += 2;
            }
            // fill: 0 solid, 256 a gradient from the fill colour to the
            // second colour (then there is no line), others a pattern of dots
            // (a grey dotted ellipse behind a handout's title)
            let (fill, line) = match pat {
                0 => (fill, line),
                256 => {
                    out.ids += 1;
                    let id = out.ids;
                    let _ = write!(out.s, "<defs><linearGradient id=\"g{id}\" x1=\"0\" y1=\"0\" x2=\"0\" y2=\"1\"><stop offset=\"0\" stop-color=\"{fill}\"/><stop offset=\"1\" stop-color=\"{line}\"/></linearGradient></defs>");
                    (format!("url(#g{id})"), "none".to_string())
                }
                _ => {
                    out.ids += 1;
                    let id = out.ids;
                    let _ = write!(out.s, "<defs><pattern id=\"p{id}\" width=\"24\" height=\"24\" patternUnits=\"userSpaceOnUse\"><circle cx=\"6\" cy=\"6\" r=\"4.5\" fill=\"{fill}\"/><circle cx=\"18\" cy=\"18\" r=\"4.5\" fill=\"{fill}\"/></pattern></defs>");
                    (format!("url(#p{id})"), line)
                }
            };
            let op = "";
            for off in offs {
                if off + 10 > ln {
                    break;
                }
                let Some((d, e)) = outline(b, o + off + 2, end, out) else { continue };
                match e {
                    None => {
                        let _ = write!(out.s, "<path d=\"{d}\" fill=\"{fill}\"{op} stroke=\"{line}\" stroke-width=\"{lw}\" stroke-linejoin=\"round\" fill-rule=\"evenodd\"/>");
                    }
                    Some(e) => {
                        let _ = write!(out.s, "<path d=\"{d}\" fill=\"{fill}\"{op} fill-rule=\"evenodd\"/><path d=\"{e}\" fill=\"none\" stroke=\"{line}\" stroke-width=\"{lw}\" stroke-linejoin=\"round\"/>");
                    }
                }
            }
        }
        // a line on its own: u16 width at +8, colour (00 BB GG RR) at +10
        0x6004 | 0x6006 | 0x6009 | 0x6304 | 0x6306 | 0x6309 => {
            if let Some((d, e)) = outline(b, o + 2, end, out) {
                let (lw, line) = (be16(b, o + 8).unwrap_or(8).clamp(1, 400), bgr(b, o + 10));
                let _ = write!(out.s, "<path d=\"{}\" fill=\"none\" stroke=\"{line}\" stroke-width=\"{lw}\" stroke-linejoin=\"round\"/>", e.unwrap_or(d));
            }
        }
        // 6001: a straight line, x1 y1 x2 y2 from +16; 6005: an open
        // line, u32 point count at +14, the points from +18
        0x6001 | 0x6301 | 0x6005 | 0x6305 => {
            let (lw, line) = (be16(b, o + 8).unwrap_or(8).clamp(1, 400), bgr(b, o + 10));
            let (n, at) = if ty & 0xff == 0x01 { (2usize, o + 16) } else { (bei(b, o + 14).unwrap_or(0).clamp(0, 100_000) as usize, o + 18) };
            if n >= 2 && at + 8 * n <= end {
                out.cmds += n;
                let mut d = String::new();
                for i in 0..n {
                    let (x, y) = (bei(b, at + 8 * i).unwrap_or(0), bei(b, at + 4 + 8 * i).unwrap_or(0));
                    let _ = write!(d, "{}{x} {y}", if i == 0 { "M" } else { "L" });
                }
                let _ = write!(out.s, "<path d=\"{d}\" fill=\"none\" stroke=\"{line}\" stroke-width=\"{lw}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/>");
            }
        }
        _ => {}
    }
}

/// Bounding box of a top-level figure record (1/40 mm).
fn figure_box(b: &[u8], o: usize) -> Option<(i32, i32, i32, i32)> {
    let ty = le16(b, o + 2)?;
    match ty {
        0x600a | 0x600b => Some((bei(b, o + 20)?, bei(b, o + 24)?, bei(b, o + 28)?, bei(b, o + 32)?)),
        0x6001 | 0x6301 => {
            let (x1, y1, x2, y2) = (bei(b, o + 16)?, bei(b, o + 20)?, bei(b, o + 24)?, bei(b, o + 28)?);
            Some((x1.min(x2), y1.min(y2), x1.max(x2), y1.max(y2)))
        }
        0x6005 | 0x6305 | 0x6006 | 0x6009 | 0x6306 | 0x6309 => {
            let q = o + 2;
            let n = bei(b, q + 12)?.clamp(0, 100_000) as usize;
            let (mut x1, mut y1, mut x2, mut y2) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
            for i in 0..n {
                let (x, y) = (bei(b, q + 16 + 8 * i)?, bei(b, q + 20 + 8 * i)?);
                (x1, y1, x2, y2) = (x1.min(x), y1.min(y), x2.max(x), y2.max(y));
            }
            // a curve bulges past its points: its frame is the curve's own
            // bounds (sampled along each segment)
            if ty & 0xff == 0x09 && n >= 2 {
                let v0 = q + 16 + 8 * n;
                let fb = v0 + 16 * (n - 1);
                let end = o + be16(b, o + 4)? as usize;
                if fb <= end {
                    let pt = |i: usize| (bei(b, q + 16 + 8 * i).unwrap_or(0) as f64, bei(b, q + 20 + 8 * i).unwrap_or(0) as f64);
                    let v = |i: usize| (bei(b, v0 + 8 * i).unwrap_or(0) as f64, bei(b, v0 + 4 + 8 * i).unwrap_or(0) as f64);
                    for i in 0..n - 1 {
                        let f = if fb + n - 1 <= end { b[fb + i] } else { 0x31 };
                        if f & 0x30 == 0 {
                            continue;
                        }
                        let (p0, p3) = (pt(i), pt(i + 1));
                        let (a, c) = (v(2 * i), v(2 * i + 1));
                        let (p1, p2) = ((p0.0 + a.0, p0.1 + a.1), (p3.0 + c.0, p3.1 + c.1));
                        for k in 1..16 {
                            let t = k as f64 / 16.0;
                            let u = 1.0 - t;
                            let bx = u * u * u * p0.0 + 3.0 * u * u * t * p1.0 + 3.0 * u * t * t * p2.0 + t * t * t * p3.0;
                            let by = u * u * u * p0.1 + 3.0 * u * u * t * p1.1 + 3.0 * u * t * t * p2.1 + t * t * t * p3.1;
                            (x1, y1, x2, y2) = (x1.min(bx as i32), y1.min(by as i32), x2.max(bx as i32), y2.max(by as i32));
                        }
                    }
                }
            }
            (n > 0).then_some((x1, y1, x2, y2))
        }
        _ => None,
    }
}

/// Every figure of one `FDMVector` stream, in order.
pub fn figures(b: &[u8]) -> Vec<Figure> {
    let mut v = Vec::new();
    let mut o = 0usize;
    let mut prev: Option<(i32, i32, i32, i32)> = None;
    let make = |bx: (i32, i32, i32, i32), body: &str| -> Option<(u32, u32, String)> {
        let (x1, y1, x2, y2) = bx;
        let (w, h) = ((x2 as i64 - x1 as i64).max(1), (y2 as i64 - y1 as i64).max(1));
        if w >= 4_000_000 || h >= 4_000_000 {
            return None;
        }
        // lines a little past the box stay visible
        let pad = 20;
        let svg = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{} {} {} {}\" width=\"{}\" height=\"{}\" preserveAspectRatio=\"none\">{}</svg>",
            x1 as i64 - pad, y1 as i64 - pad, w + 2 * pad, h + 2 * pad,
            (w / 4).max(1), (h / 4).max(1), body
        );
        Some(((w as f64 * 2.5) as u32, (h as f64 * 2.5) as u32, svg))
    };
    while o + 8 <= b.len() && v.len() < 4096 {
        let Some(ln) = be16(b, o + 4) else { break };
        let ln = ln as usize;
        if ln < 8 || o + ln > b.len() {
            break;
        }
        if let Some(bx) = figure_box(b, o) {
            let ty = le16(b, o + 2).unwrap_or(0) & 0xfcff;
            let mut out = Out { s: String::new(), cmds: 0, ids: 0 };
            figure_rec(b, o, 0, &mut out);
            if let Some((w, h, svg)) = make(bx, &out.s) {
                // a bare outline drawn over the shape before it (the black
                // outline of a filled flower or carp streamer) may have that
                // shape's frame: its own box is smaller
                let inside = |p: (i32, i32, i32, i32)| bx.0 >= p.0 - 8 && bx.1 >= p.1 - 8 && bx.2 <= p.2 + 8 && bx.3 <= p.3 + 8;
                let alt = match prev {
                    Some(p) if matches!(ty, 0x6001 | 0x6005 | 0x6006 | 0x6009) && inside(p) && p != bx => make(p, &out.s),
                    _ => None,
                };
                v.push(Figure { w, h, svg, alt });
            }
            if matches!(ty, 0x600a | 0x600b) {
                prev = Some(bx);
            }
        }
        o += ln;
    }
    v
}

// ------------------------------------------------------------------ snapshots

/// The preview (`EmbeddedPress`, `JSSnapShot32`) of a picture from another
/// program, as SVG. `None` when it is not in the GCI form or draws nothing.
pub fn snapshot_svg(b: &[u8]) -> Option<String> {
    if !b.starts_with(b"JSSnapShot32") || b.len() < 0x80 {
        return None;
    }
    let (w, h) = (lei(b, 0x48)?, lei(b, 0x4c)?);
    if !(1..=2_000_000).contains(&w) || !(1..=2_000_000).contains(&h) {
        return None;
    }
    let mut out = Out { s: String::new(), cmds: 0, ids: 0 };
    let mut objs: Vec<(i32, bool, String, i32)> = Vec::new(); // handle, is brush, colour, width
    let (mut brush, mut pen): (Option<String>, Option<(String, i32)>) = (None, None);
    let mut path = String::new();
    let mut o = 0x80usize;
    while o + 8 <= b.len() && !out.full() {
        let (Some(sz), Some(ty)) = (lei(b, o), lei(b, o + 4)) else { break };
        let sz = sz as usize;
        if sz < 8 || o + sz > b.len() {
            break;
        }
        let d = &b[o + 8..o + sz];
        let u = |i: usize| lei(d, 4 * i).unwrap_or(0);
        let colour = |i: usize| {
            let v = u(i) as u32;
            format!("#{:02x}{:02x}{:02x}", v & 255, (v >> 8) & 255, (v >> 16) & 255)
        };
        match ty {
            0x82 => {
                objs.retain(|x| x.0 != u(1));
                objs.push((u(1), true, colour(2), 0));
            }
            0x70 => {
                objs.retain(|x| x.0 != u(1));
                objs.push((u(1), false, colour(2), u(3)));
            }
            0x60 => {
                if let Some(x) = objs.iter().find(|x| x.0 == u(0)) {
                    if x.1 {
                        brush = Some(x.2.clone());
                    } else {
                        pen = Some((x.2.clone(), x.3));
                    }
                }
            }
            0x4c => path.clear(),
            0xd0 => {
                let _ = write!(path, "M{} {}", u(0), u(1));
            }
            0xd7 => {
                let n = (u(0).max(0) as usize).min(d.len().saturating_sub(8) / 8);
                out.cmds += n;
                let mut i = 0;
                while i + 2 < n {
                    let p = |k: usize| (u(2 + 2 * k), u(3 + 2 * k));
                    let (a, c, e) = (p(i), p(i + 1), p(i + 2));
                    let _ = write!(path, "C{} {} {} {} {} {}", a.0, a.1, c.0, c.1, e.0, e.1);
                    i += 3;
                }
            }
            0xd1 => path.push('Z'),
            0x46 if u(0) == 1 && !path.is_empty() => {
                let fill = brush.clone().unwrap_or_else(|| "none".into());
                let (sc, sw) = pen.clone().unwrap_or(("none".into(), 0));
                let _ = write!(out.s, "<path d=\"{path}\" fill=\"{fill}\" stroke=\"{sc}\" stroke-width=\"{}\" fill-rule=\"evenodd\"/>", sw.clamp(8, 400));
                path.clear();
            }
            0xc3 | 0xc6 => {
                let n = (u(0).max(0) as usize).min(d.len().saturating_sub(12) / 8);
                if n >= 2 {
                    out.cmds += n;
                    let p = |k: usize| (u(3 + 2 * k), u(4 + 2 * k));
                    let mut s = format!("M{} {}", p(0).0, p(0).1);
                    if ty == 0xc3 {
                        for k in 1..n {
                            let _ = write!(s, "L{} {}", p(k).0, p(k).1);
                        }
                        s.push('Z');
                    } else {
                        let mut k = 1;
                        while k + 2 < n + 1 && k + 2 < n {
                            let _ = write!(s, "C{} {} {} {} {} {}", p(k).0, p(k).1, p(k + 1).0, p(k + 1).1, p(k + 2).0, p(k + 2).1);
                            k += 3;
                        }
                    }
                    let fill = if ty == 0xc3 { brush.clone().unwrap_or_else(|| "none".into()) } else { "none".into() };
                    let (sc, sw) = pen.clone().unwrap_or(("#000000".into(), 0));
                    let _ = write!(out.s, "<path d=\"{s}\" fill=\"{fill}\" stroke=\"{sc}\" stroke-width=\"{}\"/>", sw.clamp(8, 400));
                }
            }
            0xc7 | 0xc9 => {
                if let Some((x, y, bw, bh, png)) = snapshot_bitmap(d, ty) {
                    let _ = write!(
                        out.s,
                        "<image x=\"{x}\" y=\"{y}\" width=\"{bw}\" height=\"{bh}\" preserveAspectRatio=\"none\" image-rendering=\"optimizeSpeed\" style=\"image-rendering:pixelated\" href=\"data:image/png;base64,{}\"/>",
                        base64(&png)
                    );
                }
            }
            _ => {}
        }
        o += sz;
    }
    if out.s.is_empty() {
        return None;
    }
    Some(format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {w} {h}\" width=\"{}\" height=\"{}\" preserveAspectRatio=\"none\">{}</svg>",
        (w / 10).max(1),
        (h / 10).max(1),
        out.s
    ))
}

/// A bitmap record of a snapshot: place (1/100 mm) and the picture as PNG.
/// `c7`: rect, data size, then the image block. `c9`: rect, colour size,
/// mask size, a raster op, then the colour block and the mask block. A block
/// is 64 bytes (width at +0x10, height +0x14, row length +0x18, bits per
/// pixel +0x20, pixel offset +0x30 when not 0), a palette for 8 bits and
/// less, then the rows from the top.
fn snapshot_bitmap(d: &[u8], ty: i32) -> Option<(i32, i32, i32, i32, Vec<u8>)> {
    let (x, y, w, h) = (lei(d, 0)?, lei(d, 4)?, lei(d, 8)?, lei(d, 12)?);
    let (blk, size, msize) = if ty == 0xc7 {
        (0x14usize, lei(d, 0x10)? as usize, 0usize)
    } else {
        (0x1cusize, lei(d, 0x10)? as usize, lei(d, 0x14)? as usize)
    };
    let pw = lei(d, blk + 0x10)?;
    let ph = lei(d, blk + 0x14)?;
    let stride = lei(d, blk + 0x18)? as usize;
    let bpp = lei(d, blk + 0x20)?;
    if !(1..=8000).contains(&pw) || !(1..=8000).contains(&ph) || (pw as u64 * ph as u64) > 16_000_000 {
        return None;
    }
    let (pw, ph) = (pw as usize, ph as usize);
    let pal_n = if bpp <= 8 { 1usize << bpp } else { 0 };
    let off = match lei(d, blk + 0x30)? as usize {
        0 => blk + 64 + 4 * pal_n,
        v => blk + v,
    };
    if !matches!(bpp, 1 | 4 | 8 | 24 | 32) || stride == 0 || off + stride * ph > d.len() {
        return None;
    }
    let pal = |i: usize| {
        let p = blk + 64 + 4 * i;
        (d.get(p + 2).copied().unwrap_or(0), d.get(p + 1).copied().unwrap_or(0), d.get(p).copied().unwrap_or(0))
    };
    let mut rgba = vec![0u8; pw * ph * 4];
    for r in 0..ph {
        let row = &d[off + r * stride..off + (r + 1) * stride];
        for c in 0..pw {
            let (rr, gg, bb) = match bpp {
                24 => (row.get(3 * c + 2).copied().unwrap_or(0), row.get(3 * c + 1).copied().unwrap_or(0), row.get(3 * c).copied().unwrap_or(0)),
                32 => (row.get(4 * c + 2).copied().unwrap_or(0), row.get(4 * c + 1).copied().unwrap_or(0), row.get(4 * c).copied().unwrap_or(0)),
                8 => pal(row.get(c).copied().unwrap_or(0) as usize),
                4 => pal(((row.get(c / 2).copied().unwrap_or(0) >> (4 * (1 - c % 2))) & 15) as usize),
                _ => pal(((row.get(c / 8).copied().unwrap_or(0) >> (7 - c % 8)) & 1) as usize),
            };
            let p = (r * pw + c) * 4;
            rgba[p..p + 4].copy_from_slice(&[rr, gg, bb, 255]);
        }
    }
    // the mask: a set bit is drawn, a clear bit shows what is behind
    if msize > 0 {
        let mb = blk + size;
        if let (Some(ms), Some(mo)) = (lei(d, mb + 0x18), lei(d, mb + 0x30)) {
            let (ms, mo) = (ms as usize, mb + mo as usize);
            if ms > 0 && mo + ms * ph <= d.len() {
                for r in 0..ph {
                    for c in 0..pw {
                        if (d[mo + r * ms + c / 8] >> (7 - c % 8)) & 1 == 0 {
                            rgba[(r * pw + c) * 4 + 3] = 0;
                        }
                    }
                }
            }
        }
    }
    Some((x, y, w, h, png_rgba(pw as u32, ph as u32, &rgba)))
}

// ------------------------------------------------------------------ PNG

fn crc32(chunks: &[&[u8]]) -> u32 {
    let mut c = 0xffff_ffffu32;
    for data in chunks {
        for &x in *data {
            c ^= x as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
            }
        }
    }
    !c
}

/// An RGBA picture as PNG, stored without compression (deflate "stored"
/// blocks): simple, and the browser draws it the same.
pub fn png_rgba(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
    let mut raw = Vec::with_capacity(rgba.len() + h as usize);
    for r in 0..h as usize {
        raw.push(0);
        raw.extend_from_slice(&rgba[r * w as usize * 4..(r + 1) * w as usize * 4]);
    }
    let mut z = vec![0x78, 0x01];
    let mut chunks = raw.chunks(65535).peekable();
    if chunks.peek().is_none() {
        z.extend_from_slice(&[1, 0, 0, 0xff, 0xff]);
    }
    while let Some(c) = chunks.next() {
        z.push(if chunks.peek().is_none() { 1 } else { 0 });
        let n = c.len() as u16;
        z.extend_from_slice(&n.to_le_bytes());
        z.extend_from_slice(&(!n).to_le_bytes());
        z.extend_from_slice(c);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &x in &raw {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut chunk = |name: &[u8], data: &[u8]| {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend_from_slice(name);
        out.extend_from_slice(data);
        out.extend_from_slice(&crc32(&[name, data]).to_be_bytes());
    };
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(b"IHDR", &ihdr);
    chunk(b"IDAT", &z);
    chunk(b"IEND", &[]);
    out
}

fn base64(b: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(b.len() * 4 / 3 + 4);
    for c in b.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        s.push(T[(n >> 18) as usize & 63] as char);
        s.push(T[(n >> 12) as usize & 63] as char);
        s.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        s.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_has_signature_and_end() {
        let p = png_rgba(2, 1, &[255, 0, 0, 255, 0, 0, 255, 0]);
        assert!(p.starts_with(b"\x89PNG"));
        assert!(p.ends_with(&[0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82]));
    }

    #[test]
    fn base64_pads() {
        assert_eq!(base64(b"ab"), "YWI=");
        assert_eq!(base64(b"abc"), "YWJj");
    }

    #[test]
    fn crafted_figures_stop() {
        // a group that points at itself, and a record longer than the data
        let mut b = vec![1, 0, 0x0b, 0x60, 0, 60, 0, 2];
        b.resize(60, 0);
        b[52] = 0;
        b[53] = 0;
        assert!(figures(&b).len() <= 1);
        assert!(figures(&[1, 0, 0x0a, 0x60, 0xff, 0xff, 0, 1]).is_empty());
    }
}
