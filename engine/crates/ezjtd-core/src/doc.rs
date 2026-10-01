//! Document model assembled from the raw layers.

use crate::cfb::{Cfb, EntryKind};
use crate::error::{Error, Result};
use crate::props::{self, Summary};
use crate::ssmg;
use crate::style::{self, CharStyle, StyleMap};
use crate::text::{self, InlineKind, Token};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
    /// Other values seen in TLV tag 0x24 that are not decoded yet.
    Other,
}

#[derive(Debug, Clone, Serialize)]
pub struct Run {
    pub text: String,
    #[serde(skip_serializing_if = "CharStyle::is_plain")]
    pub style: CharStyle,
    /// Ruby (furigana) reading shown above `text`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ruby: Option<String>,
    /// From an inline segment (ruby base / 均等割付 / field).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub inline: bool,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Paragraph {
    pub align: Align,
    pub runs: Vec<Run>,
}

impl Paragraph {
    pub fn plain_text(&self) -> String {
        self.runs.iter().map(|r| r.text.as_str()).collect()
    }
    pub fn is_empty(&self) -> bool {
        self.runs.iter().all(|r| r.text.is_empty())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Cell {
    /// Left / right edge in table grid units (see spec).
    pub left: u16,
    pub right: u16,
    pub paragraphs: Vec<Paragraph>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Row {
    pub cells: Vec<Cell>,
    /// Vertical rules of this text line (TLV tag 0x8f), as `(style, distance)`.
    /// style >= 0x10 draws a line; 0x08 is a gap with no line. See spec §4.3.
    pub rules: Vec<(u16, u16)>,
}

impl Row {
    /// True when the line has at least one drawn vertical rule.
    pub fn ruled(&self) -> bool {
        self.rules.iter().any(|&(s, _)| s >= 0x10)
    }
}

/// Parse the item list of TLV tag 0x8f: `[width, 0, x0]` then items.
/// Items with style < 0x10 are 2 words `(style, dist)`; others are 4 words
/// `(style, a, b, dist)`. The final item may be cut to 2 words.
pub fn parse_rules(v: &[u16]) -> Vec<(u16, u16)> {
    let mut out = Vec::new();
    let mut i = 3usize;
    while i + 1 < v.len() {
        let st = v[i];
        if st < 0x10 || i + 3 >= v.len() {
            out.push((st, v[i + 1]));
            i += 2;
        } else {
            out.push((st, v[i + 3]));
            i += 4;
        }
    }
    out
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Table {
    pub width: u16,
    pub rows: Vec<Row>,
}

impl Table {
    /// Sorted distinct column edges across all rows (for colspan layout).
    pub fn edges(&self) -> Vec<u16> {
        let mut e: Vec<u16> = self
            .rows
            .iter()
            .flat_map(|r| r.cells.iter().flat_map(|c| [c.left, c.right]))
            .collect();
        e.sort_unstable();
        e.dedup();
        e
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Block {
    Paragraph(Paragraph),
    Table(Table),
    PageBreak,
}

#[derive(Debug, Clone, Serialize)]
pub struct Sheet {
    pub name: String,
    pub path: String,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct FontInfo {
    pub name: String,
    pub charset: u8,
}

#[derive(Debug, Clone, Serialize)]
pub struct Document {
    pub format: String,
    pub summary: Summary,
    pub fonts: Vec<FontInfo>,
    pub sheets: Vec<Sheet>,
    /// Embedded objects (`Embedding n`, `OleItem n`, figures).
    pub objects: Vec<String>,
    pub warnings: Vec<String>,
}

impl Document {
    pub fn plain_text(&self) -> String {
        let mut out = String::new();
        let multi = self.sheets.len() > 1;
        for s in &self.sheets {
            if multi {
                out.push_str(&format!("=== {} ===\n", s.name));
            }
            for b in &s.blocks {
                match b {
                    Block::Paragraph(p) => {
                        out.push_str(&p.plain_text());
                        out.push('\n');
                    }
                    Block::Table(t) => {
                        for r in &t.rows {
                            let cells: Vec<String> = r
                                .cells
                                .iter()
                                .map(|c| {
                                    c.paragraphs
                                        .iter()
                                        .map(|p| p.plain_text())
                                        .collect::<Vec<_>>()
                                        .join(" / ")
                                })
                                .collect();
                            out.push_str(&cells.join("\t"));
                            out.push('\n');
                        }
                    }
                    Block::PageBreak => out.push('\u{c}'),
                }
            }
        }
        out
    }
}

/// Open any supported Ichitaro file.
pub fn open(bytes: Vec<u8>) -> Result<Document> {
    if bytes.starts_with(b"{\\rtf") {
        return Err(Error::NotJtd(
            "this file is RTF (rich text) with a .jtd name — open it in any word processor".into(),
        ));
    }
    let cfb = Cfb::open(bytes)?;
    let mut warnings = cfb.warnings.clone();
    let summary = cfb
        .read("/\u{5}SummaryInformation")
        .map(|b| props::parse_summary(&b))
        .unwrap_or_default();
    let fonts = cfb
        .read("/Font")
        .map(|b| parse_fonts(&b))
        .unwrap_or_default();

    let mut sheets = Vec::new();
    let format;
    if cfb.find("/DocumentText").is_some() {
        format = "jtd".to_string();
        sheets.push(read_sheet(&cfb, "", "Sheet 1", &mut warnings)?);
        // Multi-sheet documents keep further sheets in /ObjectSheets/DocSheet/DOCS_xxxx.
        let names = cfb
            .read("/DocItemInfo")
            .map(|b| sheet_names(&b))
            .unwrap_or_default();
        let mut extra: Vec<String> = cfb
            .entries()
            .iter()
            .filter(|e| {
                e.kind == EntryKind::Storage && e.path.starts_with("/ObjectSheets/DocSheet/")
            })
            .filter(|e| cfb.find(&format!("{}/DocumentText", e.path)).is_some())
            .map(|e| e.path.clone())
            .collect();
        extra.sort();
        for (k, p) in extra.iter().enumerate() {
            let name = names
                .get(k + 1)
                .cloned()
                .unwrap_or_else(|| format!("Sheet {}", k + 2));
            match read_sheet(&cfb, p, &name, &mut warnings) {
                Ok(s) => sheets.push(s),
                Err(e) => warnings.push(format!("{p}: {e}")),
            }
        }
        if let Some(n) = names.first() {
            if sheets.len() > 1 {
                sheets[0].name = n.clone();
            }
        }
    } else if cfb.find("/JSCompDocument").is_some() {
        return Err(Error::Unsupported(
            "compressed Ichitaro document (.jttc / -lh5-) — not yet supported".into(),
        ));
    } else {
        return Err(Error::NotJtd("no /DocumentText stream".into()));
    }

    let objects = cfb
        .entries()
        .iter()
        .filter(|e| e.kind == EntryKind::Storage)
        .filter(|e| {
            let n = e.name.to_lowercase();
            n.starts_with("embedding") || n.starts_with("oleitem") || n == "figuredata"
        })
        .map(|e| e.display_path())
        .collect();

    Ok(Document {
        format,
        summary,
        fonts,
        sheets,
        objects,
        warnings,
    })
}

fn read_sheet(cfb: &Cfb, base: &str, name: &str, warnings: &mut Vec<String>) -> Result<Sheet> {
    let path = format!("{base}/DocumentText");
    let raw = cfb
        .read(&path)
        .ok_or_else(|| Error::NotJtd(format!("missing {path}")))?;
    let blocks = blocks_from_document_text(&raw, warnings)?;
    Ok(Sheet {
        name: name.to_string(),
        path: if base.is_empty() {
            "/".into()
        } else {
            base.into()
        },
        blocks,
    })
}

/// Build blocks from the raw bytes of one `/DocumentText` stream.
pub fn blocks_from_document_text(raw: &[u8], warnings: &mut Vec<String>) -> Result<Vec<Block>> {
    let pieces = ssmg::pieces(raw)?;
    let mut units = Vec::new();
    let mut spans = Vec::new();
    for p in pieces {
        let off = units.len();
        for mut s in style::spans(&p.style, p.units.len()) {
            s.start += off;
            spans.push(s);
        }
        units.extend_from_slice(&p.units);
    }
    let map = StyleMap::new(spans);
    let tokens = text::tokenize(&units);
    Ok(build_blocks(&tokens, &map, warnings))
}

struct Builder<'a> {
    map: &'a StyleMap,
    blocks: Vec<Block>,
    para: Paragraph,
    align: Align,
    table: Option<Table>,
    row: Option<Row>,
}

impl<'a> Builder<'a> {
    fn style_at(&self, unit: usize) -> CharStyle {
        self.map
            .at(unit)
            .map(CharStyle::from_raw)
            .unwrap_or_default()
    }

    fn push_text(&mut self, start: usize, s: &str, inline: bool) {
        for (k, ch) in s.encode_utf16().enumerate() {
            let st = self.style_at(start + k);
            let c = String::from_utf16_lossy(&[ch]);
            match self.para.runs.last_mut() {
                Some(r)
                    if r.style.same_look(&st)
                        && r.ruby.is_none()
                        && r.inline == inline
                        && !inline =>
                {
                    r.text.push_str(&c)
                }
                Some(r) if inline && r.inline && r.ruby.is_none() && k > 0 => r.text.push_str(&c),
                _ => self.para.runs.push(Run {
                    text: c,
                    style: st,
                    ruby: None,
                    inline,
                }),
            }
        }
    }

    fn flush_para(&mut self, force: bool) {
        if !force && self.para.runs.is_empty() {
            return;
        }
        let mut p = std::mem::take(&mut self.para);
        p.align = self.align;
        if let Some(row) = self.row.as_mut() {
            if let Some(cell) = row.cells.last_mut() {
                cell.paragraphs.push(p);
                return;
            }
        }
        if self.table.is_some() {
            self.close_table();
        }
        self.blocks.push(Block::Paragraph(p));
    }

    fn close_row(&mut self) {
        self.flush_para(false);
        if let Some(r) = self.row.take() {
            if !r.cells.is_empty() {
                self.table.get_or_insert_with(Table::default).rows.push(r);
            }
        }
    }

    fn close_table(&mut self) {
        if let Some(r) = self.row.take() {
            if !r.cells.is_empty() {
                self.table.get_or_insert_with(Table::default).rows.push(r);
            }
        }
        if let Some(t) = self.table.take() {
            push_table(&mut self.blocks, t);
        }
    }
}

/// Single-column "tables" are how Ichitaro stores ordinary lines inside a
/// ruled (罫線) area. Unwrap them into paragraphs.
fn push_table(blocks: &mut Vec<Block>, t: Table) {
    if t.rows.is_empty() {
        return;
    }
    if t.rows.iter().all(|r| r.cells.len() <= 1) {
        for r in t.rows {
            for c in r.cells {
                for p in c.paragraphs {
                    blocks.push(Block::Paragraph(p));
                }
            }
        }
        return;
    }
    blocks.push(Block::Table(t));
}

fn build_blocks(tokens: &[Token], map: &StyleMap, _warnings: &mut Vec<String>) -> Vec<Block> {
    let mut b = Builder {
        map,
        blocks: Vec::new(),
        para: Paragraph::default(),
        align: Align::Left,
        table: None,
        row: None,
    };
    for t in tokens {
        match t {
            Token::Record {
                class: 0x0010,
                payload,
                ..
            } => {
                let tlv = text::para_tlv(payload);
                let mut align = Align::Left;
                let mut row_spec: Option<u16> = None;
                let mut rules = Vec::new();
                for (tag, v) in &tlv {
                    match *tag {
                        0x24 => {
                            align = match v.first().copied().unwrap_or(0) {
                                0 => Align::Left,
                                1 => Align::Center,
                                2 => Align::Right,
                                _ => Align::Other,
                            }
                        }
                        0x8f => {
                            row_spec = v.first().copied();
                            rules = parse_rules(v);
                        }
                        _ => {}
                    }
                }
                b.flush_para(false);
                if let Some(width) = row_spec {
                    b.close_row();
                    let t = b.table.get_or_insert_with(Table::default);
                    t.width = t.width.max(width);
                    b.row = Some(Row {
                        cells: Vec::new(),
                        rules,
                    });
                } else if b.row.is_none() && b.table.is_some() {
                    b.close_table();
                }
                b.align = align;
            }
            Token::Record {
                class: 0x0030,
                payload,
                ..
            } => {
                b.flush_para(false);
                let left = payload.get(1).copied().unwrap_or(0);
                let right = payload.get(2).copied().unwrap_or(0);
                b.row.get_or_insert_with(Row::default).cells.push(Cell {
                    left,
                    right,
                    paragraphs: Vec::new(),
                });
            }
            Token::Record { class: 0x0020, .. } => {
                b.close_row();
                b.close_table();
            }
            Token::Record { .. } => {}
            Token::Text { start, text } => b.push_text(*start, text, false),
            Token::Inline {
                text_start,
                header,
                text,
                ..
            } => match text::inline_kind(header) {
                InlineKind::Visible => {
                    // keep inline text as its own run so ruby can attach
                    let st = b.style_at(*text_start);
                    b.para.runs.push(Run {
                        text: text.clone(),
                        style: st,
                        ruby: None,
                        inline: true,
                    });
                }
                InlineKind::RubyReading => {
                    if let Some(r) = b.para.runs.last_mut() {
                        r.ruby = Some(text.clone());
                    }
                }
                InlineKind::Hidden => {}
            },
            Token::Control { code, .. } => match *code {
                text::PARA_END => {
                    let in_cell = b.row.as_ref().map(|r| !r.cells.is_empty()).unwrap_or(false);
                    b.flush_para(!in_cell);
                }
                text::ROW_END => b.close_row(),
                text::PAGE_BREAK => {
                    b.flush_para(false);
                    if b.row.is_none() {
                        b.close_table();
                        b.blocks.push(Block::PageBreak);
                    }
                }
                0x0000 => {
                    b.close_row();
                    b.close_table();
                }
                _ => {}
            },
        }
    }
    b.flush_para(false);
    b.close_row();
    b.close_table();
    // drop trailing empty paragraphs / page breaks
    while matches!(b.blocks.last(), Some(Block::Paragraph(p)) if p.is_empty())
        || matches!(b.blocks.last(), Some(Block::PageBreak))
    {
        b.blocks.pop();
    }
    b.blocks
}

/// `/Font`: `FontV.01`, u16 count, then per font: u16 index, a LOGFONT-like
/// fixed part (28 bytes, charset at +23), the face name (UTF-16BE, NUL),
/// 12 bytes, and a style name (UTF-16BE, NUL, usually 標準).
pub fn parse_fonts(b: &[u8]) -> Vec<FontInfo> {
    let mut out = Vec::new();
    if b.len() < 10 || &b[..8] != b"FontV.01" {
        return out;
    }
    let count = u16::from_be_bytes([b[8], b[9]]) as usize;
    let mut o = 10usize;
    let read_z = |o: &mut usize| -> String {
        let mut u = Vec::new();
        while *o + 1 < b.len() {
            let w = u16::from_be_bytes([b[*o], b[*o + 1]]);
            *o += 2;
            if w == 0 {
                break;
            }
            u.push(w);
        }
        String::from_utf16_lossy(&u)
    };
    for _ in 0..count.min(256) {
        if o + 30 > b.len() {
            break;
        }
        let charset = b[o + 2 + 23];
        o += 2 + 28;
        let name = read_z(&mut o);
        o += 12;
        let _style = read_z(&mut o);
        out.push(FontInfo { name, charset });
    }
    out
}

/// Sheet names from `/DocItemInfo` (UTF-16LE strings; first one per item).
fn sheet_names(b: &[u8]) -> Vec<String> {
    // The record layout is not fully decoded; collect printable UTF-16LE
    // strings that are not storage names or paths.
    let mut out = Vec::new();
    let u: Vec<u16> = b
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    let mut cur = Vec::new();
    for w in u.iter().copied().chain(std::iter::once(0)) {
        if w >= 0x20 && w != 0xffff {
            cur.push(w);
        } else {
            if !cur.is_empty() {
                let s = String::from_utf16_lossy(&cur);
                let looks_name = !s.starts_with("DOCS_")
                    && !s.contains('\\')
                    && !s.contains(':')
                    && s.chars().any(|c| !c.is_ascii_control());
                if looks_name && s.chars().count() < 64 {
                    out.push(s);
                }
            }
            cur.clear();
        }
    }
    out
}
