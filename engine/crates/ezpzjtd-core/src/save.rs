//! Saving as Ichitaro (.jtd) by patching the original file.
//!
//! We do not regenerate a document from the model: much of `/DocumentText`
//! (line headers, ruled-line geometry, hidden fields, …) is not understood
//! yet and would be lost. Instead:
//!
//! 1. The original text is parsed again *with source positions*
//!    ([`doc::blocks_tracked`]).
//! 2. The original and the edited model are flattened into item sequences
//!    (characters, paragraph ends, table rows/cells) and diffed.
//! 3. Kept items keep their original units byte for byte. Deleted items lose
//!    their units; inserted items get new units at an anchor next to their
//!    neighbours. Changed character formatting rewrites only the affected
//!    properties of the style state.
//! 4. Paragraph alignment is fixed up on line headers (class 0x10 records).
//! 5. The result is parsed again and compared with the edited model. If
//!    anything differs the save is refused, so a wrong file is never written.
//!
//! Layout caches that point into the text by position (`/LineMark`,
//! `/PageMark`, `/DocumentTextPositionTables`) are removed when the text
//! changes, and every `\x04JSRV_SegmentInformation` gets the new stream
//! sizes. Ichitaro Viewer opens the result; see `experiments/results.md`.
//!
//! Research switches: `EZPZJTD_SAVE_DEBUG=1` prints the diff and the tokens
//! around the first change; `EZPZJTD_FORCE_REWRITE=1` rewrites a sheet even
//! when nothing changed.

use std::collections::{BTreeMap, HashMap};

use crate::cfb::Cfb;
use crate::cfbw::{self, Tree};
use crate::doc::{self, Align, Block, Document, Indent, LineFeed, Paragraph};
use crate::edit::normalize_blocks;
use crate::error::{Error, Result};
use crate::jtdw::ssmg_pack;
use crate::ssmg;
use crate::style::{self, CharStyle, RawProps, StyleEvent, StyleMap};
use crate::text;

/// What a save produced.
#[derive(Debug)]
pub struct Saved {
    pub bytes: Vec<u8>,
    /// Things that could not be stored (e.g. italic has no known property).
    pub warnings: Vec<String>,
    /// Number of sheets whose text changed.
    pub changed: usize,
}

/// Save `doc` (an edited version of `original`) as a .jtd file.
pub fn save(original: &[u8], doc: &Document) -> Result<Saved> {
    let cfb = Cfb::open(original.to_vec())?;
    let mut tree = Tree::from_cfb(&cfb);
    let mut warnings = Vec::new();
    let mut changed = 0;
    for sheet in &doc.sheets {
        let base = if sheet.path == "/" {
            String::new()
        } else {
            sheet.path.clone()
        };
        let path = format!("{base}/DocumentText");
        let raw = cfb
            .read(&path)
            .ok_or_else(|| Error::Unsupported(format!("{path} is missing in the original")))?;
        let mut blocks1 = sheet.blocks.clone();
        // empty lines at the very end are not kept by the reader either
        while blocks1.len() > 1
            && matches!(blocks1.last(), Some(Block::Paragraph(p)) if p.is_empty() && !p.page_break_before)
        {
            blocks1.pop();
        }
        normalize_blocks(&mut blocks1);
        match save_sheet(&raw, &blocks1, &mut warnings)? {
            None => {}
            Some((bytes, units_changed)) => {
                changed += 1;
                tree.set_stream(&path, bytes);
                if units_changed {
                    for cache in ["LineMark", "PageMark", "DocumentTextPositionTables"] {
                        tree.remove(&format!("{base}/{cache}"));
                    }
                }
            }
        }
    }
    let bytes = if changed == 0 {
        original.to_vec()
    } else {
        tree.fix_segment_info();
        cfbw::write(&tree)
    };
    Ok(Saved {
        bytes,
        warnings,
        changed,
    })
}

// ------------------------------------------------------------------ items

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum It {
    Ch(char),
    End,
    Brk,
    TabStart,
    /// A table line with this many cells.
    RowStart(u16),
    /// A cell with these left / right edges.
    CellStart(u16, u16),
    RowEnd,
    TabEnd,
}

/// An item of the original document with its source anchors.
#[derive(Clone, Debug)]
struct I0 {
    it: It,
    /// Text inserted before this item goes here.
    before: u32,
    /// Text inserted after this item goes here.
    after: u32,
    /// Units removed when the item is deleted.
    del: (u32, u32),
    /// Flat paragraph index (characters, ends, page breaks).
    para: usize,
    /// Character index within the paragraph.
    ch: usize,
    group: u32,
    /// Row items: (block, row) in the original.
    row: (usize, usize),
    /// Deleting this page break must leave a 000A (it ended a paragraph).
    keep_as_para_end: bool,
    /// End of a paragraph that has no units (added by `normalize_blocks`).
    synthetic: bool,
}

/// An item of the edited document.
#[derive(Clone, Copy, Debug)]
struct I1 {
    it: It,
    para: usize,
    ch: usize,
    row: (usize, usize),
    cell: usize,
}

const NONE: usize = usize::MAX;

fn flat_paras(blocks: &[Block]) -> Vec<&Paragraph> {
    let mut v = Vec::new();
    for b in blocks {
        match b {
            Block::Paragraph(p) => v.push(p),
            Block::Table(t) => {
                for r in &t.rows {
                    for c in &r.cells {
                        v.extend(c.paragraphs.iter());
                    }
                }
            }
        }
    }
    v
}

/// Per-character styles of a paragraph, in plain-text order.
fn char_looks(p: &Paragraph) -> Vec<&CharStyle> {
    let mut v = Vec::new();
    for r in &p.runs {
        for _ in r.text.chars() {
            v.push(&r.style);
        }
    }
    v
}

fn flatten0(blocks: &[Block]) -> Result<Vec<I0>> {
    let mut out = Vec::new();
    let mut last = 0u32;
    let mut pi = 0usize;
    let mut prev_end: Option<(u32, bool)> = None;
    let para = |p: &Paragraph,
                pi: usize,
                row: (usize, usize),
                out: &mut Vec<I0>,
                last: &mut u32,
                prev_end: &mut Option<(u32, bool)>|
     -> Result<()> {
        let src = p.src.get();
        let blank = I0 {
            it: It::End,
            before: *last,
            after: *last,
            del: (0, 0),
            para: pi,
            ch: NONE,
            group: u32::MAX,
            row,
            keep_as_para_end: false,
            synthetic: false,
        };
        if p.page_break_before {
            if let Some(pb) = src.and_then(|s| s.page_break) {
                let ended_prev = matches!(*prev_end, Some((e, false)) if e == pb);
                out.push(I0 {
                    it: It::Brk,
                    before: pb,
                    after: pb + 1,
                    del: (pb, 1),
                    keep_as_para_end: ended_prev,
                    ..blank.clone()
                });
                *last = pb + 1;
            } else {
                out.push(I0 {
                    it: It::Brk,
                    ..blank.clone()
                });
            }
        }
        let text = p.plain_text();
        if let Some(s) = src {
            if s.chars.len() != text.chars().count() {
                return Err(Error::Corrupt("source map does not match the text".into()));
            }
        } else if !text.is_empty() {
            return Err(Error::Corrupt("paragraph without source position".into()));
        }
        for (k, c) in text.chars().enumerate() {
            let cs = src.unwrap().chars[k];
            out.push(I0 {
                it: It::Ch(c),
                before: cs.before,
                after: cs.after,
                del: (cs.unit, cs.len as u32),
                ch: k,
                group: cs.group,
                ..blank.clone()
            });
            *last = cs.after;
        }
        match src {
            Some(s) => {
                let after = s.end + s.end_explicit as u32;
                out.push(I0 {
                    it: It::End,
                    before: s.end,
                    after,
                    del: if s.end_explicit { (s.end, 1) } else { (0, 0) },
                    ..blank
                });
                *last = after;
                *prev_end = Some((s.end, s.end_explicit));
            }
            None => out.push(I0 {
                synthetic: true,
                ..blank
            }),
        }
        Ok(())
    };
    for (bi, b) in blocks.iter().enumerate() {
        match b {
            Block::Paragraph(p) => {
                para(p, pi, (NONE, NONE), &mut out, &mut last, &mut prev_end)?;
                pi += 1;
            }
            Block::Table(t) => {
                let start = t
                    .rows
                    .first()
                    .and_then(|r| r.src.get())
                    .map(|s| s.header.0)
                    .unwrap_or(last);
                let item = |it, before, after, del, row| I0 {
                    it,
                    before,
                    after,
                    del,
                    para: NONE,
                    ch: NONE,
                    group: u32::MAX,
                    row,
                    keep_as_para_end: false,
                    synthetic: false,
                };
                out.push(item(It::TabStart, start, start, (0, 0), (bi, NONE)));
                for (ri, r) in t.rows.iter().enumerate() {
                    let rs = r
                        .src
                        .get()
                        .ok_or_else(|| Error::Corrupt("row without source".into()))?;
                    let (hs, hl) = rs.header;
                    out.push(item(
                        It::RowStart(r.cells.len() as u16),
                        hs,
                        hs + hl,
                        (hs, hl),
                        (bi, ri),
                    ));
                    last = hs + hl;
                    for c in &r.cells {
                        let cell_it = It::CellStart(c.left, c.right);
                        let cs = c
                            .src
                            .get()
                            .ok_or_else(|| Error::Corrupt("cell without source".into()))?;
                        let (s, l) = cs.rec;
                        out.push(item(cell_it, s, s + l, (s, l), (bi, ri)));
                        last = s + l;
                        for p in &c.paragraphs {
                            para(p, pi, (bi, ri), &mut out, &mut last, &mut prev_end)?;
                            pi += 1;
                        }
                    }
                    match rs.end {
                        Some(e) => {
                            out.push(item(It::RowEnd, e, e + 1, (e, 1), (bi, ri)));
                            last = e + 1;
                        }
                        None => out.push(item(It::RowEnd, last, last, (0, 0), (bi, ri))),
                    }
                }
                out.push(item(It::TabEnd, last, last, (0, 0), (bi, NONE)));
            }
        }
    }
    Ok(out)
}

