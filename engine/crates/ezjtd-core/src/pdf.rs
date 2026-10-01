//! PDF export: each page is the picture the editor draws (so it looks
//! exactly like the screen) with an invisible text layer on top, so the text
//! can still be searched, selected and copied.
//!
//! No fonts are embedded: the text layer uses render mode 3 (invisible) with
//! `Identity-H` codes and a `ToUnicode` map, which is all a viewer needs to
//! find and copy text.

use crate::layout::{Item, Page};
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// One rendered page: JPEG bytes and the pixel size.
pub struct PageImage {
    pub jpeg: Vec<u8>,
    pub px_w: u32,
    pub px_h: u32,
}

fn is_half(c: char) -> bool {
    (c as u32) < 0x2000 || ('｡'..='ﾟ').contains(&c)
}

fn pdf_text_utf16(s: &str) -> String {
    let mut h = String::from("<FEFF");
    for u in s.encode_utf16() {
        let _ = write!(h, "{u:04X}");
    }
    h.push('>');
    h
}

fn num(v: f32) -> String {
    let s = format!("{:.2}", v);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Build a PDF from laid-out pages and their rendered images.
pub fn to_pdf(pages: &[Page], images: &[PageImage], title: &str) -> Vec<u8> {
    // character → code (1-based, 2 bytes)
    let mut codes: BTreeMap<char, u16> = BTreeMap::new();
    for p in pages {
        for it in &p.items {
            if let Item::Text { s, .. } = it {
                for c in s.chars() {
                    let n = codes.len() as u16 + 1;
                    if n < 0xffff {
                        codes.entry(c).or_insert(n);
                    }
                }
            }
        }
    }

    let mut objs: Vec<Vec<u8>> = Vec::new(); // index = object number - 1
    let mut add = |o: Vec<u8>| -> usize {
        objs.push(o);
        objs.len()
    };
    let n_pages = pages.len().min(images.len()).max(1);
    // fixed objects: 1 catalog, 2 pages, 3 font, 4 cidfont, 5 tounicode, 6 descriptor, 7 info
    for _ in 0..7 {
        add(Vec::new());
    }
    let mut kids = Vec::new();
    for (i, p) in pages.iter().enumerate().take(n_pages) {
        let img = images.get(i);
        let mut content = String::new();
        if let Some(img) = img {
            let _ = writeln!(content, "q {} 0 0 {} 0 0 cm /Im0 Do Q", num(p.w), num(p.h));
            let _ = img;
        }
        content.push_str("BT 3 Tr\n");
        let mut cur_size = -1.0f32;
        for it in &p.items {
            if let Item::Text { xs, y, s, size, .. } = it {
                if (*size - cur_size).abs() > 0.001 {
                    let _ = writeln!(content, "/F1 {} Tf", num(*size));
                    cur_size = *size;
                }
                let base = p.h - *y;
                for (k, c) in s.chars().enumerate() {
                    let Some(code) = codes.get(&c) else { continue };
                    let x = xs.get(k).copied().unwrap_or(0.0);
                    let _ = writeln!(
                        content,
                        "1 0 0 1 {} {} Tm <{:04X}> Tj",
                        num(x),
                        num(base),
                        code
                    );
                }
            }
        }
        content.push_str("ET\n");
        let content_obj = add(stream(String::new(), content.as_bytes()));
        let img_ref = img.map(|im| {
            add(stream(
                format!(
                    "/Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode",
                    im.px_w, im.px_h
                ),
                &im.jpeg,
            ))
        });
        let xobj = img_ref
            .map(|r| format!("/XObject << /Im0 {r} 0 R >> "))
            .unwrap_or_default();
        let page = add(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Resources << {xobj}/Font << /F1 3 0 R >> >> /Contents {content_obj} 0 R >>",
                num(p.w),
                num(p.h)
            )
            .into_bytes(),
        );
        kids.push(page);
    }
    if pages.is_empty() {
        let content_obj = add(stream(String::new(), b""));
        kids.push(add(
            format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595.28 841.89] /Contents {content_obj} 0 R >>").into_bytes(),
        ));
    }

    objs[0] = b"<< /Type /Catalog /Pages 2 0 R >>".to_vec();
    objs[1] = format!(
        "<< /Type /Pages /Kids [{}] /Count {} >>",
        kids.iter()
            .map(|k| format!("{k} 0 R"))
            .collect::<Vec<_>>()
            .join(" "),
        kids.len()
    )
    .into_bytes();
    objs[2] = b"<< /Type /Font /Subtype /Type0 /BaseFont /EZPZ-TextLayer /Encoding /Identity-H /DescendantFonts [4 0 R] /ToUnicode 5 0 R >>".to_vec();
    let mut w = String::from("[");
    for (c, code) in &codes {
        if is_half(*c) {
            let _ = write!(w, " {code} [500]");
        }
    }
    w.push_str(" ]");
    objs[3] = format!(
        "<< /Type /Font /Subtype /CIDFontType2 /BaseFont /EZPZ-TextLayer /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /FontDescriptor 6 0 R /DW 1000 /W {w} /CIDToGIDMap /Identity >>"
    )
    .into_bytes();
    let mut cmap = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /EZPZ-UCS def\n/CMapType 2 def\n1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    let pairs: Vec<(&char, &u16)> = codes.iter().collect();
    for chunk in pairs.chunks(100) {
        let _ = writeln!(cmap, "{} beginbfchar", chunk.len());
        for (c, code) in chunk {
            let mut u = String::new();
            let mut buf = [0u16; 2];
            for x in c.encode_utf16(&mut buf) {
                let _ = write!(u, "{x:04X}");
            }
            let _ = writeln!(cmap, "<{code:04X}> <{u}>");
        }
        cmap.push_str("endbfchar\n");
    }
    cmap.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    objs[4] = stream(String::new(), cmap.as_bytes());
    objs[5] = b"<< /Type /FontDescriptor /FontName /EZPZ-TextLayer /Flags 4 /FontBBox [0 -120 1000 880] /ItalicAngle 0 /Ascent 880 /Descent -120 /CapHeight 700 /StemV 80 >>".to_vec();
    objs[6] = format!(
        "<< /Title {} /Producer (EZPZ File JTD) /Creator (EZPZ File JTD) >>",
        pdf_text_utf16(title)
    )
    .into_bytes();

    let mut out: Vec<u8> = b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(o);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for off in offsets {
        out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R /Info 7 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objs.len() + 1
        )
        .as_bytes(),
    );
    out
}

