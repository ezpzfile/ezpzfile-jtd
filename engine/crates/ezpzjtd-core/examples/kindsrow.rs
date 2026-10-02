//! Insert a row in the first table whose rows have line types, save, reopen,
//! and compare the line types of the new row with the row it was copied from.
//! usage: kindsrow <file.jtd>
use ezpzjtd_core::doc::Block;
use ezpzjtd_core::edit::{Editor, Move};

fn main() {
    let path = std::env::args().nth(1).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let doc = ezpzjtd_core::open(bytes.clone()).unwrap();
    let mut e = Editor::new(doc);
    for _ in 0..400 {
        if e.in_table() {
            break;
        }
        e.move_caret(Move::Down, false);
    }
    let kinds_before: Vec<usize> = e.doc.sheets[0]
        .blocks
        .iter()
        .filter_map(|b| match b {
            Block::Table(t) => Some(t.rows.iter().filter(|r| !r.kinds.is_empty()).count()),
            _ => None,
        })
        .collect();
    println!("rows with kinds per table before: {kinds_before:?}");
    if !e.insert_row(true) {
        println!("no row inserted");
        return;
    }
    let want: Vec<usize> = e.doc.sheets[0]
        .blocks
        .iter()
        .filter_map(|b| match b {
            Block::Table(t) => Some(t.rows.iter().filter(|r| !r.kinds.is_empty()).count()),
            _ => None,
        })
        .collect();
    match ezpzjtd_core::save::save(&bytes, &e.doc) {
        Ok(s) => {
            let back = ezpzjtd_core::open(s.bytes).unwrap();
            let got: Vec<usize> = back.sheets[0]
                .blocks
                .iter()
                .filter_map(|b| match b {
                    Block::Table(t) => Some(t.rows.iter().filter(|r| !r.kinds.is_empty()).count()),
                    _ => None,
                })
                .collect();
            println!("model {want:?}\nsaved {got:?}");
        }
        Err(err) => println!("refused: {err}"),
    }
}
