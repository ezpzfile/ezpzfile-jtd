//! Hostile input: crafted files must fail fast or be cut at the limits, never
//! allocate gigabytes or lay out without end.

use ezpzjtd_core::blank::{new_document, BLANK_JTD};
use ezpzjtd_core::cfb::Cfb;
use ezpzjtd_core::cfbw::{write, Tree};
use ezpzjtd_core::edit::Editor;
use ezpzjtd_core::layout::{layout_with, PageSetup};
use ezpzjtd_core::{open, open_with, Error, Limits};
use std::time::{Duration, Instant};

fn be(v: u32) -> [u8; 4] {
    v.to_be_bytes()
}

/// BLANK_JTD with its /DocumentText replaced.
fn with_text_stream(dt: Vec<u8>) -> Vec<u8> {
    let cfb = Cfb::open(BLANK_JTD.to_vec()).unwrap();
    let mut t = Tree::from_cfb(&cfb);
    assert!(t.set_stream("/DocumentText", dt));
    write(&t)
}

fn ssmg(block_size: usize, block: &[u8], len: u32, ids: &[u32], n: u32) -> Vec<u8> {
    let mut b = b"SsmgV.01".to_vec();
    b.extend(be(1));
    b.extend(be(block_size as u32));
    b.extend(be(1));
    let mut blk = block.to_vec();
    blk.resize(block_size, 0);
    b.extend(&blk);
    b.extend(be(0));
    b.extend(be(0));
    b.extend(be(len));
    b.extend(be(n));
    b.extend(be(n));
    for id in ids {
        b.extend(be(*id));
    }
    b.extend(be(0));
    b
}

fn text_block(units: u32) -> Vec<u8> {
    let mut b = b"TextV.01".to_vec();
    b.extend(be(units));
    while b.len() < 4096 {
        b.extend("あいうえお\u{d}".encode_utf16().flat_map(|u| u.to_be_bytes()));
    }
    b
}

fn quick<T>(f: impl FnOnce() -> T) -> T {
    let t = Instant::now();
    let r = f();
    assert!(t.elapsed() < Duration::from_secs(2), "took {:?}", t.elapsed());
    r
}

#[test]
fn one_block_listed_many_times_is_refused() {
    // ~45 KB of block ids naming the same 4 KB block: 45 MB if trusted
    let file = with_text_stream(ssmg(4096, &text_block(u32::MAX / 4), u32::MAX, &vec![0; 11_000], 11_000));
    let r = quick(|| open(file));
    assert!(matches!(r, Err(Error::TooLarge(_))), "{:?}", r.map(|d| d.sheets.len()));
}

#[test]
fn huge_length_and_count_fields_are_not_trusted() {
    // claims 4 GB and 4 billion blocks, lists none
    let file = with_text_stream(ssmg(256, &text_block(4)[..256], u32::MAX, &[], u32::MAX));
    let _ = quick(|| open(file));
}

#[test]
fn page_limit_cuts_layout() {
    let mut ed = Editor::new(new_document());
    ed.insert_text(&"あ".repeat(3000));
    ed.select_all();
    ed.set_size(999.0);
    ed.max_pages = 50;
    let l = quick(|| ed.layout().clone());
    assert_eq!(l.pages.len(), 50);
    assert!(l.truncated);
    assert!(ed.status().truncated);
}

#[test]
fn small_documents_are_not_truncated() {
    let mut ed = Editor::new(new_document());
    ed.insert_text("一太郎");
    assert!(!ed.layout().truncated);
}

#[test]
fn byte_limit() {
    let r = open_with(BLANK_JTD.to_vec(), &Limits { max_bytes: 1000, ..Limits::default() });
    assert!(matches!(r, Err(Error::TooLarge(_))));
}

#[test]
fn text_unit_limit() {
    let mut ed = Editor::new(new_document());
    ed.insert_text(&"あ".repeat(500));
    let saved = ezpzjtd_core::save::save(BLANK_JTD, &ed.doc).unwrap().bytes;
    assert!(open(saved.clone()).is_ok());
    let r = open_with(saved, &Limits { max_text_units: 100, ..Limits::default() });
    assert!(matches!(r, Err(Error::TooLarge(_))));
}

#[test]
fn wild_page_setup_is_made_sane() {
    let setup = PageSetup {
        width_mm: f32::NAN,
        height_mm: 1.0e9,
        margin_top_mm: -5.0,
        margin_bottom_mm: 1.0e9,
        margin_left_mm: 0.0,
        margin_right_mm: 0.0,
        chars_per_line: 0,
        lines_per_page: u32::MAX,
        font_pt: 0.0,
        gothic: Vec::new(),
        frames: Vec::new(),
    };
    let mut ed = Editor::new(new_document());
    ed.insert_text(&"あ\u{3000}".repeat(2000));
    let blocks = ed.doc.sheets[0].blocks.clone();
    let l = quick(|| layout_with(&blocks, &setup, false, None, 2000));
    assert!(!l.pages.is_empty() && l.pages.len() <= 2000);
}

