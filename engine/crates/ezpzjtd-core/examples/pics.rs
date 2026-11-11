use ezpzjtd_core::doc::Block;
fn main() { for p in std::env::args().skip(1) {
  let d = ezpzjtd_core::open(std::fs::read(&p).unwrap()).unwrap();
  let mut anchors = 0; for s in &d.sheets { for b in &s.blocks { match b { Block::Paragraph(q) => anchors += q.frames.len(), Block::Table(t) => for r in &t.rows { for c in &r.cells { for q in &c.paragraphs { anchors += q.frames.len() } } } } } }
  let kinds: Vec<u16> = d.frames.iter().map(|f| f.kind).collect();
  println!("{:40} frames {:3} anchors {:3} pics {:3} kinds {:?} mimes {:?}", p.rsplit('/').next().unwrap(), d.frames.len(), anchors, d.pictures.len(), { let mut k = kinds.clone(); k.sort(); k.dedup(); k }, d.pictures.iter().map(|x| x.mime).collect::<std::collections::BTreeSet<_>>());
} }
