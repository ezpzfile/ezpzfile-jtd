//! Editor behaviour: typing, paragraphs, formatting, tables, undo, layout.

use ezpzjtd_core::doc::{Align, Block};
use ezpzjtd_core::edit::{Editor, Move};

fn text(e: &Editor) -> String {
    e.doc.plain_text()
}

#[test]
fn type_enter_backspace_undo() {
    let mut e = Editor::blank();
    e.insert_text("あいう");
    e.enter();
    e.insert_text("えお");
    assert_eq!(text(&e), "あいう\nえお\n");
    e.move_caret(Move::LineStart, false);
    e.backspace(); // merge paragraphs
    assert_eq!(text(&e), "あいうえお\n");
    assert!(e.undo());
    assert_eq!(text(&e), "あいう\nえお\n");
    assert!(e.redo());
    assert_eq!(text(&e), "あいうえお\n");
}

#[test]
fn typing_is_one_undo_step() {
    let mut e = Editor::blank();
    for c in "一太郎".chars() {
        e.insert_text(&c.to_string());
    }
    e.undo();
    assert_eq!(text(&e), "\n");
}

#[test]
fn selection_replace_and_delete_across_paragraphs() {
    let mut e = Editor::blank();
    e.insert_text("abc\ndef\nghi");
    e.move_caret(Move::DocStart, false);
    e.move_caret(Move::Right, false);
    for _ in 0..5 {
        e.move_caret(Move::Right, true);
    }
    assert_eq!(e.selected_text(), "bc\nde");
    e.insert_text("X");
    assert_eq!(text(&e), "aXf\nghi\n");
}

#[test]
fn bold_selection_and_pending_style() {
    let mut e = Editor::blank();
    e.insert_text("太字");
    e.move_caret(Move::LineStart, false);
    e.move_caret(Move::Right, true);
    e.toggle_bold();
    let Block::Paragraph(p) = &e.doc.sheets[0].blocks[0] else {
        panic!()
    };
    assert_eq!(p.runs.len(), 2);
    assert_eq!(p.runs[0].style.bold, Some(true));
    // collapsed caret: next typed text is bold
    e.move_caret(Move::LineEnd, false);
    e.toggle_bold();
    e.insert_text("!");
    let Block::Paragraph(p) = &e.doc.sheets[0].blocks[0] else {
        panic!()
    };
    assert_eq!(p.runs.last().unwrap().style.bold, Some(true));
    assert_eq!(p.runs.last().unwrap().text, "!");
}

#[test]
fn size_step_and_align() {
    let mut e = Editor::blank();
    e.insert_text("見出し");
    e.select_all();
    e.size_step(true);
    assert_eq!(e.caret_style().size, 11.0);
    e.set_align(Align::Center);
    assert_eq!(e.caret_style().align, Align::Center);
}

#[test]
fn table_insert_and_tab_navigation() {
    let mut e = Editor::blank();
    assert!(e.insert_table(2, 3));
    assert!(e.in_table());
    e.insert_text("A");
    assert!(e.next_cell(false));
    e.insert_text("B");
    let Block::Table(t) = &e.doc.sheets[0].blocks[0] else {
        panic!("no table")
    };
    // the line above the table carries its top line, then the 2 rows
    assert_eq!(t.rows.len(), 3);
    assert_eq!(t.rows[0].cells.len(), 1);
    assert_eq!(t.rows[1].cells.len(), 3);
    assert_eq!(t.rows[1].cells[1].paragraphs[0].plain_text(), "B");
    // a paragraph follows the table so the caret can leave it
    assert!(matches!(
        e.doc.sheets[0].blocks.last(),
        Some(Block::Paragraph(_))
    ));
    assert!(e.insert_row(true));
    let Block::Table(t) = &e.doc.sheets[0].blocks[0] else {
        panic!()
    };
    assert_eq!(t.rows.len(), 4);
}

/// Indents shift the lines (the first line has its own), and the line feed
/// (改行幅) moves the next line by a part of the normal feed.
#[test]
fn indents_and_line_feed_in_layout() {
    use ezpzjtd_core::doc::{Indent, LineFeed};
    let mut e = Editor::blank();
    e.insert_text(&"あ".repeat(100));
    let plain: Vec<(f32, f32)> = e.layout().lines.iter().map(|l| (l.left, l.top)).collect();
    let cell = e.setup.cell();
    if let Block::Paragraph(p) = &mut e.doc.sheets[0].blocks[0] {
        p.indent = Some(Indent {
            mm: false,
            left: 4,
            right: 0,
            first_left: 2,
            first_right: 0,
        });
        p.feed = Some(LineFeed { kind: 2, value: 0 });
    }
    e.relayout();
    let pitch = e.setup.pitch();
    let l = &e.layout().lines;
    assert!(
        (l[0].left - plain[0].0 - cell).abs() < 0.01,
        "first line: 2 columns"
    );
    assert!(
        (l[1].left - plain[1].0 - cell * 2.0).abs() < 0.01,
        "others: 4 columns"
    );
    // 39 characters fit on the first line, 38 on the next
    assert_eq!(l[0].end - l[0].start, 39);
    assert_eq!(l[1].end - l[1].start, 38);
    assert!((l[1].top - l[0].top - pitch / 2.0).abs() < 0.01);
}

