//! Saving as .jtd: unchanged files stay identical, edited files reopen with
//! the edited content. Uses the local corpus when `EZPZJTD_CORPUS` is set.

use ezpzjtd_core::doc::{Align, Block, Paragraph};
use ezpzjtd_core::edit::{normalize_blocks, Editor, Move};
use ezpzjtd_core::save;

fn corpus() -> Vec<(String, Vec<u8>)> {
    let Ok(dir) = std::env::var("EZPZJTD_CORPUS") else {
        return Vec::new();
    };
    let mut v: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().map(|x| x == "jtd").unwrap_or(false))
        .collect();
    v.sort();
    v.into_iter()
        .map(|p| (p.display().to_string(), std::fs::read(&p).unwrap()))
        .collect()
}

fn paras(blocks: &[Block]) -> Vec<&Paragraph> {
    let mut v = Vec::new();
    for b in blocks {
        match b {
            Block::Paragraph(p) => v.push(p),
            Block::Table(t) => {
                for r in &t.rows {
                    for c in &r.cells {
                        v.extend(c.paragraphs.iter());
                    }
                }
            }
        }
    }
    v
}

/// Compare what a save must keep: text, structure, storable formatting.
fn same_content(a: &[Block], b: &[Block]) -> Result<(), String> {
    let (mut a, mut b) = (a.to_vec(), b.to_vec());
    // empty lines at the very end are not kept (the reader drops them too)
    for v in [&mut a, &mut b] {
        while v.len() > 1
            && matches!(v.last(), Some(Block::Paragraph(p)) if p.is_empty() && !p.page_break_before)
        {
            v.pop();
        }
    }
    normalize_blocks(&mut a);
    normalize_blocks(&mut b);
    let (pa, pb) = (paras(&a), paras(&b));
    if pa.len() != pb.len() {
        return Err(format!("paragraph count {} vs {}", pa.len(), pb.len()));
    }
    for (k, (x, y)) in pa.iter().zip(&pb).enumerate() {
        if x.plain_text() != y.plain_text() {
            return Err(format!(
                "paragraph {k}: {:?} vs {:?}",
                x.plain_text(),
                y.plain_text()
            ));
        }
        if x.page_break_before != y.page_break_before {
            return Err(format!("paragraph {k}: page break"));
        }
        // indents and line feed (an empty line may be one the reader adds)
        if (x.indent, x.feed) != (y.indent, y.feed) && !x.is_empty() {
            return Err(format!(
                "paragraph {k}: indent / line feed {:?} vs {:?}",
                (x.indent, x.feed),
                (y.indent, y.feed)
            ));
        }
        // inline runs (ruby / 均等割付) carry the look of their first character
        let looks = |p: &Paragraph| -> Vec<Option<ezpzjtd_core::style::CharStyle>> {
            p.runs
                .iter()
                .flat_map(|r| {
                    r.text
                        .chars()
                        .enumerate()
                        .map(move |(i, _)| (!r.inline || i == 0).then(|| r.style.clone()))
                })
                .collect()
        };
        let (lx, ly) = (looks(x), looks(y));
        for (i, (s, t)) in lx.iter().zip(&ly).enumerate() {
            let (Some(s), Some(t)) = (s, t) else { continue };
            if s.bold != t.bold
                || s.size_pt != t.size_pt
                || s.underline != t.underline
                || s.color != t.color
            {
                return Err(format!("paragraph {k} char {i}: {s:?} vs {t:?}"));
            }
        }
    }
    Ok(())
}

#[test]
fn unchanged_documents_are_written_back_unchanged() {
    for (name, bytes) in corpus() {
        let Ok(doc) = ezpzjtd_core::open(bytes.clone()) else {
            continue;
        };
        let s = save::save(&bytes, &doc).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(s.changed, 0, "{name}");
        assert_eq!(s.bytes, bytes, "{name}");
    }
}

struct Rng(u64);
impl Rng {
    fn next(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % n.max(1)
    }
}

