use ezpzjtd_core::edit::{Editor, Pos};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let bytes = std::fs::read(&a[1]).unwrap();
    let doc = ezpzjtd_core::open(bytes.clone()).unwrap();
    let mut e = Editor::new(doc);
    let p: usize = a[2].parse().unwrap();
    let off: usize = a[3].parse().unwrap();
    e.caret = Pos { p, off }; e.anchor = e.caret;
    match a[4].as_str() { "pb" => e.page_break(), "type" => e.insert_text("あ"), "enter" => e.enter(), _ => {} }
    match ezpzjtd_core::save::save(&bytes, &e.doc) { Ok(s) => { println!("ok {} bytes", s.bytes.len()); if let Some(o) = a.get(5) { std::fs::write(o, &s.bytes).unwrap() } } Err(err) => println!("ERR {err}") }
}
