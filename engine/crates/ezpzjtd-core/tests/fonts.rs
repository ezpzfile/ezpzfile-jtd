//! Gothic set in the editor is saved: ＭＳ ゴシック goes into /Font and the
//! text points at it.
use ezpzjtd_core::blank::BLANK_JTD;
use ezpzjtd_core::doc::Block;
use ezpzjtd_core::edit::{gothic_fonts, Editor};

#[test]
fn gothic_is_saved() {
    let mut e = Editor::new(ezpzjtd_core::open(BLANK_JTD.to_vec()).unwrap());
    e.insert_text("見出しです");
    e.select_all();
    e.set_gothic(true);
    let saved = ezpzjtd_core::save::save(BLANK_JTD, &e.doc).unwrap();
    let d = ezpzjtd_core::open(saved.bytes).unwrap();
    assert_eq!(d.fonts.len(), 2);
    assert_eq!(d.fonts[1].name, "ＭＳ ゴシック");
    let g = gothic_fonts(&d);
    let Block::Paragraph(p) = &d.sheets[0].blocks[0] else { panic!() };
    assert!(p.runs.iter().all(|r| r.style.font.is_some_and(|f| g.contains(&f))));
}
