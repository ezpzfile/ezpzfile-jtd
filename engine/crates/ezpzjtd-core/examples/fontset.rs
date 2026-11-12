// Set the whole document Gothic and save: fontset in.jtd out.jtd
use ezpzjtd_core::doc::Block;
use ezpzjtd_core::edit::Editor;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let bytes = std::fs::read(&a[1]).unwrap();
    let mut e = Editor::new(ezpzjtd_core::open(bytes.clone()).unwrap());
    e.select_all();
    match std::env::var("FONT") { Ok(f) => e.set_font(&f), Err(_) => e.set_gothic(true) }
    let s = ezpzjtd_core::save::save(&bytes, &e.doc).unwrap();
    std::fs::write(&a[2], &s.bytes).unwrap();
    let d2 = ezpzjtd_core::open(s.bytes).unwrap();
    let g = ezpzjtd_core::edit::gothic_fonts(&d2);
    let (mut n, mut gn) = (0, 0);
    for b in &d2.sheets[0].blocks { if let Block::Paragraph(p) = b { for r in &p.runs { let c = r.text.chars().count(); n += c; if r.style.font.is_some_and(|f| g.contains(&f)) { gn += c } } } }
    println!("fonts {:?} gothic chars {gn}/{n}", d2.fonts.iter().map(|f| f.name.clone()).collect::<Vec<_>>());
}
