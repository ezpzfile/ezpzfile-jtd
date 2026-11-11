fn main() {
    use std::collections::BTreeMap;
    let mut m: BTreeMap<String, (usize, u64, String)> = BTreeMap::new();
    for p in std::env::args().skip(1) {
        let b = std::fs::read(&p).unwrap();
        let Ok(c) = ezpzjtd_core::cfb::Cfb::open(b) else { continue };
        for e in c.entries() {
            // collapse numbered names
            let k: String = e.path.chars().map(|ch| if ch.is_ascii_digit() { '#' } else { ch }).collect();
            let v = m.entry(k).or_insert((0, 0, p.clone()));
            v.0 += 1;
            v.1 += e.size as u64;
        }
    }
    for (k, (n, s, ex)) in m { println!("{n:4} {s:9} {k}  e.g. {}", ex.rsplit('/').next().unwrap()); }
}
