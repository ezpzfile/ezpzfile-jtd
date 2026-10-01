//! Make edited copies of corpus files with the save engine, for checking in
//! Ichitaro Viewer. usage: savegen <corpus_dir> <out_dir> [files] [steps]
//! Writes <name>-L<level>.jtd for every session that saved.
use ezpzjtd_core::doc::Align;
use ezpzjtd_core::edit::{Editor, Move};

struct Rng(u64);
impl Rng {
    fn next(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % n.max(1)
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let limit: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(1000);
    let steps: usize = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(20);
    std::fs::create_dir_all(&a[2]).unwrap();
    let mut files: Vec<_> = std::fs::read_dir(&a[1])
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().map(|x| x == "jtd").unwrap_or(false))
        .collect();
    files.sort();
    let moves = [
        Move::Right,
        Move::Down,
        Move::Down,
        Move::LineEnd,
        Move::WordRight,
        Move::Left,
        Move::PageDown,
    ];
    let (mut ok, mut refused) = (0, 0);
    for (fi, f) in files.iter().take(limit).enumerate() {
        let bytes = std::fs::read(f).unwrap();
        let Ok(doc) = ezpzjtd_core::open(bytes.clone()) else {
            continue;
        };
        for level in 0..3u64 {
            let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ ((fi as u64) << 8) ^ level);
            let mut e = Editor::new(doc.clone());
            for _ in 0..steps {
                match rng.next([6, 10, 12][level as usize]) {
                    0 | 1 => e.insert_text(
                        ["【EZPZ】", "漢字テスト", "追記。", "一\n二"][rng.next(4) as usize],
                    ),
                    2 => e.enter(),
                    3 => e.backspace(),
                    4 | 5 => {
                        for _ in 0..1 + rng.next(6) {
                            e.move_caret(moves[rng.next(moves.len() as u64) as usize], false)
                        }
                    }
                    6 => {
                        e.move_caret(Move::WordRight, true);
                        e.toggle_bold()
                    }
                    7 => {
                        e.move_caret(Move::WordRight, true);
                        e.set_color(Some("#d40000".into()))
                    }
                    8 => {
                        e.move_caret(Move::WordRight, true);
                        e.size_step(true)
                    }
                    9 => e.set_align(
                        [Align::Left, Align::Center, Align::Right][rng.next(3) as usize],
                    ),
                    10 => e.page_break(),
                    _ => {
                        if !e.insert_row(true) {
                            e.insert_table(2, 3);
                        }
                    }
                }
            }
            let name = f.file_stem().unwrap().to_string_lossy().to_string();
            match ezpzjtd_core::save::save(&bytes, &e.doc) {
                Ok(s) if s.changed > 0 => {
                    std::fs::write(format!("{}/{name}-L{level}.jtd", a[2]), &s.bytes).unwrap();
                    ok += 1;
                }
                Ok(_) => {}
                Err(_) => refused += 1,
            }
        }
    }
    println!("saved {ok}, refused {refused}");
}