/// The page setup of a document decides the line length: the characters
/// are spread over the width between the margins.
#[test]
fn chars_per_line_follow_the_page_setup() {
    let mut e = Editor::blank();
    e.setup.chars_per_line = 30;
    e.insert_text(&"あ".repeat(61));
    assert!((e.setup.cell() - 150.0 * 72.0 / 25.4 / 30.0).abs() < 0.01);
    let l = &e.layout().lines;
    assert_eq!(l.len(), 3);
    assert_eq!(l[0].end, 30);
}

#[test]
fn page_break_and_pages() {
    let mut e = Editor::blank();
    e.insert_text("一ページ目");
    e.page_break();
    e.insert_text("二ページ目");
    assert_eq!(e.layout().pages.len(), 2);
    assert_eq!(e.status().page, 2);
}

#[test]
fn wrapping_follows_the_grid() {
    let mut e = Editor::blank();
    e.insert_text(&"あ".repeat(85)); // 40 per line → 3 lines
    let lines = e.layout().lines.len();
    assert_eq!(lines, 3);
    e.move_caret(Move::DocEnd, false);
    let st = e.status();
    assert_eq!((st.line, st.col), (3, 6));
}

#[test]
fn kinsoku_hangs_punctuation() {
    let mut e = Editor::blank();
    e.insert_text(&format!("{}。次", "あ".repeat(40)));
    // 。 must not start a line: it hangs on line 1
    let l = &e.layout().lines;
    assert_eq!(l[0].end, 41);
}

#[test]
fn click_round_trip() {
    let mut e = Editor::blank();
    e.insert_text("あいうえお");
    e.move_caret(Move::DocStart, false);
    e.move_caret(Move::Right, false);
    e.move_caret(Move::Right, false);
    let r = e.caret_rect().unwrap();
    e.move_caret(Move::DocEnd, false);
    e.click(r.page, r.x + 1.0, r.y + r.h / 2.0, false);
    assert_eq!(e.caret.off, 2);
}

#[test]
fn find_and_replace_all() {
    let mut e = Editor::blank();
    e.insert_text("年度と年度\n年度");
    e.move_caret(Move::DocStart, false);
    assert!(e.find("年度", false));
    assert_eq!(e.selected_text(), "年度");
    assert_eq!(e.replace_all("年度", "年"), 3);
    assert_eq!(text(&e), "年と年\n年\n");
}

#[test]
fn docx_is_a_zip_with_document() {
    let mut e = Editor::blank();
    e.insert_text("こんにちは");
    e.insert_table(1, 2);
    let bytes = ezpzjtd_core::docx::to_docx(&e.doc, None, &e.setup);
    assert_eq!(&bytes[..2], b"PK");
    let s = String::from_utf8_lossy(&bytes);
    assert!(s.contains("word/document.xml"));
    assert!(s.contains("こんにちは"));
    assert!(s.contains("<w:tbl>"));
}

/// Every corpus file can be laid out and its caret moved to the end.
#[test]
fn local_corpus_layout() {
    let Ok(dir) = std::env::var("EZPZJTD_CORPUS") else {
        return;
    };
    for ent in std::fs::read_dir(dir).unwrap() {
        let p = ent.unwrap().path();
        if p.extension().map(|x| x != "jtd").unwrap_or(true) {
            continue;
        }
        let bytes = std::fs::read(&p).unwrap();
        let Ok(doc) = ezpzjtd_core::open(bytes) else {
            continue;
        };
        let mut e = Editor::new(doc);
        assert!(!e.layout().pages.is_empty());
        e.move_caret(Move::DocEnd, false);
        assert!(e.caret_rect().is_some(), "{}: no caret at end", p.display());
        for _ in 0..5 {
            e.move_caret(Move::Up, false);
        }
        e.insert_text("テスト");
        e.undo();
        let _ = ezpzjtd_core::docx::to_docx(&e.doc, None, &e.setup);
    }
}

