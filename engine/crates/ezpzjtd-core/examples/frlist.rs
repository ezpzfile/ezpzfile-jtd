// frames with their raw words 6 and 25: frlist in.jtd
fn main() {
    let p = std::env::args().nth(1).unwrap();
    let c = ezpzjtd_core::cfb::Cfb::open(std::fs::read(&p).unwrap()).unwrap();
    for f in ezpzjtd_core::frames::read_frames(&c, "") {
        println!("id={:x} kind={} item={} x={} y={} w={} h={} fixed={} wrap={}", f.id, f.kind, f.item, f.x, f.y, f.w, f.h, f.fixed, f.wrap);
    }
}