fn flatten1(blocks: &[Block]) -> Vec<I1> {
    let mut out = Vec::new();
    let mut pi = 0usize;
    let para = |p: &Paragraph, pi: usize, out: &mut Vec<I1>, row, cell, last: bool| {
        let it = |it, ch| I1 {
            it,
            para: pi,
            ch,
            row,
            cell,
        };
        if p.page_break_before {
            out.push(it(It::Brk, NONE));
        }
        for (k, c) in p.plain_text().chars().enumerate() {
            out.push(it(It::Ch(c), k));
        }
        let _ = last;
        out.push(it(It::End, NONE));
    };
    for (bi, b) in blocks.iter().enumerate() {
        match b {
            Block::Paragraph(p) => {
                para(p, pi, &mut out, (NONE, NONE), NONE, false);
                pi += 1;
            }
            Block::Table(t) => {
                let s = |it, row, cell| I1 {
                    it,
                    para: NONE,
                    ch: NONE,
                    row,
                    cell,
                };
                out.push(s(It::TabStart, (bi, NONE), NONE));
                for (ri, r) in t.rows.iter().enumerate() {
                    out.push(s(It::RowStart(r.cells.len() as u16), (bi, ri), NONE));
                    for (ci, c) in r.cells.iter().enumerate() {
                        out.push(s(It::CellStart(c.left, c.right), (bi, ri), ci));
                        for (k, p) in c.paragraphs.iter().enumerate() {
                            para(p, pi, &mut out, (bi, ri), ci, k + 1 == c.paragraphs.len());
                            pi += 1;
                        }
                    }
                    out.push(s(It::RowEnd, (bi, ri), NONE));
                }
                out.push(s(It::TabEnd, (bi, NONE), NONE));
            }
        }
    }
    out
}

// ------------------------------------------------------------------ diff

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Op {
    Eq(usize, usize),
    Del(usize),
    Ins(usize),
}

/// Myers' O(ND) diff with a cap on D. Returns `None` when D exceeds `cap`.
/// Memory is O(D²) (only the diagonals reached at each step are kept).
fn myers<T: PartialEq>(a: &[T], b: &[T], cap: usize) -> Option<Vec<Op>> {
    let (n, m) = (a.len() as isize, b.len() as isize);
    let max = (n + m) as usize;
    let off = max as isize + 1;
    let mut v = vec![0isize; 2 * max + 3];
    // trace[d] = v[-d..=d] before step d
    let mut trace: Vec<Vec<isize>> = Vec::new();
    let mut found = None;
    for d in 0..=max.min(cap) as isize {
        trace.push(v[(off - d) as usize..=(off + d) as usize].to_vec());
        let mut k = -d;
        while k <= d {
            let idx = (k + off) as usize;
            let mut x = if k == -d || (k != d && v[idx - 1] < v[idx + 1]) {
                v[idx + 1]
            } else {
                v[idx - 1] + 1
            };
            let mut y = x - k;
            while x < n && y < m && a[x as usize] == b[y as usize] {
                x += 1;
                y += 1;
            }
            v[idx] = x;
            if x >= n && y >= m {
                found = Some(d);
                break;
            }
            k += 2;
        }
        if found.is_some() {
            break;
        }
    }
    let d_end = found?;
    let at = |d: isize, k: isize| -> isize {
        // v[k] as it was before step d (k within -d..=d)
        let t = &trace[d as usize];
        t[(k + d) as usize]
    };
    let mut ops = Vec::new();
    let (mut x, mut y) = (n, m);
    for d in (1..=d_end).rev() {
        let k = x - y;
        let prev_k = if k == -d || (k != d && at(d, k - 1) < at(d, k + 1)) {
            k + 1
        } else {
            k - 1
        };
        let prev_x = at(d, prev_k);
        let prev_y = prev_x - prev_k;
        while x > prev_x && y > prev_y {
            x -= 1;
            y -= 1;
            ops.push(Op::Eq(x as usize, y as usize));
        }
        if x == prev_x {
            y -= 1;
            ops.push(Op::Ins(y as usize));
        } else {
            x -= 1;
            ops.push(Op::Del(x as usize));
        }
    }
    while x > 0 && y > 0 {
        x -= 1;
        y -= 1;
        ops.push(Op::Eq(x as usize, y as usize));
    }
    ops.reverse();
    Some(ops)
}

