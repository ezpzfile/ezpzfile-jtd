// print /Frame records: frames in.jtd
fn be16(b: &[u8], o: usize) -> u32 { u16::from_be_bytes([b[o], b[o + 1]]) as u32 }
fn be32(b: &[u8], o: usize) -> u32 { u32::from_be_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) }
fn main() {
    for p in std::env::args().skip(1) {
        let c = ezpzjtd_core::cfb::Cfb::open(std::fs::read(&p).unwrap()).unwrap();
        let Some(b) = c.read("/Frame") else { continue };
        println!("== {} head {:?}", p.rsplit('/').next().unwrap(), &b[..8.min(b.len())]);
        let mut o = 8;
        while o + 4 <= b.len() {
            let (t, l) = (be16(&b, o), be16(&b, o + 2) as usize);
            if l == 0 { break; }
            let r = &b[o..(o + 4 + l).min(b.len())];
            let words: Vec<String> = (4..r.len().saturating_sub(1)).step_by(2).map(|i| format!("{:x}", be16(r, i))).collect();
            if r.len() >= 42 { println!("  t={t:04x} len={l} a={:x} b={:x} c={:x} d={:x} x={} y={} w={} h={} | {}", be32(r,4), be32(r,8), be32(r,12), be32(r,16), be32(r,26) as i32, be32(r,30) as i32, be32(r,34), be32(r,38), words.join(" ")); } else { println!("  t={t:04x} len={l} {}", words.join(" ")); }
            o += 4 + l;
        }
    }
}
