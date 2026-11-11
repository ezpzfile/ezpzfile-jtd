fn main() {
    for p in std::env::args().skip(1) {
        let c = ezpzjtd_core::cfb::Cfb::open(std::fs::read(&p).unwrap()).unwrap();
        let b = c.read("/DocumentText").unwrap();
        let subs = ezpzjtd_core::ssmg::substreams(&b).map(|s| s.iter().map(|x| String::from_utf8_lossy(&x[..x.len().min(8)]).to_string()).collect::<Vec<_>>().join(",")).unwrap_or_default();
        println!("{}\t{}", p.rsplit('/').next().unwrap(), subs.chars().take(60).collect::<String>());
    }
}
