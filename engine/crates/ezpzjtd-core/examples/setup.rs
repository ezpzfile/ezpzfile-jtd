fn main() { for p in std::env::args().skip(1) { let d = ezpzjtd_core::open(std::fs::read(&p).unwrap()).unwrap(); println!("{} {:?}", p.rsplit('/').next().unwrap(), d.page); } }
