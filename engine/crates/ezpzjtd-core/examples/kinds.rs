fn main() {
    for p in std::env::args().skip(1) {
        let b = std::fs::read(&p).unwrap();
        let k = ezpzjtd_core::sniff::kind(&b);
        let r = ezpzjtd_core::open(b).map(|d| d.plain_text().chars().count().to_string()).unwrap_or_else(|e| format!("ERR {e}"));
        println!("{k}\t{r}\t{p}");
    }
}
