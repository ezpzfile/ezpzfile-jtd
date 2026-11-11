// record classes in DocumentText: recs file [class-hex to print]
use ezpzjtd_core::text::{tokenize, Token};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let c = ezpzjtd_core::cfb::Cfb::open(std::fs::read(&a[1]).unwrap()).unwrap();
    let raw = c.read("/DocumentText").unwrap();
    let (units, _) = ezpzjtd_core::doc::units_and_styles(&raw).unwrap();
    let want = a.get(2).map(|s| u16::from_str_radix(s, 16).unwrap());
    let mut counts = std::collections::BTreeMap::new();
    let mut lasttext = String::new();
    for t in tokenize(&units) {
        match &t {
            Token::Text { text, .. } => { lasttext = text.chars().rev().take(8).collect::<String>().chars().rev().collect(); }
            Token::Record { class, payload, start } => {
                *counts.entry(*class).or_insert(0) += 1;
                if Some(*class) == want { println!("@{start} after {:?}: {:x?}", lasttext, &payload[..payload.len().min(24)]); }
            }
            Token::Inline { header, .. } => { *counts.entry(0xffff).or_insert(0) += 1; if want == Some(0xffff) { println!("inline {:x?}", &header[..header.len().min(20)]); } }
            _ => {}
        }
    }
    if want.is_none() { for (k, v) in counts { println!("{k:04x} x{v}"); } }
}