#[test]
fn limits_from_json() {
    let l = Limits::from_json(r#"{"maxPages": 300, "maxBytes": 1048576, "unknown": 5, "maxSheets": 0}"#);
    assert_eq!(l.max_pages, 300);
    assert_eq!(l.max_bytes, 1 << 20);
    assert_eq!(l.max_sheets, Limits::default().max_sheets);
    assert_eq!(Limits::from_json("not json"), Limits::default());
}

/// Build an SsmgV.01 container: `blocks` of `block_size`, then one directory
/// entry per sub-stream (`len`, block ids).
fn container(block_size: usize, blocks: &[Vec<u8>], subs: &[(u32, Vec<u32>)]) -> Vec<u8> {
    let mut b = b"SsmgV.01".to_vec();
    b.extend(be(subs.len() as u32));
    b.extend(be(block_size as u32));
    b.extend(be(blocks.len() as u32));
    for blk in blocks {
        let mut blk = blk.clone();
        blk.resize(block_size, 0);
        b.extend(blk);
    }
    for (i, (len, ids)) in subs.iter().enumerate() {
        b.extend(be(i as u32));
        b.extend(be(0));
        b.extend(be(*len));
        b.extend(be(ids.len() as u32));
        b.extend(be(ids.len() as u32));
        for id in ids {
            b.extend(be(*id));
        }
    }
    b.extend(be(0));
    b
}

fn utf16be(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(|u| u.to_be_bytes()).collect()
}

#[test]
fn a_block_listed_twice_is_refused() {
    let mut first = b"TextV.01".to_vec();
    first.extend(be(1000));
    first.extend(utf16be(&"あ".repeat(100)));
    let blocks = vec![first, utf16be(&"い".repeat(128))];
    // honest: blocks 0 and 1
    let ok = container(256, &blocks, &[(512, vec![0, 1])]);
    assert!(ezpzjtd_core::ssmg::substreams(&ok).is_ok());
    // block 0 twice in one sub-stream
    let twice = container(256, &blocks, &[(512, vec![0, 0])]);
    assert!(quick(|| ezpzjtd_core::ssmg::substreams(&twice)).is_err());
    // block 1 in two sub-streams
    let shared = container(256, &blocks, &[(256, vec![0]), (256, vec![1]), (256, vec![1])]);
    assert!(quick(|| ezpzjtd_core::ssmg::substreams(&shared)).is_err());
    // and through open()
    assert!(quick(|| open(with_text_stream(twice))).is_err());
}

#[test]
fn b_qlst_sub_stream_listed_twice_is_refused() {
    let mut head = b"QLSTV.01".to_vec();
    head.extend(be(3));
    for _ in 0..3 {
        head.extend(be(64));
        head.extend(be(1)); // every piece in sub-stream 1
    }
    let piece = utf16be(&"う".repeat(64));
    let dt = container(256, &[head, piece], &[(36, vec![0]), (128, vec![1])]);
    let r = quick(|| ezpzjtd_core::ssmg::pieces(&dt));
    assert!(r.is_err());
    assert!(quick(|| open(with_text_stream(dt))).is_err());
}

#[test]
fn c_large_length_field_fails_fast() {
    let mut first = b"TextV.01".to_vec();
    first.extend(be(u32::MAX));
    // 4 GB claimed, one 256-byte block listed
    let dt = container(256, &[first], &[(u32::MAX, vec![0])]);
    let subs = quick(|| ezpzjtd_core::ssmg::substreams(&dt)).unwrap();
    assert!(subs[0].len() <= 256);
    // more block ids claimed than the stream holds
    let mut lying = container(256, &[b"TextV.01".to_vec()], &[(256, vec![0])]);
    let at = 20 + 256 + 12;
    lying[at..at + 4].copy_from_slice(&be(u32::MAX));
    assert!(quick(|| ezpzjtd_core::ssmg::substreams(&lying)).is_err());
}

#[test]
fn public_layouts_still_open() {
    // TextV.01 and QLSTV.01 built the honest way still read
    let mut head = b"QLSTV.01".to_vec();
    head.extend(be(2));
    head.extend(be(3));
    head.extend(be(1));
    head.extend(be(3));
    head.extend(be(2));
    let dt = container(256, &[head, utf16be("一太郎"), utf16be("文書\u{d}")], &[(28, vec![0]), (6, vec![1]), (6, vec![2])]);
    let p = ezpzjtd_core::ssmg::pieces(&dt).unwrap();
    let text: String = p.iter().map(|x| String::from_utf16_lossy(&x.units)).collect();
    assert_eq!(text, "一太郎文書\u{d}");
}

// ---- 2nd private report (HIDANO Masaki, KUKAN project, 2026-11-02): the
// property map of a style was copied for every style event and every glyph.

/// /DocumentText with one text sub-stream: `units`, then the raw style events.
fn styled_text(units: &[u16], style: &[u8]) -> Vec<u8> {
    let mut payload = b"TextV.01".to_vec();
    payload.extend(be(units.len() as u32));
    for u in units {
        payload.extend(u.to_be_bytes());
    }
    payload.extend(style);
    let len = payload.len();
    let nb = len.div_ceil(256);
    payload.resize(nb * 256, 0);
    let mut b = b"SsmgV.01".to_vec();
    b.extend(be(1));
    b.extend(be(256));
    b.extend(be(nb as u32));
    b.extend(&payload);
    b.extend(be(0));
    b.extend(be(0));
    b.extend(be(len as u32));
    b.extend(be(nb as u32));
    b.extend(be(nb as u32));
    for i in 0..nb as u32 {
        b.extend(be(i));
    }
    b.extend(be(0));
    b
}

/// A style event setting every property id the format allows (0..=254).
fn set_every_property(v: u32) -> Vec<u8> {
    let mut e = vec![0xfe];
    for id in 0..=254u8 {
        e.extend([id, 4]);
        e.extend(be(v));
    }
    e.extend([0xff, 0x00]);
    e
}

fn chars(n: usize, per_paragraph: usize) -> Vec<u16> {
    let mut u = Vec::new();
    for i in 0..n {
        u.push(0x3042);
        if (i + 1) % per_paragraph == 0 {
            u.push(0x0a);
        }
    }
    u
}

#[test]
fn d_many_style_events_hit_the_style_limit() {
    // 255 properties, then 200,000 six-byte events that each change one value:
    // before 0.3.3 every event copied the whole map (about 700 MB, 6 s)
    let n = 200_000;
    let mut st = set_every_property(0x8000_0000);
    for i in 0..n as u32 {
        st.extend([0xfe, 0x00, 0x01, (i & 0xff) as u8 | 1, 0xff, 0x00]);
    }
    let file = with_text_stream(styled_text(&chars(n + 10, 100), &st));
    let r = quick(|| open(file));
    assert!(matches!(r, Err(Error::TooLarge(_))), "{:?}", r.err());
}

#[test]
fn e_style_limit_is_configurable() {
    let mut st = set_every_property(0x8000_0000);
    for i in 0..100u32 {
        st.extend([0xfe, 0x00, 0x01, i as u8 | 1, 0xff, 0x00]);
    }
    let file = with_text_stream(styled_text(&chars(200, 50), &st));
    assert!(open(file.clone()).is_ok());
    let tight = Limits::from_json(r#"{"maxStyleEntries": 1000}"#);
    assert_eq!(tight.max_style_entries, 1000);
    assert!(matches!(open_with(file, &tight), Err(Error::TooLarge(_))));
}

#[test]
fn f_one_large_style_over_many_glyphs_stays_small() {
    // one 255-entry style over 400,000 characters in one paragraph: before
    // 0.3.3 each glyph copied the style and its map (about 1.4 GB, 9 s)
    let n = 400_000;
    let mut st = set_every_property(0x8000_0000);
    st.push(0x00);
    st.extend(be(n as u32));
    let file = with_text_stream(styled_text(&chars(n, n + 1), &st));
    quick(|| {
        let doc = open(file).expect("opens");
        let mut ed = Editor::new(doc);
        let l = ed.layout();
        assert!(l.truncated, "cut at the page limit");
    });
}

#[test]
fn g_runs_share_one_property_map() {
    let st = set_every_property(0x8000_0000);
    let file = with_text_stream(styled_text(&chars(30, 10), &st));
    let doc = open(file).expect("opens");
    let runs: Vec<_> = doc.sheets[0]
        .blocks
        .iter()
        .filter_map(|b| match b {
            ezpzjtd_core::doc::Block::Paragraph(p) => p.runs.first(),
            _ => None,
        })
        .collect();
    assert!(runs.len() >= 3, "{} paragraphs", runs.len());
    assert_eq!(runs[0].style.raw.len(), 255);
    for r in &runs[1..] {
        assert!(std::rc::Rc::ptr_eq(&runs[0].style.raw, &r.style.raw), "copied, not shared");
    }
}