/// Diff of the item sequences: common prefix and suffix, then Myers on the
/// middle; very large changes fall back to a paragraph-level diff.
fn diff(a: &[It], b: &[It]) -> Vec<Op> {
    let pre = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let suf = a[pre..]
        .iter()
        .rev()
        .zip(b[pre..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    let (ma, mb) = (&a[pre..a.len() - suf], &b[pre..b.len() - suf]);
    let mid = myers(ma, mb, 2000).unwrap_or_else(|| diff_segments(ma, mb));
    let mut out: Vec<Op> = (0..pre).map(|i| Op::Eq(i, i)).collect();
    out.extend(mid.into_iter().map(|o| match o {
        Op::Eq(x, y) => Op::Eq(x + pre, y + pre),
        Op::Del(x) => Op::Del(x + pre),
        Op::Ins(y) => Op::Ins(y + pre),
    }));
    out.extend((0..suf).map(|i| Op::Eq(a.len() - suf + i, b.len() - suf + i)));
    slide(&mut out, a, b);
    out
}

/// Where a block of inserted (or deleted) items sits between equal items is
/// often ambiguous: inserting a table row next to an identical row can be
/// written many ways. Move each block to the position that keeps rows and
/// paragraphs whole.
fn slide(ops: &mut Vec<Op>, a: &[It], b: &[It]) {
    fn score(v: &[It], s: usize, e: usize) -> i32 {
        let mut sc = 0;
        if matches!(v[s], It::RowStart(_) | It::TabStart) {
            sc += 4;
        }
        if s == 0 || matches!(v[s - 1], It::End | It::RowEnd | It::TabEnd) {
            sc += 2;
        }
        if matches!(v[e - 1], It::End | It::RowEnd | It::TabEnd) {
            sc += 1;
        }
        sc
    }
    let mut k = 0;
    while k < ops.len() {
        let ins = matches!(ops[k], Op::Ins(_));
        let del = matches!(ops[k], Op::Del(_));
        if !ins && !del {
            k += 1;
            continue;
        }
        // a run of one kind only, with consecutive indices
        let idx = |o: &Op| match *o {
            Op::Ins(y) if ins => Some(y),
            Op::Del(x) if del => Some(x),
            _ => None,
        };
        let s0 = k;
        let first = idx(&ops[k]).unwrap();
        let mut e0 = k + 1;
        while e0 < ops.len() && idx(&ops[e0]) == Some(first + (e0 - s0)) {
            e0 += 1;
        }
        let len = e0 - s0;
        let v = if ins { b } else { a };
        let side = |o: &Op| match *o {
            Op::Eq(x, y) => Some(if ins { y } else { x }),
            _ => None,
        };
        // how far the block can move left / right over equal items
        let mut left = 0;
        while s0 > left
            && side(&ops[s0 - 1 - left]).is_some()
            && first >= left + 1
            && v[first - 1 - left] == v[first + len - 1 - left]
        {
            left += 1;
        }
        let mut right = 0;
        while e0 + right < ops.len()
            && side(&ops[e0 + right]).is_some()
            && first + len + right < v.len()
            && v[first + right] == v[first + len + right]
        {
            right += 1;
        }
        let mut best = (score(v, first, first + len), 0isize);
        for sh in -(left as isize)..=(right as isize) {
            let st = (first as isize + sh) as usize;
            let sc = score(v, st, st + len);
            if sc > best.0 {
                best = (sc, sh);
            }
        }
        if best.1 != 0 {
            let sh = best.1;
            let (lo, hi) = if sh < 0 {
                (s0 - (-sh) as usize, e0)
            } else {
                (s0, e0 + sh as usize)
            };
            let eqs: Vec<(usize, usize)> = ops[lo..hi]
                .iter()
                .filter_map(|o| match *o {
                    Op::Eq(x, y) => Some((x, y)),
                    _ => None,
                })
                .collect();
            let nst = (first as isize + sh) as usize;
            let mut seg: Vec<Op> = Vec::new();
            let block = |i: usize| {
                if ins {
                    Op::Ins(nst + i)
                } else {
                    Op::Del(nst + i)
                }
            };
            if sh < 0 {
                // block first, then the equal items it moved over
                seg.extend((0..len).map(block));
                for (x, y) in eqs {
                    seg.push(if ins {
                        Op::Eq(x, y + len)
                    } else {
                        Op::Eq(x + len, y)
                    });
                }
            } else {
                for (x, y) in eqs {
                    seg.push(if ins {
                        Op::Eq(x, y - len)
                    } else {
                        Op::Eq(x - len, y)
                    });
                }
                seg.extend((0..len).map(block));
            }
            ops.splice(lo..hi, seg);
            k = hi;
        } else {
            k = e0;
        }
    }
}

/// Two-level diff: first over paragraph-sized segments, then over items
/// inside the segments that differ.
fn diff_segments(a: &[It], b: &[It]) -> Vec<Op> {
    fn segments(v: &[It]) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        let mut s = 0;
        for (i, it) in v.iter().enumerate() {
            let cut = !matches!(it, It::Ch(_) | It::Brk);
            if cut {
                out.push((s, i + 1));
                s = i + 1;
            }
        }
        if s < v.len() {
            out.push((s, v.len()));
        }
        out
    }
    let (sa, sb) = (segments(a), segments(b));
    let ka: Vec<&[It]> = sa.iter().map(|&(s, e)| &a[s..e]).collect();
    let kb: Vec<&[It]> = sb.iter().map(|&(s, e)| &b[s..e]).collect();
    let seg_ops = myers(&ka, &kb, 4000).unwrap_or_else(|| {
        let mut o: Vec<Op> = (0..ka.len()).map(Op::Del).collect();
        o.extend((0..kb.len()).map(Op::Ins));
        o
    });
    let mut out = Vec::new();
    let mut pend_a: Vec<usize> = Vec::new();
    let mut pend_b: Vec<usize> = Vec::new();
    let flush = |pa: &mut Vec<usize>, pb: &mut Vec<usize>, out: &mut Vec<Op>| {
        if pa.is_empty() && pb.is_empty() {
            return;
        }
        let ia: Vec<usize> = pa.iter().flat_map(|&s| sa[s].0..sa[s].1).collect();
        let ib: Vec<usize> = pb.iter().flat_map(|&s| sb[s].0..sb[s].1).collect();
        let xa: Vec<It> = ia.iter().map(|&i| a[i]).collect();
        let xb: Vec<It> = ib.iter().map(|&i| b[i]).collect();
        match myers(&xa, &xb, 3000) {
            Some(ops) => out.extend(ops.into_iter().map(|o| match o {
                Op::Eq(x, y) => Op::Eq(ia[x], ib[y]),
                Op::Del(x) => Op::Del(ia[x]),
                Op::Ins(y) => Op::Ins(ib[y]),
            })),
            None => {
                out.extend(ia.iter().map(|&i| Op::Del(i)));
                out.extend(ib.iter().map(|&i| Op::Ins(i)));
            }
        }
        pa.clear();
        pb.clear();
    };
    for op in seg_ops {
        match op {
            Op::Eq(x, y) => {
                flush(&mut pend_a, &mut pend_b, &mut out);
                for (i, j) in (sa[x].0..sa[x].1).zip(sb[y].0..sb[y].1) {
                    out.push(Op::Eq(i, j));
                }
            }
            Op::Del(x) => pend_a.push(x),
            Op::Ins(y) => pend_b.push(y),
        }
    }
    flush(&mut pend_a, &mut pend_b, &mut out);
    out
}

// ------------------------------------------------------------------ styles

/// Byte length of each style property value, as seen in the corpus.
fn default_len(id: u8) -> u8 {
    match id {
        4..=7 | 9..=12 => 1,
        15..=17 | 20 => 4,
        _ => 2,
    }
}

/// Table of distinct style states; units refer to them by index.
struct Raws {
    list: Vec<RawProps>,
}

impl Raws {
    fn add(&mut self, r: RawProps) -> u32 {
        self.list.push(r);
        (self.list.len() - 1) as u32
    }
}

fn parse_hex(c: &str) -> Option<u32> {
    let h = c.strip_prefix('#')?;
    let v = u32::from_str_radix(h, 16).ok()?;
    let (r, g, b) = ((v >> 16) & 0xff, (v >> 8) & 0xff, v & 0xff);
    Some(r | (g << 8) | (b << 16))
}

/// Same visible formatting, ignoring properties that cannot be stored.
fn same_storable(a: &CharStyle, b: &CharStyle) -> bool {
    a.bold == b.bold && a.size_pt == b.size_pt && a.underline == b.underline && a.color == b.color
}

/// `raw` with the formatting of `want` applied (only what differs).
fn apply_look(raw: &RawProps, want: &CharStyle) -> RawProps {
    let have = CharStyle::from_raw(raw);
    let mut r = raw.clone();
    if have.bold != want.bold {
        r.insert(1, if want.bold == Some(true) { 1 } else { 0xffff });
    }
    if have.size_pt != want.size_pt {
        r.insert(
            2,
            want.size_pt
                .map(|pt| (pt * 35.2778).round() as u32)
                .unwrap_or(0),
        );
    }
    if have.underline != want.underline {
        r.insert(13, want.underline.map(|u| u as u32).unwrap_or(0));
    }
    if have.color != want.color {
        r.insert(
            15,
            want.color
                .as_deref()
                .and_then(parse_hex)
                .unwrap_or(0xffff_ffff),
        );
    }
    r
}

/// The value that means "not set" for the properties the editor changes.
fn neutral(id: u8) -> Option<u32> {
    match id {
        1 => Some(0xffff),
        2 | 13 => Some(0),
        15 => Some(0xffff_ffff),
        _ => None,
    }
}

fn encode_events(
    units: &[u32],
    raws: &Raws,
    lens: &HashMap<u8, u8>,
    end: bool,
    tail: &[u8],
) -> Vec<u8> {
    let mut out = Vec::new();
    let mut state = RawProps::new();
    let mut run = 0u32;
    let flush = |run: &mut u32, out: &mut Vec<u8>| {
        if *run > 0 {
            out.push(0x00);
            out.extend_from_slice(&run.to_be_bytes());
            *run = 0;
        }
    };
    for &ri in units {
        let r = &raws.list[ri as usize];
        let mut changed: Vec<(u8, u32)> = r
            .iter()
            .filter(|(k, v)| state.get(k) != Some(v))
            .map(|(k, v)| (*k, *v))
            .collect();
        // a property this state never had must be switched back off
        for (k, v) in &state {
            if !r.contains_key(k) {
                if let Some(n) = neutral(*k) {
                    if *v != n {
                        changed.push((*k, n));
                    }
                }
            }
        }
        changed.sort_unstable();
        if changed.is_empty() {
            run += 1;
            continue;
        }
        flush(&mut run, &mut out);
        out.push(0xfe);
        for (k, v) in changed {
            let len = lens
                .get(&k)
                .copied()
                .unwrap_or_else(|| default_len(k))
                .clamp(1, 4);
            out.push(k);
            out.push(len);
            out.extend_from_slice(&v.to_be_bytes()[4 - len as usize..]);
            state.insert(k, v);
        }
        out.extend_from_slice(&[0xff, 0x00]);
    }
    flush(&mut run, &mut out);
    if end {
        out.push(0xff);
    }
    out.extend_from_slice(tail);
    out
}

// ------------------------------------------------------------------ records

fn record(class: u16, payload: &[u16]) -> Vec<u16> {
    let len = (payload.len() + 7) as u16;
    let mut v = vec![text::REC, class, len];
    v.extend_from_slice(payload);
    v.extend_from_slice(&[len, 0, class, text::TEXT_START]);
    v
}

fn payload_of(units: &[u16], (s, l): (u32, u32)) -> Vec<u16> {
    let (s, l) = (s as usize, l as usize);
    units[s + 3..s + l - 4].to_vec()
}

/// An empty line header: no properties.
fn default_header_payload() -> Vec<u16> {
    vec![0, 0xffff, 0]
}

fn tlv_align(payload: &[u16]) -> Align {
    for (tag, v) in text::para_tlv(payload) {
        if tag == 0x24 {
            return match v.first().copied().unwrap_or(0) {
                0 => Align::Left,
                1 => Align::Center,
                2 => Align::Right,
                _ => Align::Other,
            };
        }
    }
    Align::Left
}

fn has_rules(payload: &[u16]) -> bool {
    text::para_tlv(payload).iter().any(|(t, _)| *t == 0x8f)
}

/// Indents (0x26) and line feed (0x20) of a line header payload.
fn tlv_fmt(payload: &[u16]) -> (Option<Indent>, Option<LineFeed>) {
    let (mut i, mut f) = (None, None);
    for (tag, v) in text::para_tlv(payload) {
        match tag {
            0x20 => f = LineFeed::from_tlv(&v),
            0x26 => i = Indent::from_tlv(&v),
            _ => {}
        }
    }
    (i, f)
}

/// Replace, add (in ascending tag order, as Ichitaro writes them) or remove
/// one TLV item of a line header payload.
fn set_tlv(payload: &[u16], tag: u16, val: Option<&[u16]>) -> Vec<u16> {
    let mut items: Vec<(u16, Vec<u16>)> = Vec::new();
    let mut j = 1usize;
    while j + 1 < payload.len() && payload[j] != 0xffff {
        let n = payload[j + 1] as usize;
        let end = (j + 2 + n).min(payload.len());
        items.push((payload[j], payload[j + 2..end].to_vec()));
        j = end;
    }
    items.retain(|(t, _)| *t != tag);
    if let Some(v) = val {
        let at = items
            .iter()
            .position(|(t, _)| *t > tag)
            .unwrap_or(items.len());
        items.insert(at, (tag, v.to_vec()));
    }
    let mut out = vec![payload.first().copied().unwrap_or(0)];
    for (t, v) in items {
        out.push(t);
        out.push(v.len() as u16);
        out.extend(v);
    }
    out.extend_from_slice(&payload[j.min(payload.len())..]);
    out
}

/// Make a line header payload carry these indents and line feed.
fn set_fmt(payload: &[u16], indent: Option<Indent>, feed: Option<LineFeed>) -> Vec<u16> {
    let (i0, f0) = tlv_fmt(payload);
    let mut out = payload.to_vec();
    if i0 != indent {
        out = set_tlv(&out, 0x26, indent.map(|i| i.to_tlv()).as_deref());
    }
    if f0 != feed {
        out = set_tlv(&out, 0x20, feed.map(|f| f.to_tlv()).as_deref());
    }
    out
}

/// Rewrite tag 0x24 (alignment) of a line header payload.
fn set_align(payload: &[u16], align: Align) -> Vec<u16> {
    let val = match align {
        Align::Left => 0,
        Align::Center => 1,
        Align::Right => 2,
        Align::Other => return payload.to_vec(),
    };
    let mut out = vec![payload.first().copied().unwrap_or(0)];
    let mut j = 1usize;
    let mut done = false;
    while j + 1 < payload.len() {
        let tag = payload[j];
        if tag == 0xffff {
            break;
        }
        let n = payload[j + 1] as usize;
        let end = (j + 2 + n).min(payload.len());
        if tag == 0x24 {
            out.extend_from_slice(&[0x24, 1, val]);
            done = true;
        } else {
            out.extend_from_slice(&payload[j..end]);
        }
        j = end;
    }
    if !done {
        // keep tags in ascending order like Ichitaro does
        let mut v = vec![out[0]];
        let mut k = 1usize;
        let mut placed = false;
        while k + 1 < out.len() {
            let n = out[k + 1] as usize;
            if !placed && out[k] > 0x24 {
                v.extend_from_slice(&[0x24, 1, val]);
                placed = true;
            }
            v.extend_from_slice(&out[k..(k + 2 + n).min(out.len())]);
            k += 2 + n;
        }
        if !placed {
            v.extend_from_slice(&[0x24, 1, val]);
        }
        out = v;
    }
    out.extend_from_slice(&payload[j.min(payload.len())..]);
    out
}

// ------------------------------------------------------------------ sheet

struct Text0 {
    units: Vec<u16>,
    /// Style state index per unit.
    ru: Vec<u32>,
    raws: Raws,
    lens: HashMap<u8, u8>,
    tail: Vec<u8>,
    /// The style list ended with an explicit end marker (0xFF).
    had_end: bool,
    /// Sub-streams of the Ssmg container kept as they are.
    rest: Vec<Vec<u8>>,
}

fn read_text(raw: &[u8]) -> Result<Text0> {
    let subs = ssmg::substreams(raw)?;
    let head = &subs[0];
    let mut lens = HashMap::new();
    let mut tail = Vec::new();
    let mut had_end = true;
    let rest: Vec<Vec<u8>>;
    if head.starts_with(b"TextV.01") {
        rest = subs[1..].to_vec();
    } else if head.starts_with(b"QLSTV.01") {
        let k = u32::from_be_bytes([head[8], head[9], head[10], head[11]]) as usize;
        let used: Vec<usize> = (0..k)
            .filter_map(|j| {
                let o = 12 + 8 * j + 4;
                head.get(o..o + 4)
                    .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize)
            })
            .collect();
        rest = subs
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != 0 && !used.contains(i))
            .map(|(_, s)| s.clone())
            .collect();
    } else {
        return Err(Error::Unsupported("unknown DocumentText layout".into()));
    }
    let pieces = ssmg::pieces(raw)?;
    let single = pieces.len() == 1;
    let mut units = Vec::new();
    let mut ru = Vec::new();
    let mut raws = Raws { list: Vec::new() };
    for p in &pieces {
        let (ev, used) = style::parse_events(&p.style);
        for e in &ev {
            if let StyleEvent::Set(props) = e {
                for (id, v) in props {
                    lens.entry(*id).or_insert(v.len() as u8);
                }
            }
        }
        if single {
            tail = p.style[used.min(p.style.len())..].to_vec();
            had_end = ev.iter().any(|e| matches!(e, StyleEvent::End));
        }
        let mut covered = 0usize;
        let mut last = RawProps::new();
        for s in style::spans(&p.style, p.units.len()) {
            let idx = raws.add(s.raw.clone());
            for _ in 0..s.len {
                ru.push(idx);
            }
            covered += s.len;
            last = s.raw;
        }
        if covered < p.units.len() {
            let idx = raws.add(last);
            ru.extend(std::iter::repeat(idx).take(p.units.len() - covered));
        }
        ru.truncate(units.len() + p.units.len());
        units.extend_from_slice(&p.units);
    }
    Ok(Text0 {
        units,
        ru,
        raws,
        lens,
        tail,
        had_end,
        rest,
    })
}

