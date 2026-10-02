//! For each ruled line (TLV 0x8f), print the style properties 1, 2 and 8 that
//! sit on the words of each item: Ichitaro keeps line types (線種) there.
//! usage: ruletypes <file.jtd> [max_lines]
use ezpzjtd_core::{cfb::Cfb, doc, text};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let max: usize = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(10000);
    let cfb = Cfb::open(std::fs::read(&a[1]).unwrap()).unwrap();
    let raw = cfb.read("/DocumentText").unwrap();
    let (units, map) = doc::units_and_styles(&raw).unwrap();
    let mut shown = 0;
    for t in text::tokenize(&units) {
        let text::Token::Record {
            start,
            class: 0x10,
            payload,
            ..
        } = t
        else {
            continue;
        };
        // find the 0x8f item list inside the payload (payload starts at unit start+3)
        let mut j = 1usize;
        while j + 1 < payload.len() && payload[j] != 0xffff {
            let (tag, n) = (payload[j], payload[j + 1] as usize);
            if tag == 0x8f {
                let v0 = start + 3 + j + 2; // first value unit
                let mut line = String::new();
                let mut any = false;
                for k in 3..n {
                    let u = v0 + k;
                    let p = map.at(u);
                    let g = |id: u8| p.and_then(|p| p.get(&id).copied());
                    let f = |v: Option<u32>| match v {
                        None => "-".to_string(),
                        Some(0xffff) => "~".to_string(),
                        Some(x) => format!("{x}"),
                    };
                    let (p1, p2, p8) = (g(1), g(2), g(8));
                    if p1.is_some_and(|v| v != 0xffff) || p8.is_some_and(|v| v != 0xffff) {
                        any = true;
                    }
                    line.push_str(&format!(
                        "{:x}[{} {} {}] ",
                        payload[j + 2 + k],
                        f(p1),
                        f(p2),
                        f(p8)
                    ));
                }
                if any && shown < max {
                    println!("@{start} {line}");
                    shown += 1;
                }
            }
            j += 2 + n;
        }
    }
}
