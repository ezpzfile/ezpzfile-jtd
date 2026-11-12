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

#[test]
fn face_classes_and_standard_face() {
    use ezpzjtd_core::doc::FontInfo;
    use ezpzjtd_core::layout::{base_font, face_class};
    assert_eq!(face_class("ＭＳ 明朝"), 0);
    assert_eq!(face_class("ＭＳ ゴシック"), 1);
    assert_eq!(face_class("HG丸ｺﾞｼｯｸM-PRO"), 2);
    assert_eq!(face_class("HG教科書体"), 3);
    assert_eq!(face_class("HG正楷書体-PRO"), 4);
    assert_eq!(face_class("HG創英角ﾎﾟｯﾌﾟ体"), 5);
    let f = |n: &str, c: u8| FontInfo { name: n.into(), charset: c };
    // a table that starts with Ichitaro's mark face and a Latin face
    let fonts = vec![f("JustUnitMark", 2), f("Times New Roman", 0), f("HG丸ｺﾞｼｯｸM-PRO", 0x80)];
    assert_eq!(base_font(&fonts), Some(2));
}

#[test]
fn columns_fill_one_after_another() {
    use ezpzjtd_core::edit::Editor;
    use ezpzjtd_core::layout::{layout, Item, PageSetup};
    let mut ed = Editor::new(ezpzjtd_core::doc::Document::blank());
    for _ in 0..60 {
        ed.insert_text("あ");
        ed.enter();
    }
    let setup = PageSetup { columns: 2, col_gap_mm: 10.0, ..PageSetup::default() };
    let l = layout(&ed.doc.sheets[0].blocks, &setup, false, None);
    let xs: Vec<f32> = l.pages[0].items.iter().filter_map(|i| match i { Item::Text { xs, .. } => xs.first().copied(), _ => None }).collect();
    let left = setup.left();
    assert!(xs.iter().any(|&x| (x - left).abs() < 1.0));
    assert!(xs.iter().any(|&x| x > left + setup.text_w() / 2.0), "second column used");
    assert_eq!(l.pages.len(), 1);
}