fn style_map(units_len: usize, ru: &[u32], raws: &Raws) -> StyleMap {
    let mut spans: Vec<style::StyleSpan> = Vec::new();
    for (i, &r) in ru.iter().enumerate().take(units_len) {
        match spans.last_mut() {
            Some(s) if s.raw == raws.list[r as usize] => s.len += 1,
            _ => spans.push(style::StyleSpan {
                start: i,
                len: 1,
                raw: raws.list[r as usize].clone(),
            }),
        }
    }
    StyleMap::new(spans)
}

/// Units to insert at a position, with their style state.
struct Ins {
    pos: u32,
    units: Vec<u16>,
    raws: Vec<u32>,
}

/// Returns the new `/DocumentText` and whether the units changed, or `None`
/// when the sheet is unchanged.
fn save_sheet(
    raw: &[u8],
    blocks1: &[Block],
    warnings: &mut Vec<String>,
) -> Result<Option<(Vec<u8>, bool)>> {
    let mut t = read_text(raw)?;
    let map0 = style_map(t.units.len(), &t.ru, &t.raws);
    let (mut blocks0, groups) = doc::blocks_tracked(&t.units, &map0);
    normalize_blocks(&mut blocks0);
    if blocks0 == blocks1 && std::env::var_os("EZPZJTD_FORCE_REWRITE").is_none() {
        return Ok(None);
    }
    let paras0 = flat_paras(&blocks0);
    let paras1 = flat_paras(blocks1);
    let looks0: Vec<Vec<&CharStyle>> = paras0.iter().map(|p| char_looks(p)).collect();
    let looks1: Vec<Vec<&CharStyle>> = paras1.iter().map(|p| char_looks(p)).collect();
    let items0 = flatten0(&blocks0)?;
    let items1 = flatten1(blocks1);
    let a: Vec<It> = items0.iter().map(|i| i.it).collect();
    let b: Vec<It> = items1.iter().map(|i| i.it).collect();
    let ops = diff(&a, &b);

    let n = t.units.len();
    let mut del = vec![false; n];
    let mut repl: HashMap<u32, u16> = HashMap::new();
    let mut restyle: HashMap<u32, u32> = HashMap::new();
    let mut ins: Vec<Ins> = Vec::new();
    let mut group_left: HashMap<u32, usize> = HashMap::new();
    for i in &items0 {
        if i.group != u32::MAX {
            *group_left.entry(i.group).or_default() += 1;
        }
    }
    let mut italic_lost = false;
    let mark = |del: &mut Vec<bool>, (s, l): (u32, u32)| {
        for u in s..s + l {
            if let Some(d) = del.get_mut(u as usize) {
                *d = true;
            }
        }
    };

    // For each run of insertions: the last op that closes a paragraph or a
    // table. Whole paragraphs inserted at a paragraph boundary go before the
    // next paragraph's line headers, so that paragraph keeps them.
    let mut closes_until = vec![None; ops.len()];
    {
        let mut k = 0;
        while k < ops.len() {
            if !matches!(ops[k], Op::Ins(_)) {
                k += 1;
                continue;
            }
            let start = k;
            let mut last = None;
            while k < ops.len() {
                let Op::Ins(y) = ops[k] else { break };
                if matches!(items1[y].it, It::End | It::TabEnd) {
                    last = Some(k);
                }
                k += 1;
            }
            for c in closes_until.iter_mut().take(k).skip(start) {
                *c = last;
            }
        }
    }

    let mut headered: std::collections::HashSet<usize> = Default::default();
    // what the diff removes from each original paragraph
    let mut chars_left: Vec<usize> = paras0
        .iter()
        .map(|p| p.plain_text().chars().count())
        .collect();
    let mut end_deleted = vec![false; paras0.len()];
    let mut rows_deleted: std::collections::HashSet<(usize, usize)> = Default::default();
    for op in &ops {
        if let Op::Del(x) = *op {
            let o = &items0[x];
            match o.it {
                It::Ch(_) => chars_left[o.para] -= 1,
                It::End => end_deleted[o.para] = true,
                It::RowStart(_) => {
                    rows_deleted.insert(o.row);
                }
                _ => {}
            }
        }
    }

    let mut i0 = 0usize; // original items consumed
    for (oi, op) in ops.iter().enumerate() {
        match *op {
            Op::Eq(x, y) => {
                i0 = x + 1;
                let (o, e) = (&items0[x], &items1[y]);
                if o.it == It::End && o.del.1 == 0 {
                    // The paragraph ends without a 000A of its own. The parser only
                    // keeps such a paragraph when it has text, and a paragraph that
                    // only existed in the model has no units at all.
                    let p1 = paras1[e.para];
                    let empty = p1.plain_text().is_empty();
                    let multi = if e.cell != NONE {
                        table_row(blocks1, e.row).cells[e.cell].paragraphs.len() > 1
                    } else {
                        e.para + 1 < paras1.len()
                    };
                    let last_in_cell = e.cell == NONE
                        || table_row(blocks1, e.row).cells[e.cell]
                            .paragraphs
                            .last()
                            .map(|q| std::ptr::eq(q, p1))
                            .unwrap_or(true);
                    let new_after =
                        matches!(ops.get(oi + 1), Some(Op::Ins(y2)) if items1[*y2].para != NONE);
                    let box_line = e.cell == NONE
                        && paras0[o.para].src.get().map(|s| s.in_row).unwrap_or(false);
                    let need = if o.synthetic {
                        !empty || multi
                    } else if box_line {
                        // the reader keeps an empty line of a box by itself
                        new_after
                    } else {
                        (empty && multi) || !last_in_cell || new_after
                    };
                    if need {
                        let base =
                            t.ru.get((o.before as usize).saturating_sub(1))
                                .copied()
                                .unwrap_or(0);
                        if o.synthetic && e.cell == NONE && headered.insert(e.para) {
                            let u = record(0x0010, &default_header_payload());
                            let k = u.len();
                            ins.push(Ins {
                                pos: o.before,
                                units: u,
                                raws: vec![base; k],
                            });
                        }
                        ins.push(Ins {
                            pos: o.before,
                            units: vec![text::PARA_END],
                            raws: vec![base],
                        });
                    }
                }
                if let It::Ch(_) = o.it {
                    // compare with the character's own stored look (an inline run
                    // shows the look of its first character for all of them)
                    let l0 = CharStyle::from_raw(&t.raws.list[t.ru[o.del.0 as usize] as usize]);
                    let l1 = looks1[e.para][e.ch];
                    if l1.italic == Some(true) && looks0[o.para][o.ch].italic != Some(true) {
                        italic_lost = true;
                    }
                    if !same_storable(&l0, l1) {
                        for u in o.del.0..o.del.0 + o.del.1 {
                            let r = apply_look(&t.raws.list[t.ru[u as usize] as usize], l1);
                            let idx = t.raws.add(r);
                            restyle.insert(u, idx);
                        }
                    }
                }
            }
            Op::Del(x) => {
                i0 = x + 1;
                let o = &items0[x];
                match o.it {
                    It::RowStart(_) => {
                        // the whole line: header, cells, contents, 000E
                        let (bi, ri) = o.row;
                        if let Block::Table(tb) = &blocks0[bi] {
                            if let Some(rs) = tb.rows[ri].src.get() {
                                let end =
                                    rs.end.map(|e| e + 1).unwrap_or(rs.header.0 + rs.header.1);
                                mark(&mut del, (rs.header.0, end - rs.header.0));
                            }
                        }
                    }
                    It::Brk if o.keep_as_para_end => {
                        repl.insert(o.del.0, text::PARA_END);
                    }
                    It::End if rows_deleted.contains(&o.row) => mark(&mut del, o.del),
                    It::End => {
                        let a = paras0[o.para].src.get();
                        let b = paras0.get(o.para + 1).and_then(|p| p.src.get());
                        let crossing = match (a, b) {
                            (Some(a), Some(b)) => {
                                let to = b.chars.first().map(|c| c.before).unwrap_or(b.end);
                                a.in_row != b.in_row
                                    || (a.in_row && crosses_line(&t.units, o.after, to))
                            }
                            _ => false,
                        };
                        if crossing {
                            // Joining a ruled-box line with another line would need new
                            // box geometry. A line that ends up empty can go away.
                            let (a, b) = (a.unwrap(), b.unwrap());
                            let (al, bl) = (chars_left[o.para], chars_left[o.para + 1]);
                            if al == 0 && !a.in_row {
                                mark(&mut del, o.del);
                            } else if al == 0 && a.in_row {
                                match box_line(&t.units, a) {
                                    Some(r) => mark(&mut del, r),
                                    None => return Err(box_join_error()),
                                }
                            } else if bl == 0
                                && !b.in_row
                                && b.end_explicit
                                && !end_deleted[o.para + 1]
                            {
                                mark(&mut del, (b.end, 1));
                            } else if bl == 0 && b.in_row {
                                match box_line(&t.units, b) {
                                    Some(r) => mark(&mut del, r),
                                    None => return Err(box_join_error()),
                                }
                            } else {
                                return Err(box_join_error());
                            }
                        } else {
                            mark(&mut del, o.del);
                            // the next paragraph joins this one: drop its own line headers
                            if let (Some(a), Some(b)) = (a, b) {
                                if !a.in_row && !b.in_row {
                                    for h in &b.own_headers {
                                        mark(&mut del, *h);
                                    }
                                }
                            }
                        }
                    }
                    _ => mark(&mut del, o.del),
                }
                if o.group != u32::MAX {
                    if let Some(c) = group_left.get_mut(&o.group) {
                        *c -= 1;
                        if *c == 0 {
                            let (s, e) = groups[o.group as usize];
                            mark(&mut del, (s, e - s));
                        }
                    }
                }
            }
            Op::Ins(y) => {
                let e = &items1[y];
                let prev = i0.checked_sub(1).map(|k| &items0[k]);
                let next = items0.get(i0);
                if let Some(p) = prev {
                    let inside = p.group != u32::MAX
                        && matches!(p.it, It::Ch(_))
                        && p.after < groups[p.group as usize].1;
                    if inside && !matches!(e.it, It::Ch(_)) {
                        return Err(Error::Unsupported(
                            "ルビ・均等割付の文字の途中での改行は、まだ一太郎形式に保存できません"
                                .into(),
                        ));
                    }
                }
                let region = next.and_then(|nx| {
                    let first = nx.para != NONE && prev.map(|p| p.para != nx.para).unwrap_or(true);
                    let whole = closes_until[oi].map(|l| oi <= l).unwrap_or(false);
                    let s = paras0.get(nx.para).and_then(|p| p.src.get())?;
                    // (a box line is a top-level paragraph in the model too)
                    (first && whole && (!s.in_row || e.cell == NONE)).then_some(s.start)
                });
                if e.it == It::TabStart {
                    let boxed = |i: Option<&I0>| {
                        i.and_then(|i| paras0.get(i.para))
                            .and_then(|p| p.src.get())
                            .map(|s| s.in_row)
                            .unwrap_or(false)
                    };
                    if boxed(prev) || boxed(next) {
                        return Err(Error::Unsupported(
                            "罫線の枠のすぐ内側・隣に作った表は、まだ一太郎形式に保存できません"
                                .into(),
                        ));
                    }
                }
                // a page break before an existing paragraph goes before its
                // line headers (and before the line of a ruled box)
                let brk_region = (e.it == It::Brk)
                    .then(|| {
                        let nx = next?;
                        let first =
                            nx.para != NONE && prev.map(|p| p.para != nx.para).unwrap_or(true);
                        let s = paras0.get(nx.para).and_then(|p| p.src.get())?;
                        first.then_some(s.start)
                    })
                    .flatten();
                let pos = match (prev, next) {
                    (Some(p), _) if matches!(p.it, It::Ch(_)) => p.after,
                    _ if brk_region.is_some() => brk_region.unwrap(),
                    _ if region.is_some() => region.unwrap(),
                    (_, Some(nx)) => nx.before,
                    (Some(p), None) => p.after,
                    (None, None) => 0,
                };
                // style state of a neighbouring original character
                let near = match (prev, next) {
                    (Some(p), _) if matches!(p.it, It::Ch(_)) => Some(p.del.0),
                    (_, Some(nx)) if matches!(nx.it, It::Ch(_)) => Some(nx.del.0),
                    _ => None,
                }
                .or_else(|| (pos as usize).checked_sub(1).map(|u| u as u32))
                .unwrap_or(0)
                .min(n.saturating_sub(1) as u32);
                let base = t.ru.get(near as usize).copied().unwrap_or(0);
                // Ruled-line records carry no character look: Ichitaro gives
                // them the same properties as the text around, all set to 0
                // (a record that keeps a character's look draws its lines
                // wrong).
                let rec = if t.raws.list.is_empty() {
                    base
                } else {
                    let zero: RawProps =
                        t.raws.list[base as usize].keys().map(|k| (*k, 0)).collect();
                    t.raws.add(zero)
                };
                let (units, raws): (Vec<u16>, Vec<u32>) = match e.it {
                    It::Ch(c) => {
                        let l1 = looks1[e.para][e.ch];
                        if l1.italic == Some(true) {
                            italic_lost = true;
                        }
                        let r = if t.raws.list.is_empty() {
                            apply_look(&RawProps::new(), l1)
                        } else {
                            apply_look(&t.raws.list[base as usize], l1)
                        };
                        let idx = t.raws.add(r);
                        let mut buf = [0u16; 2];
                        let u = c.encode_utf16(&mut buf).to_vec();
                        let k = u.len();
                        (u, vec![idx; k])
                    }
                    // the end of a line in a cell of a new row: a record unit too
                    It::End if e.cell != NONE && table_row(blocks1, e.row).src.get().is_none() => {
                        (vec![text::PARA_END], vec![rec])
                    }
                    It::End => (vec![text::PARA_END], vec![base]),
                    It::Brk => (vec![text::PAGE_BREAK], vec![base]),
                    It::RowStart(_) => {
                        let row = table_row(blocks1, e.row);
                        let tmpl = template_row(&blocks0, prev, next);
                        let payload = match tmpl {
                            Some(h) => payload_of(&t.units, h),
                            None => synth_rules(row, table_width(blocks1, e.row.0)),
                        };
                        let u = record(0x0010, &payload);
                        let k = u.len();
                        // a copied line header keeps the style states of its
                        // units: Ichitaro keeps the line types there
                        let styles = tmpl
                            .map(|h| (h.0 as usize, h.1 as usize))
                            .filter(|&(s0, l)| l == k && s0 + l <= t.ru.len())
                            .map(|(s0, l)| t.ru[s0..s0 + l].to_vec());
                        (u, styles.unwrap_or_else(|| vec![rec; k]))
                    }
                    It::CellStart(..) => {
                        let row = table_row(blocks1, e.row);
                        let c = &row.cells[e.cell];
                        let u = record(0x0030, &[0, c.left, c.right, 0x00ff, 0]);
                        let k = u.len();
                        (u, vec![rec; k])
                    }
                    It::RowEnd => (vec![text::ROW_END], vec![rec]),
                    It::TabStart => (vec![], vec![]),
                    It::TabEnd => {
                        // the paragraph after a new table needs a plain line header
                        let needs = match next {
                            Some(nx) if nx.para != NONE => paras0[nx.para]
                                .src
                                .get()
                                .map(|s| !s.in_row && s.own_headers.is_empty())
                                .unwrap_or(false),
                            _ => false,
                        };
                        if needs {
                            let s = paras0[next.unwrap().para].src.get().unwrap();
                            let pl = s
                                .eff_header
                                .map(|h| payload_of(&t.units, h))
                                .filter(|p| !has_rules(p))
                                .unwrap_or_else(default_header_payload);
                            let u = record(0x0010, &pl);
                            let k = u.len();
                            (u, vec![base; k])
                        } else {
                            (vec![], vec![])
                        }
                    }
                };
                if !units.is_empty() {
                    if let (Some(nx), true) = (next, e.cell == NONE && e.para != NONE) {
                        if nx.synthetic && nx.it == It::End && headered.insert(e.para) {
                            let u = record(0x0010, &default_header_payload());
                            let k = u.len();
                            ins.push(Ins {
                                pos,
                                units: u,
                                raws: vec![base; k],
                            });
                        }
                    }
                    ins.push(Ins { pos, units, raws });
                }
            }
        }
    }
    if italic_lost {
        warnings.push("斜体は一太郎形式に保存できないため、通常の文字として保存しました。".into());
    }

    // stage 1: apply deletions and insertions
    ins.sort_by_key(|i| i.pos); // stable: keeps diff order at equal positions
    let mut units1: Vec<u16> = Vec::with_capacity(n + 64);
    let mut ru1: Vec<u32> = Vec::with_capacity(n + 64);
    let mut k = 0usize;
    for u in 0..=n {
        while k < ins.len() && ins[k].pos as usize == u {
            units1.extend_from_slice(&ins[k].units);
            ru1.extend_from_slice(&ins[k].raws);
            k += 1;
        }
        if u == n {
            break;
        }
        if let Some(&r) = repl.get(&(u as u32)) {
            units1.push(r);
            ru1.push(t.ru[u]);
            continue;
        }
        if del[u] {
            continue;
        }
        units1.push(t.units[u]);
        ru1.push(restyle.get(&(u as u32)).copied().unwrap_or(t.ru[u]));
    }
    while k < ins.len() {
        units1.extend_from_slice(&ins[k].units);
        ru1.extend_from_slice(&ins[k].raws);
        k += 1;
    }

    if std::env::var_os("EZPZJTD_SAVE_DEBUG").is_some() {
        let at = ins
            .first()
            .map(|i| i.pos as usize)
            .or_else(|| del.iter().position(|d| *d))
            .unwrap_or(0);
        eprintln!(
            "ops: {:?}",
            ops.iter()
                .filter(|o| !matches!(o, Op::Eq(..)))
                .take(20)
                .collect::<Vec<_>>()
        );
        eprintln!(
            "ins at {:?}",
            ins.iter()
                .map(|i| (i.pos, i.units.clone()))
                .collect::<Vec<_>>()
        );
        for (name, u) in [("before", &t.units), ("after", &units1)] {
            eprintln!("--- {name}");
            for tk in text::tokenize(u) {
                let st = match &tk {
                    text::Token::Text { start, .. }
                    | text::Token::Record { start, .. }
                    | text::Token::Inline { start, .. }
                    | text::Token::Control { start, .. } => *start,
                };
                if st + 200 >= at && st <= at + 200 {
                    eprintln!("  {tk:?}");
                }
            }
        }
    }
    // stage 2: line headers (alignment, inherited state)
    let mut want: Vec<Option<Vec<u16>>> = vec![None; paras1.len()];
    {
        let mut src0: Vec<Option<usize>> = vec![None; paras1.len()];
        for op in &ops {
            if let Op::Eq(x, y) = *op {
                let (o, e) = (&items0[x], &items1[y]);
                if o.para != NONE && e.para != NONE && src0[e.para].is_none() {
                    src0[e.para] = Some(o.para);
                }
            }
        }
        let mut prev: Option<Vec<u16>> = None;
        for (k, sp) in src0.iter().enumerate() {
            let w = match sp {
                Some(j) => paras0[*j]
                    .src
                    .get()
                    .map(|s| s.eff_header.map(|h| payload_of(&t.units, h)))
                    .unwrap_or_else(|| prev.clone()),
                None => prev.clone(),
            };
            want[k] = w.clone();
            prev = w;
        }
    }
    let (units2, ru2) = fix_headers(&units1, &ru1, blocks1, &want, warnings)?;
    // stage 3: encode
    let style_bytes = encode_events(&ru2, &t.raws, &t.lens, t.had_end, &t.tail);
    let mut head = b"TextV.01".to_vec();
    head.extend_from_slice(&(units2.len() as u32).to_be_bytes());
    for u in &units2 {
        head.extend_from_slice(&u.to_be_bytes());
    }
    head.extend_from_slice(&style_bytes);
    let mut subs = vec![head];
    subs.append(&mut t.rest);
    let packed = ssmg_pack(&subs);

    // stage 4: verify
    verify(&packed, blocks1)?;
    let units_changed = units2 != t.units;
    Ok(Some((packed, units_changed)))
}

