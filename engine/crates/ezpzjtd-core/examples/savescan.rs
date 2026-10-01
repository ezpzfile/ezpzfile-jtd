//! Try single edits at many places of every corpus file and report the
//! first failure of each kind with a minimal description.
//! usage: savescan <dir> [max_paras]
use ezpzjtd_core::edit::{plen, Editor, Pos};
use std::collections::BTreeMap;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let max: usize = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(40);
    let mut files: Vec<_> = std::fs::read_dir(&a[1])
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().map(|x| x == "jtd").unwrap_or(false))
        .collect();
    files.sort();
    let mut fails: BTreeMap<String, (usize, String)> = BTreeMap::new();
    let mut total = 0;
    for f in &files {
        let bytes = std::fs::read(f).unwrap();
        let Ok(doc) = ezpzjtd_core::open(bytes.clone()) else {
            continue;
        };
        let base = Editor::new(doc.clone());
        let np = base.para_count();
        for p in (0..np).step_by((np / max).max(1)) {
            let len = {
                let mut e = Editor::new(doc.clone());
                e.caret = Pos { p, off: 0 };
                e.anchor = e.caret;
                let _ = e.status();
                plen_of(&e, p)
            };
            let cases: Vec<(&str, usize)> = vec![
                ("type", 0),
                ("type", len / 2),
                ("type", len),
                ("enter", 0),
                ("enter", len / 2),
                ("enter", len),
                ("bs", 0),
                ("bs", len.min(1)),
                ("del", len),
                ("table", len),
                ("pb", 0),
                ("pb", len / 2),
            ];
            for (op, off) in cases {
                let mut e = Editor::new(doc.clone());
                e.caret = Pos { p, off };
                e.anchor = e.caret;
                match op {
                    "type" => e.insert_text("試"),
                    "enter" => e.enter(),
                    "bs" => e.backspace(),
                    "table" => {
                        e.insert_table(2, 3);
                        e.insert_text("表");
                    }
                    "pb" => e.page_break(),
                    _ => e.delete_forward(),
                }
                total += 1;
                if let Err(err) = ezpzjtd_core::save::save(&bytes, &e.doc) {
                    let key = format!(
                        "{op}@{} {}",
                        if off == 0 {
                            "start"
                        } else if off == len {
                            "end"
                        } else {
                            "mid"
                        },
                        err.to_string().chars().take(50).collect::<String>()
                    );
                    let ent = fails.entry(key).or_insert((0, String::new()));
                    if ent.0 == 0 {
                        ent.1 = format!("{} p={p} off={off}", f.display());
                    }
                    ent.0 += 1;
                }
            }
        }
    }
    println!("cases {total}");
    for (k, (n, ex)) in fails {
        println!("{n:5} {k}\n        e.g. {ex}");
    }
}

fn plen_of(e: &Editor, p: usize) -> usize {
    let flat = ezpzjtd_core::edit::flatten(&e.doc.sheets[0].blocks);
    let loc = flat[p];
    let para = match loc {
        ezpzjtd_core::edit::PLoc::Top(b) => match &e.doc.sheets[0].blocks[b] {
            ezpzjtd_core::doc::Block::Paragraph(p) => p.clone(),
            _ => unreachable!(),
        },
        ezpzjtd_core::edit::PLoc::Cell(b, r, c, i) => match &e.doc.sheets[0].blocks[b] {
            ezpzjtd_core::doc::Block::Table(t) => t.rows[r].cells[c].paragraphs[i].clone(),
            _ => unreachable!(),
        },
    };
    plen(&para)
}
