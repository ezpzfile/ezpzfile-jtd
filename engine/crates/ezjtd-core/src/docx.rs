//! Word (.docx) writer, with a tiny dependency-free ZIP writer.
//!
//! Keeps paragraphs, alignment, page breaks, bold / italic / underline /
//! size / colour, ruby, and ruled tables (merged cells, column widths).
//! Document properties of the original file (author, original path) are
//! deliberately not copied.

use crate::doc::{Align, Block, Document, Paragraph, Run, Table};
use crate::layout::{snapped_row, PageSetup};

// ------------------------------------------------------------------ zip

fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (i, t) in table.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
        *t = c;
    }
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc = table[((crc ^ b as u32) & 0xff) as usize] ^ (crc >> 8);
    }
    crc ^ 0xFFFF_FFFF
}

/// Minimal ZIP (stored, no compression). Enough for Office documents.
pub fn zip(files: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    let (time, date) = (0u16, (2026 - 1980) << 9 | 1 << 5 | 1);
    for (name, data) in files {
        let crc = crc32(data);
        let off = out.len() as u32;
        let n = name.as_bytes();
        let mut h = Vec::new();
        h.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        h.extend_from_slice(&20u16.to_le_bytes());
        h.extend_from_slice(&0x0800u16.to_le_bytes()); // UTF-8 names
        h.extend_from_slice(&0u16.to_le_bytes()); // stored
        h.extend_from_slice(&time.to_le_bytes());
        h.extend_from_slice(&(date as u16).to_le_bytes());
        h.extend_from_slice(&crc.to_le_bytes());
        h.extend_from_slice(&(data.len() as u32).to_le_bytes());
        h.extend_from_slice(&(data.len() as u32).to_le_bytes());
        h.extend_from_slice(&(n.len() as u16).to_le_bytes());
        h.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&h);
        out.extend_from_slice(n);
        out.extend_from_slice(data);

        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&h[4..30]);
        central.extend_from_slice(&0u16.to_le_bytes()); // comment
        central.extend_from_slice(&0u16.to_le_bytes()); // disk
        central.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
        central.extend_from_slice(&0u32.to_le_bytes()); // external attrs
        central.extend_from_slice(&off.to_le_bytes());
        central.extend_from_slice(n);
    }
    let cd_off = out.len() as u32;
    let cd_len = central.len() as u32;
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&cd_len.to_le_bytes());
    out.extend_from_slice(&cd_off.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

// ------------------------------------------------------------------ xml

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn rpr(r: &Run, base: f32) -> String {
    let s = &r.style;
    let mut x = String::new();
    if s.bold == Some(true) {
        x.push_str("<w:b/><w:bCs/>");
    }
    if s.italic == Some(true) {
        x.push_str("<w:i/><w:iCs/>");
    }
    if let Some(c) = &s.color {
        x.push_str(&format!(
            "<w:color w:val=\"{}\"/>",
            c.trim_start_matches('#').to_uppercase()
        ));
    }
    let size = s.size_pt.unwrap_or(base);
    if (size - base).abs() > 0.01 {
        let hp = (size * 2.0).round() as u32;
        x.push_str(&format!("<w:sz w:val=\"{hp}\"/><w:szCs w:val=\"{hp}\"/>"));
    }
    match s.underline {
        Some(2) => x.push_str("<w:u w:val=\"thick\"/>"),
        Some(_) => x.push_str("<w:u w:val=\"single\"/>"),
        None => {}
    }
    if x.is_empty() {
        x
    } else {
        format!("<w:rPr>{x}</w:rPr>")
    }
}

fn text_runs(text: &str, props: &str) -> String {
    let mut out = String::new();
    for (k, part) in text.split('\t').enumerate() {
        if k > 0 {
            out.push_str(&format!("<w:r>{props}<w:tab/></w:r>"));
        }
        if !part.is_empty() {
            out.push_str(&format!(
                "<w:r>{props}<w:t xml:space=\"preserve\">{}</w:t></w:r>",
                esc(part)
            ));
        }
    }
    out
}

fn para_xml(p: &Paragraph, base: f32) -> String {
    let mut ppr = String::new();
    if p.page_break_before {
        ppr.push_str("<w:pageBreakBefore/>");
    }
    match p.align {
        Align::Center => ppr.push_str("<w:jc w:val=\"center\"/>"),
        Align::Right => ppr.push_str("<w:jc w:val=\"right\"/>"),
        _ => {}
    }
    let mut x = String::from("<w:p>");
    if !ppr.is_empty() {
        x.push_str(&format!("<w:pPr>{ppr}</w:pPr>"));
    }
    for r in &p.runs {
        let props = rpr(r, base);
        match &r.ruby {
            Some(rt) => {
                let size = r.style.size_pt.unwrap_or(base);
                let hp = (size * 2.0).round() as u32;
                let rhp = (size).round() as u32;
                x.push_str(&format!(
                    "<w:r><w:ruby><w:rubyPr><w:rubyAlign w:val=\"distributeSpace\"/><w:hps w:val=\"{rhp}\"/><w:hpsRaise w:val=\"{}\"/><w:hpsBaseText w:val=\"{hp}\"/><w:lid w:val=\"ja-JP\"/></w:rubyPr><w:rt><w:r><w:rPr><w:sz w:val=\"{rhp}\"/></w:rPr><w:t>{}</w:t></w:r></w:rt><w:rubyBase>{}</w:rubyBase></w:ruby></w:r>",
                    hp.saturating_sub(2),
                    esc(rt),
                    text_runs(&r.text, &props)
                ));
            }
            None => x.push_str(&text_runs(&r.text, &props)),
        }
    }
    x.push_str("</w:p>");
    x
}

const NO_BORDERS: &str = "<w:tcBorders><w:top w:val=\"nil\"/><w:left w:val=\"nil\"/><w:bottom w:val=\"nil\"/><w:right w:val=\"nil\"/></w:tcBorders>";

fn table_xml(t: &Table, setup: &PageSetup) -> String {
    let base = setup.font_pt;
    let rows: Vec<Vec<(u16, u16)>> = t
        .rows
        .iter()
        .map(|r| {
            snapped_row(
                &r.cells
                    .iter()
                    .map(|c| (c.left, c.right))
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    let mut edges: Vec<u16> = rows.iter().flatten().flat_map(|&(a, b)| [a, b]).collect();
    edges.push(0);
    edges.sort_unstable();
    edges.dedup();
    let width_units = *edges.last().unwrap_or(&1) as f32;
    let mut unit_tw = setup.unit() * 20.0;
    let text_tw = setup.text_w() * 20.0;
    if width_units * unit_tw > text_tw * 1.05 {
        unit_tw = text_tw / width_units;
    }
    let col = |u: u16| edges.binary_search(&u).unwrap_or_else(|i| i);
    let mut x = String::from("<w:tbl><w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/><w:tblLayout w:type=\"fixed\"/><w:tblCellMar><w:left w:w=\"40\" w:type=\"dxa\"/><w:right w:w=\"40\" w:type=\"dxa\"/></w:tblCellMar></w:tblPr><w:tblGrid>");
    for w in edges.windows(2) {
        x.push_str(&format!(
            "<w:gridCol w:w=\"{}\"/>",
            ((w[1] - w[0]) as f32 * unit_tw).round() as u32
        ));
    }
    x.push_str("</w:tblGrid>");
    let ncols = edges.len() - 1;
    let line = "w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"000000\"";
    for (r, spans) in t.rows.iter().zip(rows.iter()) {
        x.push_str("<w:tr>");
        let borders = if r.ruled() {
            format!("<w:tcBorders><w:top {line}/><w:left {line}/><w:bottom {line}/><w:right {line}/></w:tcBorders>")
        } else {
            NO_BORDERS.to_string()
        };
        let mut at = 0usize;
        let gap = |n: usize, x: &mut String, a: usize| {
            let w: f32 = (a..a + n)
                .map(|i| (edges[i + 1] - edges[i]) as f32 * unit_tw)
                .sum();
            x.push_str(&format!("<w:tc><w:tcPr><w:tcW w:w=\"{}\" w:type=\"dxa\"/><w:gridSpan w:val=\"{n}\"/>{NO_BORDERS}</w:tcPr><w:p/></w:tc>", w.round() as u32));
        };
        for (c, &(l, rr)) in r.cells.iter().zip(spans.iter()) {
            let (a, b) = (col(l), col(rr));
            if a > at {
                gap(a - at, &mut x, at);
            }
            let span = b.saturating_sub(a).max(1);
            let w: f32 = (a..a + span)
                .filter(|&i| i + 1 < edges.len())
                .map(|i| (edges[i + 1] - edges[i]) as f32 * unit_tw)
                .sum();
            x.push_str(&format!(
                "<w:tc><w:tcPr><w:tcW w:w=\"{}\" w:type=\"dxa\"/>",
                w.round() as u32
            ));
            if span > 1 {
                x.push_str(&format!("<w:gridSpan w:val=\"{span}\"/>"));
            }
            x.push_str(&borders);
            x.push_str("</w:tcPr>");
            if c.paragraphs.is_empty() {
                x.push_str("<w:p/>");
            }
            for p in &c.paragraphs {
                x.push_str(&para_xml(p, base));
            }
            x.push_str("</w:tc>");
            at = a + span;
        }
        if at < ncols {
            gap(ncols - at, &mut x, at);
        }
        x.push_str("</w:tr>");
    }
    x.push_str("</w:tbl>");
    x
}

pub fn document_xml(doc: &Document, sheet: Option<usize>, setup: &PageSetup) -> String {
    let base = setup.font_pt;
    let mut body = String::new();
    let sheets: Vec<usize> = match sheet {
        Some(s) => vec![s],
        None => (0..doc.sheets.len()).collect(),
    };
    for (k, &si) in sheets.iter().enumerate() {
        if k > 0 {
            body.push_str("<w:p><w:r><w:br w:type=\"page\"/></w:r></w:p>");
        }
        for b in &doc.sheets[si].blocks {
            match b {
                Block::Paragraph(p) => body.push_str(&para_xml(p, base)),
                Block::Table(t) => body.push_str(&table_xml(t, setup)),
            }
        }
    }
    let tw = |pt: f32| (pt * 20.0).round() as u32;
    let lr = tw(setup.left());
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><w:body>{body}\
<w:sectPr><w:pgSz w:w=\"{}\" w:h=\"{}\"/><w:pgMar w:top=\"{}\" w:right=\"{lr}\" w:bottom=\"{}\" w:left=\"{lr}\" w:header=\"851\" w:footer=\"992\" w:gutter=\"0\"/><w:docGrid w:type=\"lines\" w:linePitch=\"{}\"/></w:sectPr></w:body></w:document>",
        tw(setup.page_w()),
        tw(setup.page_h()),
        tw(setup.top()),
        tw(setup.page_h() - setup.bottom()),
        tw(setup.pitch()),
    )
}

pub fn to_docx(doc: &Document, sheet: Option<usize>, setup: &PageSetup) -> Vec<u8> {
    let hp = (setup.font_pt * 2.0).round() as u32;
    let content_types = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/><Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/><Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/><Override PartName="/docProps/app.xml" ContentType="application/vnd.openxmlformats-officedocument.extended-properties+xml"/></Types>"#;
    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties" Target="docProps/app.xml"/></Relationships>"#;
    let doc_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/></Relationships>"#;
    // Underline trailing spaces like Ichitaro does (form blanks "氏名＿＿＿").
    let settings = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:compat><w:ulTrailSpace/><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat></w:settings>"#;
    let styles = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Century" w:hAnsi="Century" w:eastAsia="ＭＳ 明朝" w:cs="Times New Roman"/><w:kern w:val="2"/><w:sz w:val="{hp}"/><w:szCs w:val="{hp}"/><w:lang w:val="en-US" w:eastAsia="ja-JP"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:widowControl w:val="0"/><w:jc w:val="both"/><w:spacing w:after="0" w:line="240" w:lineRule="auto"/></w:pPr></w:pPrDefault></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="a"><w:name w:val="Normal"/></w:style><w:style w:type="table" w:default="1" w:styleId="t"><w:name w:val="Normal Table"/><w:tblPr><w:tblCellMar><w:top w:w="0" w:type="dxa"/><w:left w:w="99" w:type="dxa"/><w:bottom w:w="0" w:type="dxa"/><w:right w:w="99" w:type="dxa"/></w:tblCellMar></w:tblPr></w:style></w:styles>"#
    );
    let title = doc.summary.title.clone().unwrap_or_default();
    let core = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"><dc:title>{}</dc:title></cp:coreProperties>"#,
        esc(&title)
    );
    let app = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties"><Application>EZPZ File JTD</Application></Properties>"#;
    zip(&[
        ("[Content_Types].xml", content_types.as_bytes().to_vec()),
        ("_rels/.rels", rels.as_bytes().to_vec()),
        ("word/_rels/document.xml.rels", doc_rels.as_bytes().to_vec()),
        (
            "word/document.xml",
            document_xml(doc, sheet, setup).into_bytes(),
        ),
        ("word/styles.xml", styles.into_bytes()),
        ("word/settings.xml", settings.as_bytes().to_vec()),
        ("docProps/core.xml", core.into_bytes()),
        ("docProps/app.xml", app.as_bytes().to_vec()),
    ])
}

#[cfg(test)]
mod tests {
    #[test]
    fn crc_known_value() {
        assert_eq!(super::crc32(b"123456789"), 0xCBF4_3926);
    }
}
