// dump /DocumentViewStyles records 1002, 100B, 100D: vrec file...
fn main() {
    for p in std::env::args().skip(1) {
        let c = ezpzjtd_core::cfb::Cfb::open(std::fs::read(&p).unwrap()).unwrap();
        let Some(b) = c.read("/DocumentViewStyles") else { continue };
        let block_end = (10 + u32::from_be_bytes([b[6], b[7], b[8], b[9]]) as usize).min(b.len());
        let mut i = 10;
        print!("{}:", p.rsplit('/').next().unwrap());
        while i + 4 <= block_end {
            let tag = u16::from_be_bytes([b[i], b[i + 1]]);
            let len = u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize;
            let end = (i + 4 + len).min(block_end);
            if std::env::var("ALL").is_ok() || matches!(tag, 0x1002 | 0x100b | 0x100d | 0x100c | 0x100a) {
                print!(" [{tag:04x}] {}", b[i + 4..end].iter().map(|x| format!("{x:02x}")).collect::<Vec<_>>().join(""));
            }
            i += 4 + len;
        }
        println!();
    }
}
