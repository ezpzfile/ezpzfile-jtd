//! Minimal one-edit copies of a file, for narrowing down which kind of edit a
//! reader rejects.
//! usage: variants <file.jtd> <out_dir>
use ezpzjtd_core::edit::{Editor, Move};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let bytes = std::fs::read(&a[1]).unwrap();
    let doc = ezpzjtd_core::open(bytes.clone()).unwrap();
    std::fs::create_dir_all(&a[2]).unwrap();
    let stem = std::path::Path::new(&a[1])
        .file_stem()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let save = |e: &Editor, tag: &str| match ezpzjtd_core::save::save(&bytes, &e.doc) {
        Ok(s) => {
            std::fs::write(format!("{}/{stem}-{tag}.jtd", a[2]), &s.bytes).unwrap();
            println!("{tag}: saved, changed {}", s.changed);
        }
        Err(err) => println!("{tag}: refused {err:?}"),
    };
    // text only (baseline that worked)
    let mut e = Editor::new(doc.clone());
    e.move_caret(Move::Down, false);
    e.insert_text("追記。");
    save(&e, "text");
    // one page break
    let mut e = Editor::new(doc.clone());
    e.move_caret(Move::Down, false);
    e.move_caret(Move::Down, false);
    e.page_break();
    save(&e, "pagebreak");
    // one new table in a plain paragraph
    let mut e = Editor::new(doc.clone());
    e.move_caret(Move::Down, false);
    e.insert_table(2, 3);
    save(&e, "table");
    // one inserted row in the first table reached by moving down
    let mut e = Editor::new(doc.clone());
    let mut done = false;
    for _ in 0..400 {
        if e.insert_row(true) {
            done = true;
            break;
        }
        e.move_caret(Move::Down, false);
    }
    if done {
        save(&e, "row");
    } else {
        println!("row: no table reached");
    }
    // a plain Enter (new paragraph), for comparison
    let mut e = Editor::new(doc.clone());
    e.move_caret(Move::Down, false);
    e.enter();
    save(&e, "enter");
}
