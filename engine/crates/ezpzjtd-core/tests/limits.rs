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
