//! Make `src/blank.jtd`, the empty document a new document is saved on.
//!
//! usage: makeblank <public .jtd> <out.jtd>
//!
//! We cannot write every part of an Ichitaro file from scratch yet, so the
//! blank is an Ichitaro file with everything that belongs to its author taken
//! out (we used the public MEXT form `1258443_003.jtd`, see corpus/manifest.tsv):
//!
//! - the text: saved through `save::save` as one empty paragraph
//! - the page setup: Ichitaro's defaults for a new document (A4, margins
//!   30 mm, 40 字 × 40 行, 10.5 pt; spec §10), set in the source's own records
//! - both summaries (`\x05SummaryInformation`, `\x04JSRV_SummaryInformation`:
//!   author, company, dates, the original file path) and the layout caches:
//!   removed
//! - the document id in `/ReferenceInfo`: our own
//! - the storages' times: cleared (the class ids stay: they tell the
//!   reader that this is an Ichitaro document)
//!
//! What stays are settings Ichitaro keeps in every file (footnote, header and
//! macro storages identical in all public files, edit and view styles, fonts).
//! The result is checked: it reads as one empty A4 paragraph, and none of the
//! source's text or names are left in it.
use ezpzjtd_core::cfb::Cfb;
use ezpzjtd_core::cfbw::{self, Meta, Node, Tree};
use ezpzjtd_core::doc::Document;

/// Ichitaro's defaults for a new document (spec §10): margins 30 mm, 40 字
/// (80 half-width columns), 行間 60 % of the character (600), 40 行, 10.5 pt
/// (370 in 1/100 mm). The page records are changed in place: same records,
/// same masks, only these values. (Swapping in shorter records crashed
/// Ichitaro Viewer with some combinations.)
const MARGIN: u16 = 3000;
const HALF_CHARS: u16 = 80;
const GAP: u16 = 600;
const LINES: u16 = 40;
const FONT: u32 = 370;

/// Byte offsets of the fields of one mask group: `sizes[bit]` (0 = unknown,
/// which must not be set). Returns (offset of each set bit, end of group).
fn group(p: &[u8], at: usize, sizes: [usize; 8]) -> ([Option<usize>; 8], usize) {
    let mask = p[at];
    let mut i = at + 1;
    let mut out = [None; 8];
    for bit in 0..8 {
        if mask & (1 << bit) != 0 {
            assert!(sizes[bit] > 0, "field of unknown size in mask {mask:#04x}");
            out[bit] = Some(i);
            i += sizes[bit];
        }
    }
    (out, i)
}

fn put16(p: &mut [u8], at: Option<usize>, v: u16) {
    let at = at.expect("the field is stored");
    p[at..at + 2].copy_from_slice(&v.to_be_bytes());
}

fn page_defaults(tag: u16, p: &mut Vec<u8>) {
    match tag {
        0x1002 => {
            let (_, i) = group(p, 0, [0, 0, 0, 0, 2, 0, 0, 2]);
            let (f, _) = group(p, i, [1, 0, 0, 2, 2, 2, 2, 2]);
            for k in 3..=6 {
                put16(p, f[k], MARGIN);
            }
        }
        0x100b => {
            let (f, _) = group(p, 0, [1, 2, 1, 0, 1, 0, 1, 2]);
            put16(p, f[1], GAP);
            put16(p, f[7], HALF_CHARS);
        }
        0x100d => {
            let (f, _) = group(p, 0, [1, 2, 0, 0, 0, 0, 0, 0]);
            put16(p, f[1], LINES);
        }
        0x1006 => {
            assert_eq!(p[0] & 1, 1, "the character size is stored");
            p[1..5].copy_from_slice(&FONT.to_be_bytes());
        }
        _ => {}
    }
}

/// Our document id (32 bytes) for `/ReferenceInfo`. Documents copied from
/// one another share it in the public files, so one fixed value is fine.
const DOC_ID: [u8; 32] = *b"EZPZ File JTD blank document 1.0";

