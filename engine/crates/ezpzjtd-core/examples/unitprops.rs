//! Print the style properties at a range of DocumentText units.
//! usage: unitprops <file.jtd> <from> <to>
use ezpzjtd_core::{cfb::Cfb, doc};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (from, to): (usize, usize) = (a[2].parse().unwrap(), a[3].parse().unwrap());
    let cfb = Cfb::open(std::fs::read(&a[1]).unwrap()).unwrap();
    let raw = cfb.read("/DocumentText").unwrap();
    let (units, map) = doc::units_and_styles(&raw).unwrap();
    for u in from..to.min(units.len()) {
        println!("{u}\t{:04x}\t{:?}", units[u], map.at(u));
    }
}
