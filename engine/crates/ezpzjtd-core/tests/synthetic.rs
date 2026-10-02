//! Tests on hand-built `/DocumentText` streams, so they run without any
//! copyrighted sample files.

use ezpzjtd_core::doc::{blocks_from_document_text, Align, Block};
use ezpzjtd_core::{ssmg, style, text};

/// Build an `SsmgV.01` container holding one `TextV.01` piece.
fn ssmg_text(units: &[u16], style: &[u8]) -> Vec<u8> {
    let mut sub = b"TextV.01".to_vec();
    sub.extend_from_slice(&(units.len() as u32).to_be_bytes());
    for u in units {
        sub.extend_from_slice(&u.to_be_bytes());
    }
    sub.extend_from_slice(style);
    let bs = 256usize;
    let nb = sub.len().div_ceil(bs);
    let mut out = b"SsmgV.01".to_vec();
    out.extend_from_slice(&1u32.to_be_bytes());
    out.extend_from_slice(&(bs as u32).to_be_bytes());
    out.extend_from_slice(&(nb as u32).to_be_bytes());
    let mut padded = sub.clone();
    padded.resize(nb * bs, 0);
    out.extend_from_slice(&padded);
    for v in [0u32, 0, sub.len() as u32, nb as u32, nb as u32] {
        out.extend_from_slice(&v.to_be_bytes());
    }
    for k in 0..nb as u32 {
        out.extend_from_slice(&k.to_be_bytes());
    }
    out.extend_from_slice(&0u32.to_be_bytes());
    out
}

fn s(t: &str) -> Vec<u16> {
    t.encode_utf16().collect()
}

/// `001C 0010 len 0000 <tlv…> FFFF 0000 len 0000 0010 001F`
fn para_rec(tlv: &[(u16, &[u16])]) -> Vec<u16> {
    let mut body = vec![0u16];
    for (tag, v) in tlv {
        body.push(*tag);
        body.push(v.len() as u16);
        body.extend_from_slice(v);
    }
    body.extend_from_slice(&[0xffff, 0]);
    let len = (3 + body.len() + 4) as u16;
    let mut r = vec![0x1c, 0x10, len];
    r.extend(body);
    r.extend_from_slice(&[len, 0, 0x10, 0x1f]);
    r
}

fn ruby(base: &str, reading: &str) -> Vec<u16> {
    let mut r = vec![0x1c, 0x01, 0x07, 0x0000, 0x0000, 0x0003, 0x1d];
    r.extend(s(base));
    r.extend_from_slice(&[0x1e, 5, 0, 1, 0x1f]);
    r.extend_from_slice(&[0x1c, 0x01, 0x07, 0x0000, 0x0001, 0x0082, 0x1d]);
    r.extend(s(reading));
    r.extend_from_slice(&[0x1e, 5, 0, 1, 0x1f]);
    r
}

#[test]
fn container_roundtrip() {
    let units = s("abc");
    let raw = ssmg_text(&units, &[0x00, 0, 0, 0, 3, 0xff]);
    let p = ssmg::pieces(&raw).unwrap();
    assert_eq!(p.len(), 1);
    assert_eq!(p[0].units, units);
    assert_eq!(p[0].style, vec![0x00, 0, 0, 0, 3, 0xff]);
}

#[test]
fn style_events_cover_units() {
    // run 2, set size=423 (12pt) + bold on 1 unit, run 2
    let ev = [
        0x00, 0, 0, 0, 2, 0xfe, 2, 2, 0x01, 0xa7, 1, 2, 0, 1, 0xff, 0x00, 0x00, 0, 0, 0, 2, 0xff,
    ];
    let sp = style::spans(&ev, 5);
    let total: usize = sp.iter().map(|s| s.len).sum();
    assert_eq!(total, 5);
    let st = style::CharStyle::from_raw(&sp.last().unwrap().raw);
    assert_eq!(st.size_pt, Some(12.0));
    assert_eq!(st.bold, Some(true));
}

#[test]
fn tokenizer_records_and_ruby() {
    let mut u = para_rec(&[(0x24, &[1])]);
    u.extend(s("題名"));
    u.extend(ruby("切符", "きっぷ"));
    u.push(0x0a);
    let t = text::tokenize(&u);
    assert!(matches!(t[0], text::Token::Record { class: 0x10, .. }));
    assert!(t
        .iter()
        .any(|x| matches!(x, text::Token::Inline { text, .. } if text == "きっぷ")));
}

