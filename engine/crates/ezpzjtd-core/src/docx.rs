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

fn rpr(r: &Run, setup: &PageSetup) -> String {
    let base = setup.font_pt;
    let s = &r.style;
    let mut x = String::new();
    // The run's own face (not the standard one) for all of its text, and
    // the second face (id 8) for half-width text when it has one.
    let name = |f: Option<u16>| f.and_then(|f| setup.font_names.get(f as usize)).filter(|n| !n.is_empty());
    let own = name(s.font).filter(|_| s.font != setup.base_font);
    let latin = name(s.font_latin).or(own);
    if own.is_some() || name(s.font_latin).is_some() {
        x.push_str("<w:rFonts");
        if let Some(l) = latin {
            x.push_str(&format!(" w:ascii=\"{0}\" w:hAnsi=\"{0}\"", esc(l)));
        }
        if let Some(e) = own {
            x.push_str(&format!(" w:eastAsia=\"{}\"", esc(e)));
        }
        x.push_str("/>");
    }
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

/// Pictures written so far: the frame's picture goes to `word/media`.
struct Cx<'a> {
    doc: &'a Document,
    used: Vec<(std::borrow::Cow<'a, [u8]>, &'static str)>,
}

fn ext_of(mime: &str) -> &'static str {
    match mime {
        "image/png" => "png",
        "image/jpeg" => "jpeg",
        "image/gif" => "gif",
        "image/bmp" => "bmp",
        "image/svg+xml" => "svg",
        _ => "",
    }
}

/// A picture anchored to its paragraph where Ichitaro puts it: `x` from the
/// left of the text area, `y` from the top of the anchor's line, behind the
/// text the way the editor draws it.
fn picture_xml(id: u16, cx: &mut Cx, in_cell: bool) -> String {
    let Some(f) = cx.doc.frames.iter().find(|f| f.id == id && (f.kind == 1 || f.kind == 4)) else {
        return String::new();
    };
    let Some(pic) = cx.doc.pictures.iter().find(|p| p.item == id) else {
        return String::new();
    };
    let ext = ext_of(pic.mime);
    if ext.is_empty() || pic.data.is_empty() {
        return String::new();
    }
    cx.used.push((std::borrow::Cow::Borrowed(&pic.data[..]), ext));
    let n = cx.used.len();
    // a figure is SVG: Word takes it with a PNG beside it for older readers
    // (a clear pixel here), LibreOffice and Word 2016 and later draw the SVG
    let blip = if ext == "svg" {
        cx.used.push((std::borrow::Cow::Owned(crate::figure::png_rgba(1, 1, &[0, 0, 0, 0])), "png"));
        format!("<a:blip r:embed=\"rIdImg{}\"><a:extLst><a:ext uri=\"{{96DAC541-7B7A-43D3-8B79-37D633B846F1}}\"><asvg:svgBlip xmlns:asvg=\"http://schemas.microsoft.com/office/drawing/2016/SVG/main\" r:embed=\"rIdImg{n}\"/></a:ext></a:extLst></a:blip>", n + 1)
    } else {
        format!("<a:blip r:embed=\"rIdImg{n}\"/>")
    };
    let emu = |v: i64| v * 360; // 1/100 mm
    let (x, y, w, h) = (emu(f.x as i64), emu(f.y as i64), emu(f.w as i64).max(1), emu(f.h as i64).max(1));
    // a frame on the page is measured from the paper's edges, the rest from the anchor's line
    let (hrel, vrel) = if f.fixed { ("page", "page") } else { ("column", if in_cell { "paragraph" } else { "line" }) };
    let lic = if in_cell { "0" } else { "1" };
    format!(
        "<w:r><w:drawing><wp:anchor distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\" simplePos=\"0\" relativeHeight=\"{n}\" behindDoc=\"1\" locked=\"0\" layoutInCell=\"{lic}\" allowOverlap=\"1\">\
<wp:simplePos x=\"0\" y=\"0\"/><wp:positionH relativeFrom=\"{hrel}\"><wp:posOffset>{x}</wp:posOffset></wp:positionH><wp:positionV relativeFrom=\"{vrel}\"><wp:posOffset>{y}</wp:posOffset></wp:positionV>\
<wp:extent cx=\"{w}\" cy=\"{h}\"/><wp:effectExtent l=\"0\" t=\"0\" r=\"0\" b=\"0\"/><wp:wrapNone/><wp:docPr id=\"{n}\" name=\"Picture {n}\"/>\
<wp:cNvGraphicFramePr><a:graphicFrameLocks xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" noChangeAspect=\"1\"/></wp:cNvGraphicFramePr>\
<a:graphic xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><pic:pic xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">\
<pic:nvPicPr><pic:cNvPr id=\"{n}\" name=\"image{n}.{ext}\"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill>{blip}<a:stretch><a:fillRect/></a:stretch></pic:blipFill>\
<pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{w}\" cy=\"{h}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"
    )
}

