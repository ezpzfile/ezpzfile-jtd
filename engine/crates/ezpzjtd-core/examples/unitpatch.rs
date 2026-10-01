//! Research: insert a copy of units[a..b] at position p in /DocumentText.
//! usage: unitpatch <in.jtd> <out.jtd> <a> <b> <p> [dropcaches]
use ezpzjtd_core::cfb::Cfb;
use ezpzjtd_core::cfbw::{self, Tree};
use ezpzjtd_core::jtdw::TextV;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let bytes = std::fs::read(&a[1]).unwrap();
    let c = Cfb::open(bytes).unwrap();
    let mut tree = Tree::from_cfb(&c);
    let mut tv = TextV::parse(&c.read("/DocumentText").unwrap()).unwrap();
    let (s, e, p): (usize, usize, usize) = (
        a[3].parse().unwrap(),
        a[4].parse().unwrap(),
        a[5].parse().unwrap(),
    );
    let piece: Vec<u16> = tv.units[s..e].to_vec();
    tv.insert(p, &piece);
    tree.set_stream("/DocumentText", tv.encode());
    if a.get(6).is_some() {
        for x in ["/LineMark", "/PageMark", "/DocumentTextPositionTables"] {
            tree.remove(x);
        }
    }
    std::fs::write(&a[2], cfbw::write(&tree)).unwrap();
}