/// The unit range of a one-cell line of a ruled box (line header, cell
/// record, its text, optional 000A, 000E) that holds only paragraph `p`.
fn box_line(units: &[u16], p: &doc::ParaSrc) -> Option<(u32, u32)> {
    if !p.in_row {
        return None;
    }
    let (s, e) = (p.start as usize, p.end as usize);
    let tail = units.get(e..)?;
    let end = e + tail.iter().position(|&u| u == text::ROW_END)?;
    let toks = text::tokenize(units.get(s..=end)?);
    let headers = toks
        .iter()
        .filter(|t| matches!(t, text::Token::Record { class: 0x10, .. }))
        .count();
    let cells = toks
        .iter()
        .filter(|t| matches!(t, text::Token::Record { class: 0x30, .. }))
        .count();
    let breaks = toks
        .iter()
        .filter(|t| matches!(t, text::Token::Control { code, .. } if *code == text::PARA_END))
        .count();
    (headers == 1 && cells == 1 && breaks <= 1).then_some((s as u32, (end + 1 - s) as u32))
}

fn box_join_error() -> Error {
    Error::Unsupported(
        "罫線の枠の行をまたいで行をつなぐ編集は、まだ一太郎形式に保存できません".into(),
    )
}

/// A cell record or a line end lies in `units[from..to]`.
fn crosses_line(units: &[u16], from: u32, to: u32) -> bool {
    let (from, to) = (from as usize, (to as usize).min(units.len()));
    (from..to).any(|i| {
        units[i] == text::ROW_END || (units[i] == text::REC && units.get(i + 1) == Some(&0x0030))
    })
}

