//! Research: insert given units at position p in /DocumentText (caches dropped).
//! usage: unitins <in.jtd> <out.jtd> <p> <spec>
//! spec: comma list of hex words, "s:TEXT" for text, "u:a-b" to copy units a..b
use ezjtd_core::cfb::Cfb;
use ezjtd_core::cfbw::{self, Tree};
use ezjtd_core::jtdw::TextV;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let c = Cfb::open(std::fs::read(&a[1]).unwrap()).unwrap();
    let mut tree = Tree::from_cfb(&c);
    let mut tv = TextV::parse(&c.read("/DocumentText").unwrap()).unwrap();
    let p: usize = a[3].parse().unwrap();
    let mut v: Vec<u16> = Vec::new();
    for item in a[4].split(',') {
        if let Some(t) = item.strip_prefix("s:") {
            v.extend(t.encode_utf16());
        } else if let Some(r) = item.strip_prefix("u:") {
            let (x, y) = r.split_once('-').unwrap();
            v.extend_from_slice(
                &tv.units[x.parse::<usize>().unwrap()..y.parse::<usize>().unwrap()],
            );
        } else {
            v.push(u16::from_str_radix(item, 16).unwrap());
        }
    }
    tv.insert(p, &v);
    tree.set_stream("/DocumentText", tv.encode());
    if std::env::var_os("KEEP").is_none() {
        for x in ["/LineMark", "/PageMark", "/DocumentTextPositionTables"] {
            tree.remove(x);
        }
    }
    if std::env::var_os("NOFIX").is_none() {
        tree.fix_segment_info();
    }
    std::fs::write(&a[2], cfbw::write(&tree)).unwrap();
}