fn view_styles(b: &[u8]) -> Vec<u8> {
    // 00 01 00 02, then blocks (u16 tag, u32 length); block 1000 holds records
    assert_eq!(&b[..4], &[0, 1, 0, 2]);
    let mut out = b.to_vec();
    let mut i = 4;
    while i + 6 <= b.len() {
        let tag = u16::from_be_bytes([b[i], b[i + 1]]);
        let len = u32::from_be_bytes([b[i + 2], b[i + 3], b[i + 4], b[i + 5]]) as usize;
        if tag == 0x1000 {
            let mut j = i + 6;
            while j + 4 <= i + 6 + len {
                let t = u16::from_be_bytes([b[j], b[j + 1]]);
                let l = u16::from_be_bytes([b[j + 2], b[j + 3]]) as usize;
                let mut p = b[j + 4..j + 4 + l].to_vec();
                page_defaults(t, &mut p);
                out[j + 4..j + 4 + l].copy_from_slice(&p);
                j += 4 + l;
            }
        }
        i += 6 + len;
    }
    assert_eq!(i, b.len());
    out
}

fn clear_times(meta: &mut Meta) {
    meta.ctime = 0;
    meta.mtime = 0;
}

fn clear_meta(nodes: &mut [Node]) {
    for n in nodes {
        match n {
            Node::Storage { meta, children, .. } => {
                clear_times(meta);
                clear_meta(children);
            }
            Node::Stream { meta, .. } => clear_times(meta),
        }
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let src = std::fs::read(&a[1]).unwrap();
    // 1. the text: one empty paragraph
    let cleared = ezpzjtd_core::save::save(&src, &Document::blank()).expect("clear the text");
    let cfb = Cfb::open(cleared.bytes.clone()).unwrap();
    let mut tree = Tree::from_cfb(&cfb);
    // 2. page setup
    let vs = cfb.read("/DocumentViewStyles").unwrap();
    tree.set_stream("/DocumentViewStyles", view_styles(&vs));
    // 3. summaries and layout caches
    for p in [
        "/\u{5}SummaryInformation",
        "/\u{4}JSRV_SummaryInformation",
        "/LineMark",
        "/PageMark",
        "/PaperMark",
        "/DocumentTextPositionTables",
    ] {
        tree.remove(p);
    }
    // 4. our document id
    let mut ri = cfb.read("/ReferenceInfo").unwrap();
    let at = ri
        .windows(10)
        .position(|w| w == [0, 1, 0, 7, 0, 0, 0, 0, 0, 0x20])
        .expect("id")
        + 10;
    ri[at..at + 32].copy_from_slice(&DOC_ID);
    tree.set_stream("/ReferenceInfo", ri);
    // 5. times
    clear_times(&mut tree.root);
    clear_meta(&mut tree.children);
    tree.fix_segment_info();
    let out = cfbw::write(&tree);

    // checks: one empty paragraph on a default A4 page, nothing of the source left
    let d = ezpzjtd_core::open(out.clone()).unwrap();
    assert_eq!(d.plain_text().trim(), "");
    let p = d.page.clone().expect("page setup");
    assert_eq!(
        (
            p.width_mm,
            p.height_mm,
            p.margin_top_mm,
            p.margin_left_mm,
            p.chars_per_line,
            p.lines_per_page,
            p.font_pt
        ),
        (210.0, 297.0, 30.0, 30.0, 40, 40, 10.5)
    );
    let words: Vec<&str> = std::env::args()
        .skip(3)
        .collect::<Vec<_>>()
        .leak()
        .iter()
        .map(|s| s.as_str())
        .collect();
    for w in words {
        let be: Vec<u8> = w.encode_utf16().flat_map(|u| u.to_be_bytes()).collect();
        let le: Vec<u8> = w.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        let (sj, _, _) = encoding_rs::SHIFT_JIS.encode(w);
        for pat in [be, le, sj.to_vec(), w.as_bytes().to_vec()] {
            assert!(
                !out.windows(pat.len()).any(|x| x == &pat[..]),
                "{w:?} is still in the blank"
            );
        }
    }
    std::fs::write(&a[2], &out).unwrap();
    println!("{} bytes, page {:?}", out.len(), p);
}
