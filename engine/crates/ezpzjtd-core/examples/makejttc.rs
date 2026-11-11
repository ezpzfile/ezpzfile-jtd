//! Write a compressed Ichitaro file (.jttc) built from a .jtd, for browser tests:
//!   cargo run --example makejttc -- in.jtd out.jttc
use ezpzjtd_core::cfb::Cfb;
use ezpzjtd_core::cfbw::{write, Meta, Node, Tree};
struct W {
    out: Vec<u8>,
    acc: u32,
    n: u32,
}
impl W {
    fn put(&mut self, v: u32, bits: u32) {
        for k in (0..bits).rev() {
            self.acc = (self.acc << 1) | ((v >> k) & 1);
            self.n += 1;
            if self.n == 8 {
                self.out.push(self.acc as u8);
                self.acc = 0;
                self.n = 0;
            }
        }
    }
    fn finish(mut self) -> Vec<u8> {
        while self.n != 0 {
            self.put(0, 1);
        }
        self.out
    }
}

/// `-lh5-` data with literals only: every byte is an 8-bit code.
fn lh5_literals(data: &[u8]) -> Vec<u8> {
    let mut w = W { out: Vec::new(), acc: 0, n: 0 };
    for chunk in data.chunks(65535) {
        w.put(chunk.len() as u32, 16);
        // code-length code: symbols 2 and 10, one bit each
        w.put(11, 5);
        w.put(0, 3);
        w.put(0, 3);
        w.put(1, 3); // symbol 2: length 1
        w.put(0, 2); // no zero run after the third length
        for _ in 3..10 {
            w.put(0, 3);
        }
        w.put(1, 3); // symbol 10: length 1
        // literal/length code: 256 literals of length 8, then 254 unused
        w.put(510, 9);
        for _ in 0..256 {
            w.put(1, 1); // symbol 10 → length 8
        }
        w.put(0, 1); // symbol 2 → a run of 20 + 9 bits zeros
        w.put(254 - 20, 9);
        // position code: one symbol, no bits
        w.put(0, 4);
        w.put(0, 4);
        for &b in chunk {
            w.put(b as u32, 8);
        }
    }
    w.finish()
}

/// A level-0 LHA member header in front of `packed`.
fn member(method: &[u8; 5], packed: &[u8], original: usize) -> Vec<u8> {
    let name = b"doc.jtd";
    let mut h = vec![0u8, 0];
    h.extend_from_slice(method);
    h.extend_from_slice(&(packed.len() as u32).to_le_bytes());
    h.extend_from_slice(&(original as u32).to_le_bytes());
    h.extend_from_slice(&[0, 0, 0, 0, 0x20, 0]); // time, attribute, level 0
    h.push(name.len() as u8);
    h.extend_from_slice(name);
    h.extend_from_slice(&[0, 0]); // crc (not checked)
    h[0] = (h.len() - 2) as u8;
    h.extend_from_slice(packed);
    h
}

fn jttc(stream: Vec<u8>) -> Vec<u8> {
    let c = Cfb::open(BLANK.with(|b| b.borrow().clone())).unwrap();
    let mut t = Tree::from_cfb(&c);
    assert!(t.remove("/DocumentText"));
    t.children.push(Node::Stream { name: "JSCompDocument".into(), meta: Meta::default(), data: stream });
    write(&t)
}


thread_local!(static BLANK: std::cell::RefCell<Vec<u8>> = std::cell::RefCell::new(Vec::new()));
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let src = std::fs::read(&a[1]).unwrap();
    BLANK.with(|b| *b.borrow_mut() = src.clone());
    let file = jttc(member(b"-lh5-", &lh5_literals(&src), src.len()));
    std::fs::write(&a[2], file).unwrap();
}
