//! Print the editor layout of a .jtd file as JSON (for drawing it elsewhere).
//! usage: cargo run --example layoutjson -- file.jtd > layout.json
use ezpzjtd_core::edit::Editor;

fn main() {
    let path = std::env::args().nth(1).expect("file.jtd");
    let doc = ezpzjtd_core::open(std::fs::read(path).unwrap()).unwrap();
    let mut e = Editor::new(doc);
    e.show_marks = false;
    let l = e.layout().clone();
    println!(
        "{}",
        serde_json::json!({ "setup": e.setup, "pages": l.pages })
    );
}
