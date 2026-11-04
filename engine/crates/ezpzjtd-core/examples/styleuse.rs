//! How many style property entries real files use (to set `max_style_entries`).
//! `cargo run --release --example styleuse -- a.jtd b.jtd …`
use ezpzjtd_core::cfb::Cfb;
fn main() {
    let mut worst = (0usize, String::new());
    for f in std::env::args().skip(1) {
        let Ok(cfb) = Cfb::open(std::fs::read(&f).unwrap()) else { continue };
        let mut used = 0usize;
        for e in cfb.entries() {
            if e.path.ends_with("/DocumentText") {
                if let Some(raw) = cfb.read(&e.path) {
                    let mut b = usize::MAX / 2;
                    if ezpzjtd_core::doc::units_and_styles_with(&raw, &mut b).is_ok() {
                        used += usize::MAX / 2 - b;
                    }
                }
            }
        }
        if used > worst.0 {
            worst = (used, f.clone());
        }
    }
    println!("max entries used: {} ({})", worst.0, worst.1);
}
