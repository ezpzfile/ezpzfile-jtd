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


fn be(v: u32) -> [u8; 4] {
    v.to_be_bytes()
}

/// /DocumentText with one text sub-stream: `units`, then raw style events.
fn styled_text(units: &[u16], style: &[u8]) -> Vec<u8> {
    let mut payload = b"TextV.01".to_vec();
    payload.extend(be(units.len() as u32));
    for u in units {
        payload.extend(u.to_be_bytes());
    }
    payload.extend(style);
    let len = payload.len();
    let nb = len.div_ceil(256);
    payload.resize(nb * 256, 0);
    let mut b = b"SsmgV.01".to_vec();
    b.extend(be(1));
    b.extend(be(256));
    b.extend(be(nb as u32));
    b.extend(&payload);
    b.extend(be(0));
    b.extend(be(0));
    b.extend(be(len as u32));
    b.extend(be(nb as u32));
    b.extend(be(nb as u32));
    for i in 0..nb as u32 {
        b.extend(be(i));
    }
    b.extend(be(0));
    b
}

/// Style-event fuzzer (0.3.3): random event lists (runs, property sets with
/// many ids and values) over a seed's text; open + layout must stay small.
/// `cargo run --release --example stylefuzz -- <iterations> seed.jtd`
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let iters: usize = args[1].parse().unwrap();
    let seed = std::fs::read(&args[2]).unwrap();
    let cfb = Cfb::open(seed).unwrap();
    let mut rng = Rng(0x2545_f491_4f6c_dd1d);
    let (mut worst_ms, mut worst_mb, mut panics, mut errs) = (0u128, 0usize, 0, 0);
    for it in 0..iters {
        let n = 1 + rng.below(60_000);
        let per = 1 + rng.below(2000);
        let mut units = Vec::with_capacity(n + n / per + 1);
        for i in 0..n {
            units.push(if rng.below(20) == 0 { 0x41 } else { 0x3042 });
            if (i + 1) % per == 0 {
                units.push(0x0a);
            }
        }
        let mut st = Vec::new();
        let events = rng.below(20_000);
        let wide = 1 + rng.below(255);
        for _ in 0..events {
            if rng.below(3) == 0 {
                st.push(0x00);
                st.extend(be(match rng.below(4) {
                    0 => u32::MAX,
                    1 => rng.below(10) as u32,
                    _ => rng.below(n) as u32,
                }));
            } else {
                st.push(0xfe);
                for _ in 0..1 + rng.below(wide) {
                    let id = rng.below(255) as u8;
                    let len = rng.below(5);
                    st.extend([id, len as u8]);
                    for _ in 0..len {
                        st.push(rng.next() as u8);
                    }
                }
                st.extend([0xff, 0x00]);
            }
        }
        let mut t = Tree::from_cfb(&cfb);
        t.set_stream("/DocumentText", styled_text(&units, &st));
        // the simple CFB writer stops at about 7 MB
        let Ok(file) = std::panic::catch_unwind(|| write(&t)) else { continue };
        if it == 0 {
            std::panic::set_hook(Box::new(|_| {}));
        }
        let base = NOW.load(Ordering::Relaxed);
        PEAK.store(base, Ordering::Relaxed);
        let t0 = Instant::now();
        let r = std::panic::catch_unwind(move || match ezpzjtd_core::open(file) {
            Ok(doc) => {
                let mut ed = ezpzjtd_core::edit::Editor::new(doc);
                Some(ed.layout().pages.len())
            }
            Err(_) => None,
        });
        let ms = t0.elapsed().as_millis();
        let mb = PEAK.load(Ordering::Relaxed).saturating_sub(base) >> 20;
        match r {
            Err(_) => {
                panics += 1;
                println!("#{it} panic");
            }
            Ok(None) => errs += 1,
            Ok(Some(_)) => {}
        }
        if ms > worst_ms || mb > worst_mb {
            worst_ms = worst_ms.max(ms);
            worst_mb = worst_mb.max(mb);
            println!("#{it} units={n} events={events} wide={wide}: {ms}ms {mb}MB");
        }
    }
    println!("done {iters}: worst {worst_ms}ms {worst_mb}MB, panics {panics}, errors {errs}");
}
