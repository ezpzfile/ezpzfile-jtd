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

/// Where a model element came from in `/DocumentText`.
///
/// Only filled when a document is parsed for saving ([`blocks_tracked`]).
/// It never takes part in equality or serialization, so models with and
/// without source information compare equal.
#[derive(Debug, Clone, Default)]
pub struct Src<T>(pub Option<Box<T>>);

impl<T> PartialEq for Src<T> {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl<T> Src<T> {
    pub fn get(&self) -> Option<&T> {
        self.0.as_deref()
    }
}

/// Source of one character of a paragraph (unit indices into the text).
#[derive(Debug, Clone, Copy)]
pub struct CharSrc {
    pub unit: u32,
    pub len: u8,
    /// Where text typed just before this character goes.
    pub before: u32,
    /// Where text typed just after this character goes.
    pub after: u32,
    /// Inline group (ruby / 均等割付) this character belongs to, or `u32::MAX`.
    pub group: u32,
}

#[derive(Debug, Clone, Default)]
pub struct ParaSrc {
    /// First unit of the paragraph's region (just after the previous terminator).
    pub start: u32,
    /// Line header records (class 0x10) at the start of the region: (start, len).
    pub own_headers: Vec<(u32, u32)>,
    /// The header record in effect at the start of this paragraph.
    pub eff_header: Option<(u32, u32)>,
    /// One entry per character of [`Paragraph::plain_text`].
    pub chars: Vec<CharSrc>,
    /// Unit of the token that ended the paragraph.
    pub end: u32,
    /// `end` is a 000A that belongs to this paragraph.
    pub end_explicit: bool,
    /// Unit of the 000C before this paragraph.
    pub page_break: Option<u32>,
    /// The paragraph sits in a ruled line (table row or 罫線 box).
    pub in_row: bool,
}

#[derive(Debug, Clone, Default)]
pub struct RowSrc {
    /// The line header record (with tag 0x8f): (start, len).
    pub header: (u32, u32),
    /// The 000E that ends the line.
    pub end: Option<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct CellSrc {
    /// The cell record (class 0x30): (start, len).
    pub rec: (u32, u32),
}

#[derive(Debug, Clone, Serialize, PartialEq)]
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

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct Paragraph {
    pub align: Align,
    pub runs: Vec<Run>,
    /// Start this paragraph on a new page (改ページ).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub page_break_before: bool,
    #[serde(skip)]
    pub src: Src<ParaSrc>,
}

impl Paragraph {
    pub fn plain_text(&self) -> String {
        self.runs.iter().map(|r| r.text.as_str()).collect()
    }
    pub fn is_empty(&self) -> bool {
        self.runs.iter().all(|r| r.text.is_empty())
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Cell {
    /// Left / right edge in table grid units (see spec).
    pub left: u16,
    pub right: u16,
    pub paragraphs: Vec<Paragraph>,
    #[serde(skip)]
    pub src: Src<CellSrc>,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct Row {
    pub cells: Vec<Cell>,
    /// Vertical rules of this text line (TLV tag 0x8f), as `(style, distance)`.
    /// style >= 0x10 draws a line; 0x08 is a gap with no line. See spec §4.3.
    pub rules: Vec<(u16, u16)>,
    #[serde(skip)]
    pub src: Src<RowSrc>,
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

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
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

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Block {
    Paragraph(Paragraph),
    Table(Table),
}

#[derive(Debug, Clone, Serialize, PartialEq)]
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
    /// An empty document with one sheet and one empty paragraph.
    pub fn blank() -> Document {
        Document {
            format: "new".into(),
            summary: Summary::default(),
            fonts: Vec::new(),
            sheets: vec![Sheet {
                name: "Sheet 1".into(),
                path: "/".into(),
                blocks: vec![Block::Paragraph(Paragraph::default())],
            }],
            objects: Vec::new(),
            warnings: Vec::new(),
        }
    }

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
                        if p.page_break_before {
                            out.push('\u{c}');
                        }
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
    let (units, map) = units_and_styles(raw)?;
    Ok(build_blocks(&units, &map, warnings, false).0)
}

/// All text units of a `/DocumentText` stream (pieces joined) and their styles.
pub fn units_and_styles(raw: &[u8]) -> Result<(Vec<u16>, StyleMap)> {
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
    Ok((units, StyleMap::new(spans)))
}

/// Like [`blocks_from_document_text`] but every paragraph, row and cell
/// carries its source position ([`Src`]). Also returns the unit ranges of
/// inline groups (ruby base + reading, 均等割付), indexed by
/// [`CharSrc::group`].
pub fn blocks_tracked(units: &[u16], map: &StyleMap) -> (Vec<Block>, Vec<(u32, u32)>) {
    let mut w = Vec::new();
    build_blocks(units, map, &mut w, true)
}

/// Source tracking state of the builder.
#[derive(Default)]
struct Track {
    on: bool,
    tok_start: u32,
    tok_end: u32,
    /// The current token is a 000A.
    tok_para_end: bool,
    /// The region of the next paragraph begins after the current token.
    tok_resets_after: bool,
    region_start: u32,
    heads: Vec<(u32, u32)>,
    eff: Option<(u32, u32)>,
    /// `eff` when the current paragraph's content began.
    start_eff: Option<Option<(u32, u32)>>,
    chars: Vec<CharSrc>,
    page_break: Option<u32>,
    groups: Vec<(u32, u32)>,
}

struct Builder<'a> {
    map: &'a StyleMap,
    blocks: Vec<Block>,
    para: Paragraph,
    align: Align,
    table: Option<Table>,
    row: Option<Row>,
    page_break: bool,
    /// Alignment of the current paragraph when a line header inside it
    /// changed `align` for the paragraphs that follow.
    para_align: Option<Align>,
    t: Track,
}

impl<'a> Builder<'a> {
    fn style_at(&self, unit: usize) -> CharStyle {
        self.map
            .at(unit)
            .map(CharStyle::from_raw)
            .unwrap_or_default()
    }

    fn push_text(&mut self, start: usize, s: &str, inline: bool) {
        let mut k = 0usize;
        for ch in s.chars() {
            let st = self.style_at(start + k);
            let c = ch.to_string();
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
            if self.t.on {
                if self.t.start_eff.is_none() {
                    self.t.start_eff = Some(self.t.eff);
                }
                let unit = (start + k) as u32;
                let len = ch.len_utf16() as u8;
                self.t.chars.push(CharSrc {
                    unit,
                    len,
                    before: unit,
                    after: unit + len as u32,
                    group: u32::MAX,
                });
            }
            k += ch.len_utf16();
        }
    }

    fn flush_para(&mut self, force: bool) {
        if !force && self.para.runs.is_empty() {
            return;
        }
        let mut p = std::mem::take(&mut self.para);
        p.align = self.para_align.take().unwrap_or(self.align);
        let mut src = None;
        if self.t.on {
            let t = &mut self.t;
            src = Some(ParaSrc {
                start: t.region_start,
                own_headers: std::mem::take(&mut t.heads),
                eff_header: t.start_eff.take().unwrap_or(t.eff),
                chars: std::mem::take(&mut t.chars),
                end: t.tok_start,
                end_explicit: t.tok_para_end,
                page_break: None,
                in_row: false,
            });
            t.region_start = if t.tok_resets_after {
                t.tok_end
            } else {
                t.tok_start
            };
        }
        if let Some(row) = self.row.as_mut() {
            if let Some(cell) = row.cells.last_mut() {
                if let Some(mut s) = src {
                    s.in_row = true;
                    p.src = Src(Some(Box::new(s)));
                }
                cell.paragraphs.push(p);
                return;
            }
        }
        if self.table.is_some() {
            self.close_table();
        }
        p.page_break_before = std::mem::take(&mut self.page_break);
        if let Some(mut s) = src {
            if p.page_break_before {
                s.page_break = self.t.page_break.take();
            }
            p.src = Src(Some(Box::new(s)));
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
            let row_src = r.src.get().cloned();
            for c in r.cells {
                if c.paragraphs.is_empty() {
                    // an empty line of the box still takes a line
                    let mut p = Paragraph::default();
                    if let (Some(rs), Some(cs)) = (&row_src, c.src.get()) {
                        p.src = Src(Some(Box::new(ParaSrc {
                            start: rs.header.0,
                            own_headers: vec![rs.header],
                            eff_header: Some(rs.header),
                            chars: Vec::new(),
                            end: rs.end.unwrap_or(cs.rec.0 + cs.rec.1),
                            end_explicit: false,
                            page_break: None,
                            in_row: true,
                        })));
                    }
                    blocks.push(Block::Paragraph(p));
                }
                for p in c.paragraphs {
                    blocks.push(Block::Paragraph(p));
                }
            }
        }
        return;
    }
    blocks.push(Block::Table(t));
}

fn tok_start(t: &Token) -> usize {
    match t {
        Token::Text { start, .. }
        | Token::Record { start, .. }
        | Token::Inline { start, .. }
        | Token::Control { start, .. } => *start,
    }
}

fn build_blocks(
    units: &[u16],
    map: &StyleMap,
    _warnings: &mut Vec<String>,
    track: bool,
) -> (Vec<Block>, Vec<(u32, u32)>) {
    let tokens = text::tokenize(units);
    let mut b = Builder {
        map,
        blocks: Vec::new(),
        para: Paragraph::default(),
        align: Align::Left,
        table: None,
        row: None,
        page_break: false,
        para_align: None,
        t: Track {
            on: track,
            ..Track::default()
        },
    };
    for (ti, t) in tokens.iter().enumerate() {
        let start = tok_start(t) as u32;
        let end = tokens.get(ti + 1).map(tok_start).unwrap_or(units.len()) as u32;
        // a context record (class 0) right before an inline segment belongs to it
        let ctx = ti.checked_sub(1).and_then(|i| match &tokens[i] {
            Token::Record {
                start, class: 0, ..
            } => Some(*start as u32),
            _ => None,
        });
        b.t.tok_start = start;
        b.t.tok_end = end;
        b.t.tok_para_end = matches!(
            t,
            Token::Control {
                code: text::PARA_END,
                ..
            }
        );
        b.t.tok_resets_after = matches!(
            t,
            Token::Control {
                code: text::PARA_END | text::ROW_END,
                ..
            }
        );
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
                if row_spec.is_none() && b.row.is_none() && !b.para.runs.is_empty() {
                    // A line header inside a line (after text, before 000A):
                    // the paragraph goes on; the new state applies from here.
                    if b.para_align.is_none() {
                        b.para_align = Some(b.align);
                    }
                    b.align = align;
                    if track {
                        b.t.eff = Some((start, end - start));
                    }
                    continue;
                }
                b.flush_para(false);
                if let Some(width) = row_spec {
                    b.close_row();
                    let t = b.table.get_or_insert_with(Table::default);
                    t.width = t.width.max(width);
                    b.row = Some(Row {
                        cells: Vec::new(),
                        rules,
                        src: if track {
                            Src(Some(Box::new(RowSrc {
                                header: (start, end - start),
                                end: None,
                            })))
                        } else {
                            Src::default()
                        },
                    });
                } else if b.row.is_none() && b.table.is_some() {
                    b.close_table();
                }
                b.align = align;
                if track {
                    if b.t.chars.is_empty() {
                        b.t.heads.push((start, end - start));
                    }
                    b.t.eff = Some((start, end - start));
                }
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
                    src: if track {
                        Src(Some(Box::new(CellSrc {
                            rec: (start, end - start),
                        })))
                    } else {
                        Src::default()
                    },
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
                    if track {
                        if b.t.start_eff.is_none() {
                            b.t.start_eff = Some(b.t.eff);
                        }
                        let g = b.t.groups.len() as u32;
                        let gs = ctx.unwrap_or(start);
                        b.t.groups.push((gs, end));
                        let n = text.chars().count();
                        let mut k = *text_start as u32;
                        for (i, ch) in text.chars().enumerate() {
                            let len = ch.len_utf16() as u32;
                            b.t.chars.push(CharSrc {
                                unit: k,
                                len: len as u8,
                                before: if i == 0 { gs } else { k },
                                after: if i + 1 == n { end } else { k + len },
                                group: g,
                            });
                            k += len;
                        }
                    }
                }
                InlineKind::RubyReading => {
                    if let Some(r) = b.para.runs.last_mut() {
                        r.ruby = Some(text.clone());
                    }
                    if track {
                        if let Some(c) = b.t.chars.last_mut() {
                            c.after = end;
                            if let Some(g) = b.t.groups.get_mut(c.group as usize) {
                                g.1 = end;
                            }
                        }
                    }
                }
                InlineKind::Hidden => {}
            },
            Token::Control { code, .. } => match *code {
                text::PARA_END => {
                    // 000A always ends a paragraph, empty lines in cells included
                    // (a row with no cell yet has nowhere to put one)
                    let has_cell = b.row.as_ref().map(|r| !r.cells.is_empty()).unwrap_or(true);
                    b.flush_para(has_cell);
                    // an empty cell paragraph is dropped; its region ends here too
                    b.t.region_start = end;
                    b.t.heads.clear();
                }
                text::ROW_END => {
                    if let Some(rs) = b.row.as_mut().and_then(|r| r.src.0.as_mut()) {
                        rs.end = Some(start);
                    }
                    b.close_row();
                    b.t.region_start = end;
                    b.t.heads.clear();
                }
                text::PAGE_BREAK => {
                    b.flush_para(false);
                    if b.row.is_none() {
                        b.close_table();
                        b.page_break = true;
                        b.t.page_break = Some(start);
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
    b.t.tok_start = units.len() as u32;
    b.t.tok_end = units.len() as u32;
    b.t.tok_para_end = false;
    b.flush_para(false);
    b.close_row();
    b.close_table();
    // drop trailing empty paragraphs
    while b.blocks.len() > 1 && matches!(b.blocks.last(), Some(Block::Paragraph(p)) if p.is_empty())
    {
        b.blocks.pop();
    }
    if b.blocks.is_empty() {
        b.blocks.push(Block::Paragraph(Paragraph::default()));
    }
    (b.blocks, b.t.groups)
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
