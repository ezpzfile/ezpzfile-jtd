//! Exporters: Markdown and a self-contained HTML fragment.

use crate::doc::{Align, Block, Document, Paragraph, Run, Table};

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn run_css(r: &Run) -> String {
    let s = &r.style;
    let mut css = String::new();
    if s.bold == Some(true) {
        css.push_str("font-weight:bold;");
    }
    if s.italic == Some(true) {
        css.push_str("font-style:italic;");
    }
    if let Some(pt) = s.size_pt {
        css.push_str(&format!("font-size:{pt}pt;"));
    }
    if let Some(c) = &s.color {
        css.push_str(&format!("color:{c};"));
    }
    match s.underline {
        Some(2) => css.push_str("text-decoration:underline;text-decoration-thickness:2px;"),
        Some(_) => css.push_str("text-decoration:underline;"),
        None => {}
    }
    // Horizontal scale (半角/倍角, `scale_x`) is kept in the model but not
    // rendered yet: CSS transforms do not reflow and break line wrapping.
    css
}

fn run_html(r: &Run) -> String {
    let body = match &r.ruby {
        Some(rt) => format!("<ruby>{}<rt>{}</rt></ruby>", esc(&r.text), esc(rt)),
        None => esc(&r.text),
    };
    let css = run_css(r);
    if css.is_empty() {
        body
    } else {
        format!("<span style=\"{css}\">{body}</span>")
    }
}

fn para_html(p: &Paragraph) -> String {
    let align = match p.align {
        Align::Center => " style=\"text-align:center\"",
        Align::Right => " style=\"text-align:right\"",
        _ => "",
    };
    let inner: String = p.runs.iter().map(run_html).collect();
    if inner.trim().is_empty() {
        format!("<p{align}>&nbsp;</p>")
    } else {
        format!("<p{align}>{inner}</p>")
    }
}

/// Cell spans snapped so that each cell reaches the next cell's left edge.
/// (Ichitaro leaves a few grid units between cells for the rule itself.)
fn snapped(t: &Table) -> Vec<Vec<(u16, u16)>> {
    t.rows
        .iter()
        .map(|r| {
            let n = r.cells.len();
            (0..n)
                .map(|i| {
                    let l = r.cells[i].left;
                    let rr = if i + 1 < n {
                        r.cells[i + 1].left
                    } else {
                        r.cells[i].right.max(l + 1)
                    };
                    (l, rr.max(l + 1))
                })
                .collect()
        })
        .collect()
}

fn table_html(t: &Table) -> String {
    let spans = snapped(t);
    let mut edges: Vec<u16> = spans.iter().flatten().flat_map(|&(a, b)| [a, b]).collect();
    edges.sort_unstable();
    edges.dedup();
    let col = |x: u16| edges.binary_search(&x).unwrap_or_else(|i| i);
    let first = *edges.first().unwrap_or(&0) as f32;
    let total = (*edges.last().unwrap_or(&1) as f32 - first).max(1.0);
    let mut h = String::from("<table class=\"jtd-table\"><colgroup>");
    for w in edges.windows(2) {
        h.push_str(&format!(
            "<col style=\"width:{:.2}%\">",
            (w[1] - w[0]) as f32 / total * 100.0
        ));
    }
    h.push_str("</colgroup>");
    let ncols = edges.len().saturating_sub(1);
    for (r, sp) in t.rows.iter().zip(spans.iter()) {
        h.push_str(if r.ruled() {
            "<tr class=\"ruled\">"
        } else {
            "<tr>"
        });
        let mut at = 0usize;
        for (c, &(l, rr)) in r.cells.iter().zip(sp.iter()) {
            let (a, b) = (col(l), col(rr));
            if a > at {
                h.push_str(&format!("<td class=\"gap\" colspan=\"{}\"></td>", a - at));
            }
            let span = b.saturating_sub(a).max(1);
            let inner: String = c.paragraphs.iter().map(para_html).collect();
            h.push_str(&format!("<td colspan=\"{span}\">{inner}</td>"));
            at = a + span;
        }
        if at < ncols {
            h.push_str(&format!(
                "<td class=\"gap\" colspan=\"{}\"></td>",
                ncols - at
            ));
        }
        h.push_str("</tr>");
    }
    h.push_str("</table>");
    h
}

