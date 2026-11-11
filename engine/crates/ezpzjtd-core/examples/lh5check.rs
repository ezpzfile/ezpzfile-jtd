// Decode the first member of an .lzh file and write it to stdout (to compare with `lha -p`).
use std::io::Write;
fn main() {
    let p = std::env::args().nth(1).unwrap();
    let b = std::fs::read(&p).unwrap();
    match ezpzjtd_core::lh5::unpack_first(&b, 1 << 30) {
        Ok(Some(v)) => std::io::stdout().write_all(&v).unwrap(),
        Ok(None) => { eprintln!("no member"); std::process::exit(2) }
        Err(e) => { eprintln!("error {e}"); std::process::exit(1) }
    }
}
