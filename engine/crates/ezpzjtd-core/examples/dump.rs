fn main() {
    let a: Vec<String> = std::env::args().collect();
    let c = ezpzjtd_core::cfb::Cfb::open(std::fs::read(&a[1]).unwrap()).unwrap();
    let b = c.read(&a[2]).unwrap();
    for (i, ch) in b.chunks(16).enumerate() {
        let hex: Vec<String> = ch.iter().map(|x| format!("{x:02x}")).collect();
        let u16s: String = ch.chunks(2).map(|w| { let v = if w.len()==2 { u16::from_be_bytes([w[0],w[1]]) } else {0}; char::from_u32(v as u32).filter(|c| !c.is_control() && v >= 0x3000).unwrap_or('.') }).collect();
        println!("{:05x}  {:<48} {}", i*16, hex.join(" "), u16s);
    }
}
