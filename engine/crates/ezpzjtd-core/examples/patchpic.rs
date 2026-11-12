// replace the picture of Embedding n: patchpic in.jtd out.jtd n pic.gif [n pic ...]
use ezpzjtd_core::cfb::Cfb;
use ezpzjtd_core::cfbw::{write, Tree};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let c = Cfb::open(std::fs::read(&a[1]).unwrap()).unwrap();
    let mut t = Tree::from_cfb(&c);
    for k in (3..a.len()).step_by(2) {
        let path = format!("/EmbedItems/Embedding {}/Contents", a[k]);
        let old = c.read(&path).unwrap();
        let (_, data) = ezpzjtd_core::frames::picture_in(&old).unwrap();
        let at = (0..old.len()).find(|&i| old[i..].starts_with(&data[..8])).unwrap();
        let new_pic = std::fs::read(&a[k + 1]).unwrap();
        let mut b = old[..at - 4].to_vec();
        b.extend_from_slice(&(new_pic.len() as u32).to_le_bytes());
        b.extend_from_slice(&new_pic);
        assert!(t.set_stream(&path, b));
    }
    t.fix_segment_info();
    std::fs::write(&a[2], write(&t)).unwrap();
}