fn stream(dict: String, data: &[u8]) -> Vec<u8> {
    let mut o = format!("<< {dict} /Length {} >>\nstream\n", data.len()).into_bytes();
    o.extend_from_slice(data);
    o.extend_from_slice(b"\nendstream");
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_pdf_is_well_formed() {
        let page = Page {
            w: 595.28,
            h: 841.89,
            items: vec![Item::Text {
                xs: vec![72.0, 82.5, 93.0],
                y: 100.0,
                s: "一太a".into(),
                size: 10.5,
                b: false,
                i: false,
                color: None,
                pre: false,
            }],
        };
        let pdf = to_pdf(&[page], &[], "テスト");
        let s = String::from_utf8_lossy(&pdf);
        assert!(s.starts_with("%PDF-1.4"));
        assert!(s.contains("/Count 1"));
        assert!(s.contains("beginbfchar"));
        assert!(s.trim_end().ends_with("%%EOF"));
        // startxref points at the xref table, and every offset at its object
        let xref_at: usize = s
            .rsplit("startxref\n")
            .next()
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        assert!(pdf[xref_at..].starts_with(b"xref"));
        let table = String::from_utf8_lossy(&pdf[xref_at..]).to_string();
        for (i, line) in table
            .lines()
            .skip(3)
            .take_while(|l| l.ends_with(" n "))
            .enumerate()
        {
            let off: usize = line[..10].parse().unwrap();
            assert!(pdf[off..].starts_with(format!("{} 0 obj", i + 1).as_bytes()));
        }
    }
}
