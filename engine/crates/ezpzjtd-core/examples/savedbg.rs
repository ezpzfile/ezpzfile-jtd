//! Debug helper: apply a scripted edit and try to save.
//! usage: savedbg <file.jtd> <script> [out.jtd]
//! script: comma-separated steps: t:TEXT (type), e (enter), b (backspace),
//! d (delete), m:N (move right N), u:N (move down N), end, home, B (bold sel N right), c (center)
use ezpzjtd_core::edit::{Editor, Move};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let bytes = std::fs::read(&a[1]).unwrap();
    let doc = ezpzjtd_core::open(bytes.clone()).unwrap();
    let mut e = Editor::new(doc);
    for step in a[2].split(',') {
        let (k, v) = step.split_once(':').unwrap_or((step, ""));
        let n: usize = v.parse().unwrap_or(1);
        match k {
            "t" => e.insert_text(v),
            "at" => {
                let (p, o) = v.split_once('/').unwrap();
                e.caret = ezpzjtd_core::edit::Pos {
                    p: p.parse().unwrap(),
                    off: o.parse().unwrap(),
                };
                e.anchor = e.caret;
            }
            "e" => e.enter(),
            "b" => e.backspace(),
            "d" => e.delete_forward(),
            "m" => (0..n).for_each(|_| e.move_caret(Move::Right, false)),
            "s" => (0..n).for_each(|_| e.move_caret(Move::Right, true)),
            "u" => (0..n).for_each(|_| e.move_caret(Move::Down, false)),
            "end" => e.move_caret(Move::DocEnd, false),
            "home" => e.move_caret(Move::DocStart, false),
            "le" => e.move_caret(Move::LineEnd, false),
            "B" => e.toggle_bold(),
            "c" => e.set_align(ezpzjtd_core::doc::Align::Center),
            "r" => e.set_align(ezpzjtd_core::doc::Align::Right),
            "pb" => e.page_break(),
            "row" => {
                e.insert_row(true);
            }
            "delrow" => {
                e.delete_row();
            }
            "tab" => {
                e.insert_table(2, 3);
            }
            "cell" => {
                e.next_cell(false);
            }
            _ => panic!("unknown step {k}"),
        }
    }
    println!("caret {:?}", e.caret);
    match ezpzjtd_core::save::save(&bytes, &e.doc) {
        Ok(s) => {
            println!("ok changed={} warnings={:?}", s.changed, s.warnings);
            if let Some(o) = a.get(3) {
                std::fs::write(o, &s.bytes).unwrap();
            }
        }
        Err(err) => println!("ERR {err}"),
    }
}
