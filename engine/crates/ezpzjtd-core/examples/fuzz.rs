//! Field fuzzer for hostile input: change length/count fields of public
//! sample files and check that open + layout stay small and fast.
//! `cargo run --release --example fuzz -- <iterations> <out_dir> seed1.jtd seed2.jtd …`
//! Prints the worst cases (time, peak memory, pages per KB) and any panic.
use ezpzjtd_core::cfb::Cfb;
use ezpzjtd_core::cfbw::{write, Tree};
use std::alloc::{GlobalAlloc, Layout as AL, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

struct Count;
static NOW: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for Count {
    unsafe fn alloc(&self, l: AL) -> *mut u8 {
        let n = NOW.fetch_add(l.size(), Ordering::Relaxed) + l.size();
        PEAK.fetch_max(n, Ordering::Relaxed);
        System.alloc(l)
    }
    unsafe fn dealloc(&self, p: *mut u8, l: AL) {
        NOW.fetch_sub(l.size(), Ordering::Relaxed);
        System.dealloc(p, l)
    }
}
#[global_allocator]
static A: Count = Count;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

const STREAMS: &[&str] = &["/DocumentText", "/DocumentViewStyles", "/Font", "/\u{5}SummaryInformation"];

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let iters: usize = args[1].parse().unwrap();
    let out_dir = &args[2];
    let seeds: Vec<Vec<u8>> = args[3..].iter().map(|p| std::fs::read(p).unwrap()).collect();
    std::panic::set_hook(Box::new(|_| {}));
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let (mut worst_ms, mut worst_mb, mut worst_ratio, mut panics, mut errs) = (0u128, 0usize, 0f64, 0, 0);
    for it in 0..iters {
        let seed = &seeds[rng.below(seeds.len())];
        let cfb = Cfb::open(seed.clone()).unwrap();
        let stream = STREAMS[if rng.below(4) == 0 { rng.below(STREAMS.len()) } else { 0 }];
        let Some(mut data) = cfb.read(stream) else { continue };
        if data.len() < 8 {
            continue;
        }
        for _ in 0..1 + rng.below(6) {
            let at = rng.below(data.len().saturating_sub(4));
            let v: u32 = match rng.below(6) {
                0 => 0,
                1 => u32::MAX,
                2 => 0x7fff_ffff,
                3 => rng.below(64) as u32,
                4 => 1 << rng.below(32),
                _ => rng.next() as u32,
            };
            let width = [1usize, 2, 4][rng.below(3)];
            let bytes = v.to_be_bytes();
            data[at..at + width].copy_from_slice(&bytes[4 - width..]);
        }
        let mut t = Tree::from_cfb(&cfb);
        t.set_stream(stream, data);
        let file = write(&t);
        let len = file.len();
        let base = NOW.load(Ordering::Relaxed);
        PEAK.store(base, Ordering::Relaxed);
        let t0 = Instant::now();
        let keep = file.clone();
        let r = std::panic::catch_unwind(move || match ezpzjtd_core::open(file) {
            Ok(doc) => {
                let mut ed = ezpzjtd_core::edit::Editor::new(doc);
                let pages = ed.layout().pages.len();
                let _ = ed.status();
                Some(pages)
            }
            Err(_) => None,
        });
        let ms = t0.elapsed().as_millis();
        let mb = PEAK.load(Ordering::Relaxed).saturating_sub(base) >> 20;
        let mut note = String::new();
        match r {
            Err(_) => {
                panics += 1;
                note = "panic".into();
            }
            Ok(None) => errs += 1,
            Ok(Some(p)) => {
                let ratio = p as f64 / (len as f64 / 1024.0);
                if ratio > worst_ratio {
                    worst_ratio = ratio;
                    note = format!("pages {p} ({ratio:.1}/KB)");
                }
            }
        }
        if ms > worst_ms {
            worst_ms = ms;
            note = format!("{note} slow {ms}ms");
        }
        if mb > worst_mb {
            worst_mb = mb;
            note = format!("{note} mem {mb}MB");
        }
        if !note.is_empty() {
            println!("#{it} {stream} {len}B: {note}");
            std::fs::write(format!("{out_dir}/case-{it}.jtd"), &keep).ok();
        }
    }
    println!("done {iters}: worst {worst_ms}ms {worst_mb}MB {worst_ratio:.1} pages/KB, panics {panics}, errors {errs}");
}