/// Random editing must never panic (simple xorshift, no extra dependency).
#[test]
fn random_editing_does_not_panic() {
    let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut rnd = move |n: u64| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed % n.max(1)
    };
    let mut docs = vec![Editor::blank()];
    if let Ok(dir) = std::env::var("EZPZJTD_CORPUS") {
        for ent in std::fs::read_dir(dir).unwrap().take(12) {
            let p = ent.unwrap().path();
            if p.extension().map(|x| x == "jtd").unwrap_or(false) {
                if let Ok(d) = ezpzjtd_core::open(std::fs::read(&p).unwrap()) {
                    docs.push(Editor::new(d));
                }
            }
        }
    }
    let moves = [
        Move::Left,
        Move::Right,
        Move::Up,
        Move::Down,
        Move::LineStart,
        Move::LineEnd,
        Move::DocStart,
        Move::DocEnd,
        Move::PageUp,
        Move::PageDown,
        Move::WordLeft,
        Move::WordRight,
    ];
    for e in docs.iter_mut() {
        for _ in 0..600 {
            match rnd(20) {
                0..=3 => {
                    e.insert_text(["あ", "漢字", "a", "。", "\t", "一\n二", "「"][rnd(7) as usize])
                }
                4 => e.enter(),
                5 => e.backspace(),
                6 => e.delete_forward(),
                7..=9 => e.move_caret(moves[rnd(moves.len() as u64) as usize], rnd(3) == 0),
                10 => e.toggle_bold(),
                11 => e.size_step(rnd(2) == 0),
                12 => e.set_align(Align::Center),
                13 => {
                    e.undo();
                }
                14 => {
                    e.redo();
                }
                15 => {
                    e.insert_table(1 + rnd(3) as usize, 1 + rnd(4) as usize);
                }
                16 => {
                    e.next_cell(rnd(2) == 0);
                }
                17 => {
                    if rnd(2) == 0 {
                        e.insert_row(true);
                    } else {
                        e.delete_row();
                    }
                }
                18 => e.page_break(),
                _ => {
                    let pages = e.layout().pages.len();
                    let p = rnd(pages as u64) as usize;
                    e.click(p, rnd(600) as f32, rnd(840) as f32, rnd(4) == 0);
                }
            }
            let _ = e.caret_rect();
            let _ = e.selection_rects();
            let _ = e.status();
        }
        let _ = ezpzjtd_core::docx::to_docx(&e.doc, None, &e.setup);
    }
}

/// A new table follows the geometry of Ichitaro's own files (spec §4.3): every
/// vertical rule takes 2 grid units, cells sit between rules, and each line
/// adds up to the table width. The line above holds the top line, and every
/// row has a line under each cell, so the grid is closed.
#[test]
fn new_table_rules_add_up() {
    // the end of a ruled line: cut last item adds 1, a full one a + dist + 2
    fn line_end(r: &ezpzjtd_core::doc::Row) -> u32 {
        let mut c = r.x0 as u32 + 1;
        for (i, it) in r.rules.iter().enumerate() {
            let cut = i + 1 == r.rules.len() && it.a == 0 && it.b == 0 && it.dist == 0;
            c += if cut {
                1
            } else {
                it.a as u32 + it.dist as u32 + 2
            };
        }
        c
    }
    for chars in [40u32, 90] {
        for cols in 1..=7 {
            let mut e = Editor::blank();
            e.setup.chars_per_line = chars;
            assert!(e.insert_table(2, cols));
            let Block::Table(t) = &e.doc.sheets[0].blocks[0] else {
                panic!("no table")
            };
            let w = t.width;
            assert_eq!(w as u32, chars * 4);
            let rows = &t.rows[1..];
            let right = rows[0].cells.last().unwrap().right + 1; // closing rule centre
            for row in std::iter::once(&t.rows[0]).chain(rows) {
                assert_eq!(line_end(row), w as u32, "{chars}/{cols}: line adds up");
                for it in &row.rules {
                    assert!(it.a <= 0xf0 && it.dist <= 0xf0, "{chars}/{cols}: {it:?}");
                }
            }
            // the line above: one line under it, across the table
            let top = t.rows[0].lines();
            assert!(top.verticals.is_empty());
            let under: u32 = top.below.iter().map(|(a, b)| (b - a) as u32).sum();
            assert_eq!(top.below.first().unwrap().0, 1);
            assert_eq!(under, right as u32 - 1, "{chars}/{cols}");
            for row in rows {
                let mut x = 0u16;
                for (c, cell) in row.cells.iter().enumerate() {
                    assert_eq!(cell.left, x + 2, "{chars}/{cols} cell {c}");
                    assert_eq!(row.rules[c].dist, cell.right - cell.left);
                    x = cell.right;
                }
                let g = row.lines();
                assert_eq!(g.verticals.len(), cols + 1);
                assert_eq!(g.verticals.last().unwrap().0, right);
                let under: u32 = g.below.iter().map(|(a, b)| (b - a) as u32).sum();
                assert_eq!(
                    under,
                    right as u32 - 1,
                    "{chars}/{cols}: a line under every cell"
                );
            }
        }
    }
}
