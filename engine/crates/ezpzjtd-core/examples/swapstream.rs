//! Experiment: copy top-level streams from a donor file into a base file.
//! usage: swapstream <base.jtd> <donor.jtd> <out.jtd> <stream>... ("-name" removes it from base, "name=@file" sets it from a file)
use ezpzjtd_core::cfb::Cfb;
use ezpzjtd_core::cfbw::{self, Meta, Node, Tree};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let base = Cfb::open(std::fs::read(&a[1]).unwrap()).unwrap();
    let donor = Cfb::open(std::fs::read(&a[2]).unwrap()).unwrap();
    let mut tree = Tree::from_cfb(&base);
    for s in &a[4..] {
        if let Some(n) = s.strip_prefix('-') {
            println!("remove {n}: {}", tree.remove(&format!("/{n}")));
            continue;
        }
        let (s, data) = match s.split_once("=@") {
            Some((name, file)) => (&name.to_string(), std::fs::read(file).unwrap()),
            None => (s, donor.read(&format!("/{s}")).expect("donor has it")),
        };
        if !tree.set_stream(&format!("/{s}"), data.clone()) {
            tree.children.push(Node::Stream {
                name: s.clone(),
                meta: Meta::default(),
                data,
            });
            println!("added {s}");
        } else {
            println!("replaced {s}");
        }
    }
    tree.fix_segment_info();
    std::fs::write(&a[3], cfbw::write(&tree)).unwrap();
}
