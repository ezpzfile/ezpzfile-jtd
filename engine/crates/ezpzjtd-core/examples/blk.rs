fn main() {
    let a: Vec<String> = std::env::args().collect();
    let d = ezpzjtd_core::open(std::fs::read(&a[1]).unwrap()).unwrap();
    let (from, to): (usize, usize) = (a[2].parse().unwrap(), a[3].parse().unwrap());
    for (bi, b) in d.sheets[0].blocks.iter().enumerate().skip(from).take(to - from) {
        if let ezpzjtd_core::doc::Block::Paragraph(p) = b {
            println!("block {bi} feed={:?} runs:", p.feed);
            for r in &p.runs { println!("   {:?} size={:?}", r.text, r.style.size_pt); }
        }
    }
}
