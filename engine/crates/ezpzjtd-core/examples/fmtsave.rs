//! Give paragraphs of a file indents and line feed in the model, save, and
//! write the result, to check in Ichitaro that the line headers we write
//! (TLV 0x26, 0x20) are understood.
//! usage: fmtsave <file.jtd> <out.jtd>
use ezpzjtd_core::doc::{Block, Indent, LineFeed};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let bytes = std::fs::read(&a[1]).unwrap();
    let mut doc = ezpzjtd_core::open(bytes.clone()).unwrap();
    let paras: Vec<usize> = doc.sheets[0]
        .blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| matches!(b, Block::Paragraph(p) if !p.is_empty()))
        .map(|(i, _)| i)
        .collect();
    if let Block::Paragraph(p) = &mut doc.sheets[0].blocks[paras[1]] {
        p.indent = Some(Indent {
            mm: false,
            left: 4,
            right: 0,
            first_left: 2,
            first_right: 0,
        });
    }
    if let Block::Paragraph(p) = &mut doc.sheets[0].blocks[paras[2]] {
        p.feed = Some(LineFeed { kind: 2, value: 0 });
    }
    match ezpzjtd_core::save::save(&bytes, &doc) {
        Ok(s) => {
            std::fs::write(&a[2], &s.bytes).unwrap();
            println!("saved, changed {}", s.changed);
        }
        Err(e) => println!("refused: {e}"),
    }
}