fn table_row(blocks: &[Block], (bi, ri): (usize, usize)) -> &doc::Row {
    match &blocks[bi] {
        Block::Table(t) => &t.rows[ri],
        _ => unreachable!("row item outside a table"),
    }
}

/// A neighbouring original row to copy a line header from.
fn template_row(blocks0: &[Block], prev: Option<&I0>, next: Option<&I0>) -> Option<(u32, u32)> {
    for it in [prev, next].into_iter().flatten() {
        if it.row.1 != NONE {
            if let Block::Table(t) = &blocks0[it.row.0] {
                if let Some(s) = t.rows[it.row.1].src.get() {
                    return Some(s.header);
                }
            }
        }
    }
    None
}

/// Line header payload for a new table row: `width, 0, x0`, then the row's
/// rule items `(style, a, b, dist)`, the last one cut to `(style, 0)` when it
/// is a closing rule with nothing after it (spec §4.3). Rows the editor made
/// add up to `width`; anything else gets plain rules between its cells.
fn synth_rules(row: &doc::Row, width: u16) -> Vec<u16> {
    let sum = |x0: u16, rules: &[doc::Rule]| -> u32 {
        let mut c = x0 as u32 + 1;
        for (i, r) in rules.iter().enumerate() {
            let cut = i + 1 == rules.len() && r.a == 0 && r.b == 0 && r.dist == 0;
            c += if cut {
                1
            } else {
                r.a as u32 + r.dist as u32 + 2
            };
        }
        c
    };
    let (x0, rules) = if !row.rules.is_empty() && sum(row.x0, &row.rules) == width as u32 {
        (row.x0, row.rules.clone())
    } else {
        let x0 = row
            .cells
            .first()
            .map(|c| c.left.saturating_sub(2))
            .unwrap_or(0);
        let mut rules: Vec<doc::Rule> = row
            .cells
            .iter()
            .map(|c| doc::Rule {
                style: 0x13,
                a: 0,
                b: 0,
                dist: c.right.saturating_sub(c.left),
            })
            .collect();
        let end = row.cells.last().map(|c| c.right + 2).unwrap_or(2);
        rules.push(doc::Rule {
            style: 0x13,
            dist: width.saturating_sub(end),
            ..Default::default()
        });
        (x0, rules)
    };
    let mut v = vec![width, 0, x0];
    for (i, r) in rules.iter().enumerate() {
        if i + 1 == rules.len() && r.a == 0 && r.b == 0 && r.dist == 0 {
            v.extend_from_slice(&[r.style, 0]);
        } else {
            v.extend_from_slice(&[r.style, r.a, r.b, r.dist]);
        }
    }
    let mut p = vec![0, 0x8f, v.len() as u16];
    p.extend(v);
    p.extend_from_slice(&[0xffff, 0]);
    p
}

