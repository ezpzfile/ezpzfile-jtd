fn main() { let p = std::env::args().nth(1).unwrap(); let d = ezpzjtd_core::open(std::fs::read(&p).unwrap()).unwrap(); print!("{}", d.plain_text()); }