/// Edit, save, reopen, compare. `level` 0 = text only, 1 = + formatting,
/// 2 = + page breaks and tables.
fn session(e: &mut Editor, rng: &mut Rng, steps: usize, level: u64) -> Vec<String> {
    let mut log = Vec::new();
    let moves = [
        Move::Left,
        Move::Right,
        Move::Up,
        Move::Down,
        Move::LineStart,
        Move::LineEnd,
        Move::DocStart,
        Move::DocEnd,
        Move::WordLeft,
        Move::WordRight,
        Move::PageDown,
    ];
    for _ in 0..steps {
        let kinds = [7, 11, 14][level as usize];
        let op = rng.next(kinds);
        log.push(format!("{op}@{:?}", e.caret));
        match op {
            0 | 1 => e.insert_text(
                ["あ", "漢字", "a", "。", "テスト文", "一\n二", "「」"][rng.next(7) as usize],
            ),
            2 => e.enter(),
            3 => e.backspace(),
            4 => e.delete_forward(),
            5 | 6 => {
                for _ in 0..1 + rng.next(4) {
                    e.move_caret(
                        moves[rng.next(moves.len() as u64) as usize],
                        rng.next(3) == 0,
                    )
                }
            }
            7 => e.toggle_bold(),
            8 => e.size_step(rng.next(2) == 0),
            9 => e.set_color(if rng.next(2) == 0 {
                Some("#d40000".into())
            } else {
                None
            }),
            10 => e.set_align([Align::Left, Align::Center, Align::Right][rng.next(3) as usize]),
            11 => e.page_break(),
            12 => {
                if rng.next(2) == 0 {
                    e.insert_row(true);
                } else {
                    e.delete_row();
                }
            }
            _ => {
                e.insert_table(1 + rng.next(3) as usize, 2 + rng.next(3) as usize);
            }
        }
    }
    log
}

#[test]
fn edited_documents_reopen_with_the_edits() {
    let docs = corpus();
    if docs.is_empty() {
        return;
    }
    let mut rng = Rng(0x2545_F491_4F6C_DD1D);
    let mut stats = [[0usize; 3]; 3]; // level → [ok, refused, total]
    let mut refusals: std::collections::BTreeMap<String, usize> = Default::default();
    for (name, bytes) in &docs {
        let Ok(doc) = ezpzjtd_core::open(bytes.clone()) else {
            continue;
        };
        for level in 0..3u64 {
            for _ in 0..3 {
                let mut e = Editor::new(doc.clone());
                let log = session(&mut e, &mut rng, 25, level);
                stats[level as usize][2] += 1;
                match save::save(bytes, &e.doc) {
                    Ok(s) => {
                        stats[level as usize][0] += 1;
                        let back = ezpzjtd_core::open(s.bytes.clone()).unwrap_or_else(|err| {
                            panic!("{name}: saved file does not open: {err}")
                        });
                        assert_eq!(back.sheets.len(), e.doc.sheets.len(), "{name}");
                        for (x, y) in back.sheets.iter().zip(&e.doc.sheets) {
                            if let Err(m) = same_content(&x.blocks, &y.blocks) {
                                panic!("{name} level {level}: {m}");
                            }
                        }
                        // saving the saved file again without edits changes nothing
                        let again = save::save(&s.bytes, &back).unwrap();
                        assert_eq!(again.changed, 0, "{name}");
                    }
                    Err(err) => {
                        stats[level as usize][1] += 1;
                        let key: String = err.to_string().chars().take(60).collect();
                        let n = refusals.entry(format!("L{level} {key}")).or_default();
                        if *n == 0
                            && std::env::var("SAVE_SHOW")
                                .map(|v| key.contains(&v))
                                .unwrap_or(false)
                        {
                            eprintln!(
                                "FIRST L{level} {key}\n  file {name}\n  ops {}",
                                log.join(" ")
                            );
                            std::env::set_var("EZPZJTD_SAVE_DEBUG", "1");
                            let _ = save::save(bytes, &e.doc);
                            std::env::remove_var("EZPZJTD_SAVE_DEBUG");
                        }
                        *n += 1;
                    }
                }
            }
        }
    }
    eprintln!("save stats per level [ok, refused, total]: {stats:?}");
    for (k, v) in &refusals {
        eprintln!("  {v:4}  {k}");
    }
    // Text-only editing is refused only for the edits that are known not to
    // be storable yet (joining lines across a ruled box, a line break inside
    // ruby), and every level saves almost every session.
    let known = |k: &str| k.contains("罫線の枠") || k.contains("ルビ");
    let odd: Vec<_> = refusals
        .iter()
        .filter(|(k, _)| k.starts_with("L0") && !known(k))
        .collect();
    assert!(odd.len() <= 8, "unexpected refusals of text edits: {odd:?}");
    for (lv, s) in stats.iter().enumerate() {
        assert!(
            s[0] * 100 >= s[2] * 85,
            "level {lv} saved only {} of {}",
            s[0],
            s[2]
        );
    }
}