#[test]
fn paragraph_alignment_and_ruby() {
    let mut u = para_rec(&[(0x24, &[1])]);
    u.extend(s("題名"));
    u.extend(ruby("切符", "きっぷ"));
    u.push(0x0a);
    u.extend(para_rec(&[(0x24, &[2])]));
    u.extend(s("右"));
    u.push(0x0a);
    let raw = ssmg_text(&u, &[0xff]);
    let blocks = blocks_from_document_text(&raw, &mut Vec::new()).unwrap();
    let paras: Vec<_> = blocks
        .iter()
        .filter_map(|b| {
            if let Block::Paragraph(p) = b {
                Some(p)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(paras.len(), 2);
    assert_eq!(paras[0].align, Align::Center);
    assert_eq!(paras[0].plain_text(), "題名切符");
    let r = paras[0].runs.iter().find(|r| r.text == "切符").unwrap();
    assert_eq!(r.ruby.as_deref(), Some("きっぷ"));
    assert_eq!(paras[1].align, Align::Right);
}

/// A line header formats its own line only; a line with no header has the
/// defaults. One inside a line (after its text) formats the next line.
/// [一太郎 2026: formatting the middle of three paragraphs writes one header,
/// and the third paragraph shows left aligned and not indented]
#[test]
fn line_header_formats_its_own_line() {
    use ezpzjtd_core::doc::Indent;
    let mut u = para_rec(&[(0x24, &[2]), (0x26, &[0, 4, 0, 4, 0])]);
    u.extend(s("日付"));
    u.push(0x0a);
    u.extend(s("各位"));
    u.push(0x0a);
    // a header after the text of a line: for the next line
    u.extend(s("前"));
    u.extend(para_rec(&[(0x24, &[1])]));
    u.push(0x0a);
    u.extend(s("中央"));
    u.push(0x0a);
    u.extend(s("後"));
    u.push(0x0a);
    let raw = ssmg_text(&u, &[0xff]);
    let blocks = blocks_from_document_text(&raw, &mut Vec::new()).unwrap();
    let paras: Vec<_> = blocks
        .iter()
        .filter_map(|b| if let Block::Paragraph(p) = b { Some(p) } else { None })
        .collect();
    let got: Vec<_> = paras.iter().map(|p| (p.plain_text(), p.align)).collect();
    assert_eq!(
        got,
        [
            ("日付".to_string(), Align::Right),
            ("各位".to_string(), Align::Left),
            ("前".to_string(), Align::Left),
            ("中央".to_string(), Align::Center),
            ("後".to_string(), Align::Left),
        ]
    );
    assert!(matches!(paras[0].indent, Some(Indent { .. })));
    assert_eq!(paras[1].indent, None);
}

#[test]
fn table_rows_and_cells() {
    // one ruled row with two cells
    let rules: Vec<u16> = vec![0xa0, 0, 0, 0x13, 0, 0, 0x4c, 0x13, 0, 0, 0x4c, 0x13, 0];
    let cell = |l: u16, r: u16| vec![0x1c, 0x30, 12, 0, l, r, 0xff, 0, 12, 0, 0x30, 0x1f];
    let mut u = para_rec(&[(0x8f, &rules)]);
    u.extend(cell(2, 0x4e));
    u.extend(s("左"));
    u.extend(cell(0x52, 0x9e));
    u.extend(s("右"));
    u.push(0x0e);
    u.extend(para_rec(&[(0x8f, &rules)]));
    u.extend(cell(2, 0x4e));
    u.extend(s("下"));
    u.extend(cell(0x52, 0x9e));
    u.push(0x0e);
    u.extend(para_rec(&[(0x24, &[0])]));
    u.extend(s("本文"));
    u.push(0x0a);
    let raw = ssmg_text(&u, &[0xff]);
    let blocks = blocks_from_document_text(&raw, &mut Vec::new()).unwrap();
    let Block::Table(t) = &blocks[0] else {
        panic!("expected table, got {blocks:?}")
    };
    assert_eq!(t.rows.len(), 2);
    assert_eq!(t.rows[0].cells.len(), 2);
    assert!(t.rows[0].ruled());
    assert_eq!(t.rows[0].cells[1].paragraphs[0].plain_text(), "右");
    assert!(matches!(&blocks[1], Block::Paragraph(p) if p.plain_text() == "本文"));
}

#[test]
fn rtf_is_reported() {
    let e = ezpzjtd_core::open(b"{\\rtf1\\ansi hello}".to_vec()).unwrap_err();
    assert!(e.to_string().contains("RTF"));
}

/// Runs over a local corpus when `EZPZJTD_CORPUS` points to a folder of .jtd files.
#[test]
fn local_corpus_opens() {
    let Ok(dir) = std::env::var("EZPZJTD_CORPUS") else {
        return;
    };
    let mut n = 0;
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.extension().map(|x| x == "jtd").unwrap_or(false) {
            let bytes = std::fs::read(&p).unwrap();
            if bytes.starts_with(b"{\\rtf") {
                continue;
            }
            let d = ezpzjtd_core::open(bytes).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
            assert!(
                !d.plain_text().trim().is_empty(),
                "{} has no text",
                p.display()
            );
            n += 1;
        }
    }
    eprintln!("opened {n} corpus files");
}

/// Indents (TLV 0x26) and line feed (TLV 0x20) as the latest 一太郎 writes them
/// (spec §4.1); a table row keeps the line feed of its line header.
#[test]
fn indents_and_line_feed() {
    use ezpzjtd_core::doc::{Indent, LineFeed};
    let mut u = s("一");
    u.push(0x0a);
    // left 4 half-width columns, first line 2 (a hanging indent)
    u.extend(para_rec(&[(0x26, &[0, 4, 0, 2, 0])]));
    u.extend(s("二"));
    u.push(0x0a);
    // 1/2 line feed, then 150 %
    u.extend(para_rec(&[(0x20, &[2, 0, 0, 0])]));
    u.extend(s("三"));
    u.push(0x0a);
    u.extend(para_rec(&[
        (0x20, &[9, 1500, 0, 0]),
        (0x26, &[1, 1000, 0, 1000, 0]),
    ]));
    u.extend(s("四"));
    u.push(0x0a);
    let rules: Vec<u16> = vec![0xa0, 0, 0, 0x13, 0, 0, 0x4c, 0x13, 0, 0, 0x4c, 0x13, 0];
    let cell = |l: u16, r: u16| vec![0x1c, 0x30, 12, 0, l, r, 0xff, 0, 12, 0, 0x30, 0x1f];
    u.extend(para_rec(&[(0x20, &[2, 0, 2, 0]), (0x8f, &rules)]));
    u.extend(cell(2, 0x4e));
    u.extend(s("左"));
    u.extend(cell(0x52, 0x9e));
    u.extend(s("右"));
    u.push(0x0e);
    let raw = ssmg_text(&u, &[0xff]);
    let blocks = blocks_from_document_text(&raw, &mut Vec::new()).unwrap();
    let p = |i: usize| match &blocks[i] {
        Block::Paragraph(p) => p.clone(),
        b => panic!("{b:?}"),
    };
    assert_eq!((p(0).indent, p(0).feed), (None, None));
    assert_eq!(
        p(1).indent,
        Some(Indent {
            mm: false,
            left: 4,
            right: 0,
            first_left: 2,
            first_right: 0
        })
    );
    assert_eq!(p(1).feed, None);
    assert_eq!(p(2).feed, Some(LineFeed { kind: 2, value: 0 }));
    assert_eq!(p(2).indent, None);
    assert_eq!(p(3).feed.unwrap().points(20.0), Some(30.0));
    let i = p(3).indent.unwrap();
    assert!(i.mm && (i.points(10.0)[0] - 1000.0 * 72.0 / 2540.0).abs() < 0.01);
    let Block::Table(t) = &blocks[4] else {
        panic!("expected table")
    };
    assert_eq!(t.rows[0].feed, Some(LineFeed { kind: 2, value: 0 }));
    assert_eq!(t.rows[0].cells[0].paragraphs[0].feed, None);
}

/// A `b` word with a rule class puts a vertical rule at the end of the
/// item's own line (spec §4.3): the right edge of a box whose last row
/// ends in a line under it.
#[test]
fn b_word_with_a_rule_class() {
    use ezpzjtd_core::doc::{Row, Rule};
    // corpus line: x0 0x0a, three ruled cells, then (8, 3, 0x13, 0), blank
    let r = |style, a, b, dist| Rule { style, a, b, dist };
    let row = Row {
        x0: 0x0a,
        rules: vec![
            r(0x1b, 0, 8, 0x20),
            r(0x1b, 0, 8, 0x24),
            r(0x1b, 0, 8, 0x22),
            r(8, 3, 0x13, 0),
            r(0, 0, 0, 0x2b),
        ],
        ..Row::default()
    };
    let g = row.lines();
    let xs: Vec<u16> = g.verticals.iter().map(|v| v.0).collect();
    assert_eq!(xs, vec![11, 45, 83, 123]);
    assert!(g.verticals[3].1 && g.verticals[3].2);
    // the line under runs from the first rule to the last one
    assert_eq!(g.below.first().unwrap().0, 11);
    assert_eq!(g.below.iter().map(|s| s.1).max(), Some(123));
}