/// HTML body fragment (no `<html>` wrapper).
pub fn to_html(doc: &Document) -> String {
    let mut h = String::new();
    for s in &doc.sheets {
        if doc.sheets.len() > 1 {
            h.push_str(&format!("<h2 class=\"jtd-sheet\">{}</h2>", esc(&s.name)));
        }
        h.push_str("<section class=\"jtd-page\">");
        for b in &s.blocks {
            match b {
                Block::Paragraph(p) => {
                    if p.page_break_before {
                        h.push_str("</section><section class=\"jtd-page\">");
                    }
                    h.push_str(&para_html(p))
                }
                Block::Table(t) => h.push_str(&table_html(t)),
            }
        }
        h.push_str("</section>");
    }
    h
}

pub const HTML_CSS: &str = "body{background:#eee;margin:0;padding:24px;font-family:'Hiragino Mincho ProN','Yu Mincho','MS Mincho',serif}\
.jtd-page{background:#fff;color:#111;max-width:210mm;margin:0 auto 24px;padding:20mm;box-sizing:border-box;box-shadow:0 1px 4px rgba(0,0,0,.15);font-size:10.5pt;line-height:1.6;overflow:hidden}\
@media (max-width:600px){.jtd-page{padding:16px;font-size:10pt}}\
@media print{body{background:#fff;padding:0}.jtd-page{box-shadow:none;margin:0;max-width:none;padding:0;page-break-after:always}}\
.jtd-page p{margin:0;white-space:pre-wrap;overflow-wrap:anywhere}\
.jtd-table{border-collapse:collapse;width:100%;table-layout:fixed;margin:0}\
.jtd-table td{padding:1px 4px;vertical-align:top}\
.jtd-table tr.ruled td{border-left:1px solid #333;border-right:1px solid #333;border-top:1px solid #d0d0d0;border-bottom:1px solid #d0d0d0}\
.jtd-table tr.ruled td.gap{border:none}\
rt{font-size:.5em}";

pub fn to_html_page(doc: &Document) -> String {
    let title = doc
        .summary
        .title
        .clone()
        .unwrap_or_else(|| "Ichitaro document".into());
    format!(
        "<!doctype html><html lang=\"ja\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{}</title><style>{}</style></head><body>{}</body></html>",
        esc(&title),
        HTML_CSS,
        to_html(doc)
    )
}

fn para_md(p: &Paragraph) -> String {
    let mut s = String::new();
    for r in &p.runs {
        let t = match &r.ruby {
            Some(rt) => format!("{}（{}）", r.text, rt),
            None => r.text.clone(),
        };
        if r.style.bold == Some(true) && !t.trim().is_empty() {
            s.push_str(&format!("**{}**", t.trim()));
        } else {
            s.push_str(&t);
        }
    }
    s
}

pub fn to_markdown(doc: &Document) -> String {
    let mut out = String::new();
    for s in &doc.sheets {
        if doc.sheets.len() > 1 {
            out.push_str(&format!("## {}\n\n", s.name));
        }
        for b in &s.blocks {
            match b {
                Block::Paragraph(p) => {
                    if p.page_break_before {
                        out.push_str("---\n\n");
                    }
                    let t = para_md(p);
                    out.push_str(t.trim_end());
                    out.push_str("\n\n");
                }
                Block::Table(t) => {
                    let n = t.rows.iter().map(|r| r.cells.len()).max().unwrap_or(1);
                    for (i, r) in t.rows.iter().enumerate() {
                        let mut cells: Vec<String> = r
                            .cells
                            .iter()
                            .map(|c| {
                                c.paragraphs
                                    .iter()
                                    .map(para_md)
                                    .collect::<Vec<_>>()
                                    .join("<br>")
                                    .replace('|', "\\|")
                            })
                            .collect();
                        cells.resize(n, String::new());
                        out.push_str(&format!("| {} |\n", cells.join(" | ")));
                        if i == 0 {
                            out.push_str(&format!("|{}\n", " --- |".repeat(n)));
                        }
                    }
                    out.push('\n');
                }
            }
        }
    }
    out
}