fn para_xml(p: &Paragraph, setup: &PageSetup, cx: &mut Cx) -> String {
    let base = setup.font_pt;
    let tw = |pt: f32| (pt * 20.0).round() as i32;
    let mut ppr = String::new();
    if p.page_break_before {
        ppr.push_str("<w:pageBreakBefore/>");
    }
    // 改行幅: an exact line height in Word
    if let Some(f) = p.feed.and_then(|f| f.points(setup.pitch())) {
        ppr.push_str(&format!(
            "<w:spacing w:line=\"{}\" w:lineRule=\"exact\"/>",
            tw(f.max(1.0))
        ));
    }
    if let Some(i) = p.indent {
        let [l, r, fl, _] = i.points(setup.cell());
        let first = fl - l;
        let fl_attr = if first >= 0.0 {
            format!("w:firstLine=\"{}\"", tw(first))
        } else {
            format!("w:hanging=\"{}\"", tw(-first))
        };
        ppr.push_str(&format!(
            "<w:ind w:left=\"{}\" w:right=\"{}\" {fl_attr}/>",
            tw(l),
            tw(r)
        ));
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
    for &id in &p.frames {
        x.push_str(&picture_xml(id, cx, false));
    }
    for r in &p.runs {
        let props = rpr(r, setup);
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

fn table_xml(t: &Table, setup: &PageSetup, cx: &mut Cx) -> String {
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
        // pictures anchored on this line go into its first cell, placed on the page
        let mut row_pics = String::new();
        for &id in &r.frames {
            row_pics.push_str(&picture_xml(id, cx, true));
        }
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
                x.push_str(&format!("<w:p>{}</w:p>", std::mem::take(&mut row_pics)));
            }
            for p in &c.paragraphs {
                let mut px = para_xml(p, setup, cx);
                if !row_pics.is_empty() {
                    let at = px.find("</w:pPr>").map_or(5, |i| i + 8);
                    px.insert_str(at, &std::mem::take(&mut row_pics));
                }
                x.push_str(&px);
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
    let mut cx = Cx { doc, used: Vec::new() };
    document_with(doc, sheet, setup, &mut cx)
}

fn document_with<'a>(doc: &'a Document, sheet: Option<usize>, setup: &PageSetup, cx: &mut Cx<'a>) -> String {
    let own;
    let setup = if setup.font_names.len() != doc.fonts.len() {
        let mut s = setup.clone();
        s.fill_fonts(&doc.fonts);
        own = s;
        &own
    } else {
        setup
    };
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
                Block::Paragraph(p) => body.push_str(&para_xml(p, setup, cx)),
                Block::Table(t) => body.push_str(&table_xml(t, setup, cx)),
            }
        }
    }
    let tw = |pt: f32| (pt * 20.0).round() as u32;
    let (l, r) = (
        tw(setup.left()),
        tw(setup.page_w() - setup.left() - setup.text_w()),
    );
    let (top, bottom) = (tw(setup.top()), tw(setup.page_h() - setup.bottom()));
    // 縦組み: the layout page is turned a quarter turn; Word gets the paper as it is
    // and writes the text top to bottom, right to left
    let (pw, ph, mt, mr, mb, ml, dir) = if setup.vertical {
        (tw(setup.page_h()), tw(setup.page_w()), l, top, r, bottom, "<w:textDirection w:val=\"tbRl\"/>")
    } else {
        (tw(setup.page_w()), tw(setup.page_h()), top, r, bottom, l, "")
    };
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><w:body>{body}\
<w:sectPr><w:pgSz w:w=\"{pw}\" w:h=\"{ph}\"/><w:pgMar w:top=\"{mt}\" w:right=\"{mr}\" w:bottom=\"{mb}\" w:left=\"{ml}\" w:header=\"851\" w:footer=\"992\" w:gutter=\"0\"/>{dir}<w:docGrid w:type=\"lines\" w:linePitch=\"{}\"/></w:sectPr></w:body></w:document>",
        tw(setup.pitch()),
    )
}