fn table_width(blocks: &[Block], bi: usize) -> u16 {
    match &blocks[bi] {
        Block::Table(t) => t.width,
        _ => 0,
    }
}

/// Make every top-level paragraph start with the line header state it
/// should have: the state of the original paragraph it came from (so merges
/// and splits do not change the paragraphs that inherit it), with the
/// alignment the edited model asks for. Paragraphs in ruled lines get their
/// alignment on the line header.
fn fix_headers(
    units: &[u16],
    ru: &[u32],
    blocks1: &[Block],
    want: &[Option<Vec<u16>>],
    warnings: &mut Vec<String>,
) -> Result<(Vec<u16>, Vec<u32>)> {
    let (mut bo, _) = doc::blocks_tracked(units, &StyleMap::empty());
    normalize_blocks(&mut bo);
    let po = flat_paras(&bo);
    let p1 = flat_paras(blocks1);
    if po.len() != p1.len() {
        return Err(Error::Unsupported(format!(
            "この編集は、まだ一太郎形式に保存できません（段落の構成 {} / {}）",
            po.len(),
            p1.len()
        )));
    }
    let lines = line_headers(units);
    // record start → replacement payload; position → inserted payload
    let mut replace: BTreeMap<u32, (u32, Vec<u16>)> = BTreeMap::new();
    let mut insert: BTreeMap<u32, Vec<u16>> = BTreeMap::new();
    let mut conflict = false;
    for (k, (a, b)) in po.iter().zip(p1.iter()).enumerate() {
        let Some(s) = a.src.get() else { continue };
        if s.in_row {
            if let Some(h) = s.eff_header {
                let mut pl = replace
                    .get(&h.0)
                    .map(|x| x.1.clone())
                    .unwrap_or_else(|| payload_of(units, h));
                let mut changed = false;
                if b.align != Align::Other && tlv_align(&pl) != b.align {
                    if replace.contains_key(&h.0) {
                        conflict = true;
                    }
                    pl = set_align(&pl, b.align);
                    changed = true;
                }
                // a line of a ruled box (a top-level paragraph in the model)
                // keeps its line feed on the row's line header
                if !is_row_para(blocks1, k) && tlv_fmt(&pl).1 != b.feed {
                    pl = set_tlv(&pl, 0x20, b.feed.map(|f| f.to_tlv()).as_deref());
                    changed = true;
                }
                if changed {
                    replace.insert(h.0, (h.1, pl));
                }
            }
            continue;
        }
        // the state this paragraph should start with: its original one
        let wbase = want
            .get(k)
            .cloned()
            .flatten()
            .unwrap_or_else(default_header_payload);
        // the state in effect where this paragraph's content starts: its
        // line's own header, or one inside the line before, else the defaults
        let cs = s.chars.first().map(|c| c.before).unwrap_or(s.end);
        let li = lines.partition_point(|l| l.1 < cs).min(lines.len().saturating_sub(1));
        let line = lines.get(li).copied();
        let last_h = line.and_then(|l| l.2);
        let last_ins = insert
            .range(line.map(|l| l.0).unwrap_or(0)..=cs)
            .next_back()
            .map(|(p, pl)| (*p, pl.clone()));
        let current = match (last_h, last_ins) {
            (Some(h), Some((ip, pl))) if ip > h.0 => pl,
            (Some(h), _) => replace
                .get(&h.0)
                .map(|x| x.1.clone())
                .unwrap_or_else(|| payload_of(units, h)),
            (None, Some((_, pl))) => pl,
            (None, None) => default_header_payload(),
        };
        let align_ok = b.align == Align::Other || tlv_align(&current) == b.align;
        let fmt_ok = tlv_fmt(&current) == (b.indent, b.feed);
        if align_ok && fmt_ok && (current == wbase || (has_rules(&current) && has_rules(&wbase))) {
            continue;
        }
        // a plain line header (a ruled line's geometry is not copied)
        let plain = if has_rules(&wbase) {
            default_header_payload()
        } else {
            wbase
        };
        let desired = if b.align == Align::Other || tlv_align(&plain) == b.align {
            plain
        } else {
            set_align(&plain, b.align)
        };
        let desired = set_fmt(&desired, b.indent, b.feed);
        if desired == current {
            continue;
        }
        if std::env::var_os("EZPZJTD_SAVE_DEBUG").is_some() {
            eprintln!(
                "header fix para {k} {:?}: current {:?} desired {:?} own {:?}",
                b.plain_text().chars().take(10).collect::<String>(),
                current,
                desired,
                s.own_headers
            );
        }
        if let Some(&h) = s.own_headers.last() {
            replace.insert(h.0, (h.1, desired));
        } else {
            let at = s.page_break.map(|u| u + 1).unwrap_or(s.start);
            insert.insert(at, desired);
        }
    }
    if conflict {
        warnings
            .push("表の同じ行で異なる配置を指定したため、行ごとにそろえて保存しました。".into());
    }
    if replace.is_empty() && insert.is_empty() {
        return Ok((units.to_vec(), ru.to_vec()));
    }
    let mut ou = Vec::with_capacity(units.len() + 64);
    let mut or = Vec::with_capacity(units.len() + 64);
    let mut u = 0usize;
    while u <= units.len() {
        if let Some(pl) = insert.get(&(u as u32)) {
            let rec = record(0x0010, pl);
            let r = ru.get(u).or(ru.last()).copied().unwrap_or(0);
            or.extend(std::iter::repeat(r).take(rec.len()));
            ou.extend(rec);
        }
        if u == units.len() {
            break;
        }
        if let Some((len, pl)) = replace.get(&(u as u32)) {
            let rec = record(0x0010, pl);
            or.extend(std::iter::repeat(ru[u]).take(rec.len()));
            ou.extend(rec);
            u += *len as usize;
            continue;
        }
        ou.push(units[u]);
        or.push(ru[u]);
        u += 1;
    }
    Ok((ou, or))
}