pub fn to_docx(doc: &Document, sheet: Option<usize>, setup: &PageSetup) -> Vec<u8> {
    let hp = (setup.font_pt * 2.0).round() as u32;
    let mut cx = Cx { doc, used: Vec::new() };
    let document = document_with(doc, sheet, setup, &mut cx);
    let mut exts: Vec<&str> = cx.used.iter().map(|u| u.1).collect();
    exts.sort();
    exts.dedup();
    let defaults: String = exts.iter().map(|e| format!("<Default Extension=\"{e}\" ContentType=\"image/{}\"/>", if *e == "svg" { "svg+xml" } else { e })).collect();
    let pic_rels: String = (1..=cx.used.len())
        .map(|n| format!("<Relationship Id=\"rIdImg{n}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"media/image{n}.{}\"/>", cx.used[n - 1].1))
        .collect();
    let base = setup.base_font.and_then(|i| doc.fonts.get(i as usize)).or_else(|| crate::layout::base_font(&doc.fonts).and_then(|i| doc.fonts.get(i as usize)));
    let east = base.filter(|f| f.charset == 0x80).map_or("ＭＳ 明朝".to_string(), |f| esc(&f.name));
    let content_types = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/>{defaults}<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/><Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/><Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/><Override PartName="/docProps/app.xml" ContentType="application/vnd.openxmlformats-officedocument.extended-properties+xml"/></Types>"#);
    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties" Target="docProps/app.xml"/></Relationships>"#;
    let doc_rels = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/>{pic_rels}</Relationships>"#);
    // Underline trailing spaces like Ichitaro does (form blanks "氏名＿＿＿").
    let settings = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:compat><w:ulTrailSpace/><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat></w:settings>"#;
    let styles = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Century" w:hAnsi="Century" w:eastAsia="{east}" w:cs="Times New Roman"/><w:kern w:val="2"/><w:sz w:val="{hp}"/><w:szCs w:val="{hp}"/><w:lang w:val="en-US" w:eastAsia="ja-JP"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:widowControl w:val="0"/><w:jc w:val="both"/><w:spacing w:after="0" w:line="240" w:lineRule="auto"/></w:pPr></w:pPrDefault></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="a"><w:name w:val="Normal"/></w:style><w:style w:type="table" w:default="1" w:styleId="t"><w:name w:val="Normal Table"/><w:tblPr><w:tblCellMar><w:top w:w="0" w:type="dxa"/><w:left w:w="99" w:type="dxa"/><w:bottom w:w="0" w:type="dxa"/><w:right w:w="99" w:type="dxa"/></w:tblCellMar></w:tblPr></w:style></w:styles>"#
    );
    let title = doc.summary.title.clone().unwrap_or_default();
    let core = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"><dc:title>{}</dc:title></cp:coreProperties>"#,
        esc(&title)
    );
    let app = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties"><Application>EZPZ File JTD</Application></Properties>"#;
    let names: Vec<String> = (1..=cx.used.len()).map(|n| format!("word/media/image{n}.{}", cx.used[n - 1].1)).collect();
    let mut files: Vec<(&str, Vec<u8>)> = vec![
        ("[Content_Types].xml", content_types.into_bytes()),
        ("_rels/.rels", rels.as_bytes().to_vec()),
        ("word/_rels/document.xml.rels", doc_rels.into_bytes()),
        ("word/document.xml", document.into_bytes()),
        ("word/styles.xml", styles.into_bytes()),
        ("word/settings.xml", settings.as_bytes().to_vec()),
        ("docProps/core.xml", core.into_bytes()),
        ("docProps/app.xml", app.as_bytes().to_vec()),
    ];
    for (n, (data, _)) in names.iter().zip(&cx.used) {
        files.push((n.as_str(), data.to_vec()));
    }
    zip(&files)
}

#[cfg(test)]
mod tests {
    #[test]
    fn crc_known_value() {
        assert_eq!(super::crc32(b"123456789"), 0xCBF4_3926);
    }
}