/// Lines of the text outside ruled rows as (start, end, header in effect),
/// `end` being the unit that ends the line (000A or 000E). The same rule as
/// the reader: a line header formats its own line, one that comes after text
/// in a line formats the next line, and a line with neither has the defaults.
fn line_headers(units: &[u16]) -> Vec<(u32, u32, Option<(u32, u32)>)> {
    let toks = text::tokenize(units);
    let mut out = Vec::new();
    let (mut line_start, mut has_text) = (0u32, false);
    let (mut own, mut carry_in, mut carry_out) = (None, None, None);
    for (i, tk) in toks.iter().enumerate() {
        match tk {
            text::Token::Record { start, class: 0x10, payload } => {
                let end = toks.get(i + 1).map(tok_start).unwrap_or(units.len());
                let h = (*start as u32, (end - start) as u32);
                if has_rules(payload) {
                    // a ruled row: its header is the row's own
                    own = None;
                } else if has_text {
                    carry_out = Some(h);
                } else {
                    own = Some(h);
                }
            }
            text::Token::Text { .. } | text::Token::Inline { .. } => has_text = true,
            text::Token::Control { start, code } if *code == text::PARA_END || *code == text::ROW_END => {
                out.push((line_start, *start as u32, own.or(carry_in)));
                carry_in = if *code == text::PARA_END { carry_out.take() } else { None };
                carry_out = None;
                own = None;
                has_text = false;
                line_start = *start as u32 + 1;
            }
            _ => {}
        }
    }
    out.push((line_start, units.len() as u32, own.or(carry_in)));
    out
}

fn tok_start(t: &text::Token) -> usize {
    match t {
        text::Token::Text { start, .. }
        | text::Token::Record { start, .. }
        | text::Token::Inline { start, .. }
        | text::Token::Control { start, .. } => *start,
    }
}

/// Parse the new text and compare it with the edited model.
/// Tables whose rows all have one cell read back as plain lines (a ruled line
/// with only a line under it, for example a table whose rows were all
/// deleted but the line above). Compare against what the reader will see.
fn as_read(blocks: &[Block]) -> Vec<Block> {
    let mut out = Vec::new();
    for b in blocks {
        match b {
            Block::Table(t) if t.rows.iter().all(|r| r.cells.len() <= 1) => {
                for r in &t.rows {
                    for c in &r.cells {
                        if c.paragraphs.is_empty() {
                            out.push(Block::Paragraph(doc::Paragraph::default()));
                        }
                        out.extend(c.paragraphs.iter().cloned().map(Block::Paragraph));
                    }
                }
            }
            _ => out.push(b.clone()),
        }
    }
    normalize_blocks(&mut out);
    out
}

fn verify(packed: &[u8], blocks1: &[Block]) -> Result<()> {
    let (units, map) = doc::units_and_styles(packed)?;
    let (mut got, _) = doc::blocks_tracked(&units, &map);
    normalize_blocks(&mut got);
    let want = as_read(blocks1);
    let blocks1 = &want[..];
    let a = flatten1(&got);
    let b = flatten1(blocks1);
    let fail = |what: String| {
        Err(Error::Unsupported(format!(
            "保存結果の確認で違いが見つかったため、一太郎形式での保存を中止しました（{what}）"
        )))
    };
    if a.len() != b.len() || a.iter().zip(&b).any(|(x, y)| x.it != y.it) {
        let at = a
            .iter()
            .zip(&b)
            .position(|(x, y)| x.it != y.it)
            .unwrap_or(a.len().min(b.len()));
        if std::env::var("EZPZJTD_SAVE_DEBUG").is_ok() {
            let lo = at.saturating_sub(6);
            eprintln!(
                "VERIFY got  {:?}",
                a.iter()
                    .skip(lo)
                    .take(14)
                    .map(|x| &x.it)
                    .collect::<Vec<_>>()
            );
            eprintln!(
                "VERIFY want {:?}",
                b.iter()
                    .skip(lo)
                    .take(14)
                    .map(|x| &x.it)
                    .collect::<Vec<_>>()
            );
        }
        return fail(format!("text or structure differs at item {at}"));
    }
    let pa = flat_paras(&got);
    let pb = flat_paras(blocks1);
    for (k, (x, y)) in pa.iter().zip(pb.iter()).enumerate() {
        let in_row = false;
        if x.page_break_before != y.page_break_before {
            return fail(format!("page break of paragraph {k}"));
        }
        // indents and line feed: a line of a ruled box keeps only its line
        // feed (on the row's line header), a cell of a table none (Row::feed),
        // and an empty line the reader adds (no source) is not stored
        let stored = x.src.get().map(|s| (s.in_row,));
        let differs = match stored {
            Some((true,)) => x.feed != y.feed,
            Some((false,)) => (x.indent, x.feed) != (y.indent, y.feed),
            None => false,
        };
        if differs && !is_row_para(&got, k) {
            if std::env::var("EZPZJTD_SAVE_DEBUG").is_ok() {
                eprintln!(
                    "FMT {k} {:?} got {:?} want {:?} src {:?}",
                    y.plain_text().chars().take(12).collect::<String>(),
                    (x.indent, x.feed),
                    (y.indent, y.feed),
                    y.src
                        .get()
                        .map(|s| (s.in_row, s.own_headers.clone(), s.eff_header))
                );
            }
            return fail(format!("indent or line feed of paragraph {k}"));
        }
        if !in_row && y.align != Align::Other && x.align != y.align && !is_row_para(&got, k) {
            return fail(format!(
                "alignment of paragraph {k} ({:?} instead of {:?}, {:?})",
                x.align,
                y.align,
                y.plain_text().chars().take(12).collect::<String>()
            ));
        }
        // each character's own stored look (inline runs show only the first)
        let units_of: Vec<u32> = x
            .src
            .get()
            .map(|s| s.chars.iter().map(|c| c.unit).collect())
            .unwrap_or_default();
        for (i, s2) in char_looks(y).into_iter().enumerate() {
            let s1 = match units_of.get(i) {
                Some(&u) => map
                    .at(u as usize)
                    .map(CharStyle::from_raw)
                    .unwrap_or_default(),
                None => char_looks(x)[i].clone(),
            };
            if !same_storable(&s1, s2) {
                return fail(format!("formatting of paragraph {k} character {i}"));
            }
        }
    }
    Ok(())
}

/// Paragraph `k` is inside a multi-column table (alignment is per line there).
fn is_row_para(blocks: &[Block], k: usize) -> bool {
    let mut i = 0usize;
    for b in blocks {
        match b {
            Block::Paragraph(_) => i += 1,
            Block::Table(t) => {
                let n: usize = t
                    .rows
                    .iter()
                    .flat_map(|r| &r.cells)
                    .map(|c| c.paragraphs.len())
                    .sum();
                if k >= i && k < i + n {
                    return true;
                }
                i += n;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn myers_basic() {
        let a: Vec<char> = "abcabba".chars().collect();
        let b: Vec<char> = "cbabac".chars().collect();
        let ops = myers(&a, &b, 100).unwrap();
        let dels = ops.iter().filter(|o| matches!(o, Op::Del(_))).count();
        let insn = ops.iter().filter(|o| matches!(o, Op::Ins(_))).count();
        assert_eq!(dels + insn, 5);
        // replaying the script turns a into b
        let mut out = Vec::new();
        for o in ops {
            match o {
                Op::Eq(x, _) => out.push(a[x]),
                Op::Ins(y) => out.push(b[y]),
                Op::Del(_) => {}
            }
        }
        assert_eq!(out, b);
    }

    /// Indents and line feed go into the line header in tag order, and go
    /// away again when the model has none.
    #[test]
    fn indent_and_feed_tlv() {
        let p = vec![0, 0x24, 1, 1, 0x8f, 1, 0xa0, 0xffff, 0];
        let ind = Indent {
            mm: false,
            left: 4,
            right: 0,
            first_left: 2,
            first_right: 0,
        };
        let feed = LineFeed { kind: 2, value: 0 };
        let q = set_fmt(&p, Some(ind), Some(feed));
        assert_eq!(
            q,
            vec![
                0, 0x20, 4, 2, 0, 0, 0, 0x24, 1, 1, 0x26, 5, 0, 4, 0, 2, 0, 0x8f, 1, 0xa0, 0xffff,
                0
            ]
        );
        assert_eq!(tlv_fmt(&q), (Some(ind), Some(feed)));
        assert_eq!(tlv_align(&q), Align::Center);
        assert_eq!(set_fmt(&q, None, None), p);
    }

    #[test]
    fn align_tlv() {
        let p = vec![0, 0x20, 4, 1, 0, 1, 0, 0x8f, 3, 0xa0, 0, 0, 0xffff, 0];
        let q = set_align(&p, Align::Center);
        assert_eq!(tlv_align(&q), Align::Center);
        assert_eq!(text::para_tlv(&q).len(), 3);
        assert_eq!(q[..7], p[..7]);
        let r = set_align(&q, Align::Left);
        assert_eq!(tlv_align(&r), Align::Left);
        assert_eq!(
            set_align(&default_header_payload(), Align::Right),
            vec![0, 0x24, 1, 2, 0xffff, 0]
        );
    }
}
