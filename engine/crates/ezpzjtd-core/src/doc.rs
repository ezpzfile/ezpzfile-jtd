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

/// Paragraph indents (インデント), line header TLV `0026`: (unit, left,
/// right, first-line left, first-line right). Spec §4.1.
#[derive(Debug, Clone, Copy, Serialize, Default, PartialEq, Eq)]
pub struct Indent {
    /// Values in 1/100 mm; otherwise in half-width columns.
    pub mm: bool,
    pub left: i16,
    pub right: i16,
    /// Left and right of the first line, from the margins (not from `left`).
    pub first_left: i16,
    pub first_right: i16,
}

impl Indent {
    pub(crate) fn from_tlv(v: &[u16]) -> Option<Indent> {
        if v.len() < 5 {
            return None;
        }
        let i = Indent {
            mm: v[0] == 1,
            left: v[1] as i16,
            right: v[2] as i16,
            first_left: v[3] as i16,
            first_right: v[4] as i16,
        };
        (i.left != 0 || i.right != 0 || i.first_left != 0 || i.first_right != 0).then_some(i)
    }
    /// The four values in points, given the width of one character cell.
    pub(crate) fn to_tlv(self) -> Vec<u16> {
        vec![
            self.mm as u16,
            self.left as u16,
            self.right as u16,
            self.first_left as u16,
            self.first_right as u16,
        ]
    }
    pub fn points(&self, cell: f32) -> [f32; 4] {
        let k = if self.mm { 72.0 / 2540.0 } else { cell / 2.0 };
        [self.left, self.right, self.first_left, self.first_right].map(|v| v as f32 * k)
    }
}

/// Line feed after each line of the paragraph (改行幅), line header TLV
/// `0020`: (kind, value, kind, value). Spec §4.1.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct LineFeed {
    /// 1: none (0), 2: 1/2, 3: 1/3, 4: 1/4, 5: 2/3, 6: 3/4, 7: ruby,
    /// 8: `value` in 1/100 mm, 9: `value` in 0.1 % of the normal feed.
    pub kind: u16,
    pub value: u16,
}

impl LineFeed {
    pub(crate) fn from_tlv(v: &[u16]) -> Option<LineFeed> {
        let (kind, value) = (*v.first()?, v.get(1).copied().unwrap_or(0));
        (1..=9).contains(&kind).then_some(LineFeed { kind, value })
    }
    /// As the latest 一太郎 writes it: the second pair empty.
    pub(crate) fn to_tlv(self) -> Vec<u16> {
        vec![self.kind, self.value, 0, 0]
    }
    /// The feed in points, given the normal one. `None`: the normal feed.
    pub fn points(&self, normal: f32) -> Option<f32> {
        let f = match self.kind {
            1 => 0.0,
            2 => normal / 2.0,
            3 => normal / 3.0,
            4 => normal / 4.0,
            5 => normal * 2.0 / 3.0,
            6 => normal * 3.0 / 4.0,
            8 if self.value > 0 => self.value as f32 * 72.0 / 2540.0,
            9 if self.value > 0 => normal * self.value as f32 / 1000.0,
            _ => return None,
        };
        Some(f)
    }
}

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct Paragraph {
    pub align: Align,
    pub runs: Vec<Run>,
    /// Start this paragraph on a new page (改ページ).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub page_break_before: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indent: Option<Indent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feed: Option<LineFeed>,
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
    /// Where the first rule's centre is, minus 1 (the third word of TLV 0x8f).
    pub x0: u16,
    /// The ruled-line items of this text line (TLV tag 0x8f). See spec §4.3.
    pub rules: Vec<Rule>,
    /// Line feed (改行幅) of the line that holds the row.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feed: Option<LineFeed>,
    /// Line types (線種) of `rules`, item by item; empty when all are plain.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub kinds: Vec<RuleKinds>,
    #[serde(skip)]
    pub src: Src<RowSrc>,
}

/// Line types (線種 1-16; 0 = the default, type 1) of one ruled-line item.
/// Ichitaro keeps them as style properties on the item's words (spec §4.3):
/// on the first word, property 1 and 2 for the upper and lower half of the
/// vertical rule and property 3 / 8 for the item's own horizontal line
/// through the middle / under the line; on the third word, property 3 / 8
/// for the line to the next item.
#[derive(Debug, Clone, Copy, Serialize, Default, PartialEq, Eq)]
pub struct RuleKinds {
    pub up: u8,
    pub down: u8,
    pub mid: u8,
    pub below: u8,
    pub next_mid: u8,
    pub next_below: u8,
    /// The vertical rule that `b` places at the end of the item's own line.
    pub b_up: u8,
    pub b_down: u8,
}

/// One item of a ruled line: `(style, a, b, dist)` (spec §4.3).
///
/// `style`: 0x10 half-width rule, 0x20 full-width rule (0 when there is no
/// vertical rule), plus bits 1 (vertical, upper half of the line), 2 (lower
/// half), 4 (horizontal line through the middle of the line, starting here)
/// and 8 (horizontal line under the line, starting here). `a`: length of a
/// horizontal line that does not run to the next rule. `b`: the horizontal
/// line from here to the next rule (8 under the line, class + 4 through the
/// middle). The next item is `a + dist + 2` grid units further on.
#[derive(Debug, Clone, Copy, Serialize, Default, PartialEq, Eq)]
pub struct Rule {
    pub style: u16,
    pub a: u16,
    pub b: u16,
    pub dist: u16,
}

impl Rule {
    pub const UP: u16 = 1;
    pub const DOWN: u16 = 2;
    pub const MID: u16 = 4;
    pub const BELOW: u16 = 8;

    /// Vertical bits (UP, DOWN) when this item draws a vertical rule.
    pub fn vertical(&self) -> u16 {
        if self.style >= 0x10 {
            self.style & 3
        } else {
            0
        }
    }
}

/// The lines one ruled text line draws, in grid units (rule centres).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RowLines {
    /// `(x, upper half, lower half)`
    pub verticals: Vec<(u16, bool, bool)>,
    /// `(x1, x2)` under the line
    pub below: Vec<(u16, u16)>,
    /// `(x1, x2)` through the middle of the line
    pub middle: Vec<(u16, u16)>,
    /// Line types, one per entry of the lists above: `(upper, lower)` for
    /// the verticals.
    pub vertical_kinds: Vec<(u8, u8)>,
    pub below_kinds: Vec<u8>,
    pub middle_kinds: Vec<u8>,
}

impl Row {
    /// True when the line has at least one drawn vertical rule.
    pub fn ruled(&self) -> bool {
        self.rules.iter().any(|r| r.vertical() != 0)
    }

    /// Where the rules and horizontal lines of this line are.
    pub fn lines(&self) -> RowLines {
        let mut out = RowLines::default();
        let mut c = self.x0 as u32 + 1;
        for (i, r) in self.rules.iter().enumerate() {
            let k = self.kinds.get(i).copied().unwrap_or_default();
            let next = c + r.a as u32 + r.dist as u32 + 2;
            let v = r.vertical();
            if v != 0 {
                out.verticals
                    .push((c as u16, v & Rule::UP != 0, v & Rule::DOWN != 0));
                out.vertical_kinds.push((k.up, k.down));
            }
            // `b` with a class (0x10 / 0x20) is the style of a vertical rule
            // at the end of this item's own line (`centre + a + 1`)
            if r.b >= 0x10 && r.b & 3 != 0 {
                let x = (c + r.a as u32 + 1).min(u16::MAX as u32) as u16;
                out.verticals
                    .push((x, r.b & Rule::UP != 0, r.b & Rule::DOWN != 0));
                out.vertical_kinds.push((k.b_up, k.b_down));
            }
            let own = r.style & (Rule::MID | Rule::BELOW);
            let to_next = r.b & (Rule::MID | Rule::BELOW);
            // kinds: (through the middle, under the line)
            let mut push = |bits: u16, x1: u32, x2: u32, (km, kb): (u8, u8)| {
                let seg = (
                    x1.min(u16::MAX as u32) as u16,
                    x2.min(u16::MAX as u32) as u16,
                );
                if x2 > x1 {
                    if bits & Rule::BELOW != 0 {
                        out.below.push(seg);
                        out.below_kinds.push(kb);
                    }
                    if bits & Rule::MID != 0 {
                        out.middle.push(seg);
                        out.middle_kinds.push(km);
                    }
                }
            };
            if own != 0 && r.a > 0 {
                push(own, c, c + r.a as u32 + 1, (k.mid, k.below));
            }
            if to_next != 0 {
                push(to_next, c, next, (k.next_mid, k.next_below));
            } else if own != 0 && r.a == 0 {
                push(own, c, next, (k.mid, k.below));
            }
            c = next;
        }
        out
    }
}

/// Index (in the record payload) of the first value of TLV `tag`.
fn tlv_value_at(payload: &[u16], tag: u16) -> Option<usize> {
    let mut j = 1usize;
    while j + 1 < payload.len() && payload[j] != 0xffff {
        if payload[j] == tag {
            return Some(j + 2);
        }
        j += 2 + payload[j + 1] as usize;
    }
    None
}

/// Line types of the items of a ruled line whose TLV values start at unit
/// `v0` (see RuleKinds). Empty when every item is plain.
fn rule_kinds(map: &StyleMap, v0: usize, n: usize) -> Vec<RuleKinds> {
    let get = |u: usize, id: u8| -> u8 {
        match map.at(u).and_then(|p| p.get(&id).copied()) {
            Some(v) if (2..=16).contains(&v) => v as u8,
            _ => 0,
        }
    };
    let mut out = Vec::new();
    let mut i = 3usize;
    while i + 1 < n {
        let full = i + 3 < n;
        let u = v0 + i;
        out.push(RuleKinds {
            up: get(u, 1),
            down: get(u, 2),
            mid: get(u, 3),
            below: get(u, 8),
            next_mid: if full { get(u + 2, 3) } else { 0 },
            next_below: if full { get(u + 2, 8) } else { 0 },
            b_up: if full { get(u + 2, 1) } else { 0 },
            b_down: if full { get(u + 2, 2) } else { 0 },
        });
        i += if full { 4 } else { 2 };
    }
    if out.iter().all(|k| *k == RuleKinds::default()) {
        out.clear();
    }
    out
}

/// Parse the item list of TLV tag 0x8f: `[width, 0, x0]` then 4-word items
/// `(style, a, b, dist)`; the final item may be cut to `(style, dist)`.
pub fn parse_rules(v: &[u16]) -> (u16, Vec<Rule>) {
    let x0 = v.get(2).copied().unwrap_or(0);
    let mut out = Vec::new();
    let mut i = 3usize;
    while i + 1 < v.len() {
        if i + 3 < v.len() {
            out.push(Rule {
                style: v[i],
                a: v[i + 1],
                b: v[i + 2],
                dist: v[i + 3],
            });
            i += 4;
        } else {
            out.push(Rule {
                style: v[i],
                a: 0,
                b: 0,
                dist: v[i + 1],
            });
            i += 2;
        }
    }
    (x0, out)
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
    /// Page setup (文書スタイル) stored in the file, if it could be read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<crate::layout::PageSetup>,
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
            page: None,
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
            "this file is RTF (rich text) with a .jtd name; open it in any word processor".into(),
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
            "compressed Ichitaro document (.jttc / -lh5-) is not supported yet".into(),
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

    let page = cfb
        .read("/DocumentViewStyles")
        .and_then(|b| crate::page::parse(&b))
        .map(|p| p.setup());

    Ok(Document {
        format,
        summary,
        fonts,
        sheets,
        objects,
        warnings,
        page,
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
    indent: Option<Indent>,
    feed: Option<LineFeed>,
    /// Like `para_align`, for indents and line feed.
    para_fmt: Option<(Option<Indent>, Option<LineFeed>)>,
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

    /// No line header in effect: the defaults (left, no indent, normal feed).
    fn reset_line_state(&mut self) {
        self.align = Align::Left;
        (self.indent, self.feed) = (None, None);
        self.t.eff = None;
    }

    fn flush_para(&mut self, force: bool) {
        if !force && self.para.runs.is_empty() {
            return;
        }
        let mut p = std::mem::take(&mut self.para);
        p.align = self.para_align.take().unwrap_or(self.align);
        (p.indent, p.feed) = self.para_fmt.take().unwrap_or((self.indent, self.feed));
        if self.row.is_some() {
            // the line header belongs to the row (see Row::feed)
            (p.indent, p.feed) = (None, None);
        }
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
            let feed = r.feed;
            for c in r.cells {
                if c.paragraphs.is_empty() {
                    // an empty line of the box still takes a line
                    let mut p = Paragraph {
                        feed,
                        ..Paragraph::default()
                    };
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
                for mut p in c.paragraphs {
                    p.feed = feed;
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
        indent: None,
        feed: None,
        para_fmt: None,
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
                let mut x0 = 0u16;
                let (mut indent, mut feed) = (None, None);
                let mut kinds = Vec::new();
                for (tag, v) in &tlv {
                    match *tag {
                        0x20 => feed = LineFeed::from_tlv(v),
                        0x26 => indent = Indent::from_tlv(v),
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
                            (x0, rules) = parse_rules(v);
                            if let Some(at) = tlv_value_at(payload, 0x8f) {
                                kinds = rule_kinds(b.map, start as usize + 3 + at, v.len());
                            }
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
                    if b.para_fmt.is_none() {
                        b.para_fmt = Some((b.indent, b.feed));
                    }
                    b.align = align;
                    (b.indent, b.feed) = (indent, feed);
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
                        x0,
                        rules,
                        feed,
                        kinds,
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
                (b.indent, b.feed) = (indent, feed);
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
                    // a line header inside this paragraph is meant for the next one
                    let next_has_header = b.para_align.is_some() || b.para_fmt.is_some();
                    b.flush_para(has_cell);
                    if b.row.is_none() && !next_has_header {
                        // a line header formats its own paragraph only: one with
                        // no header has the defaults [一太郎 2026: formatting
                        // the middle of three paragraphs writes one header]
                        b.reset_line_state();
                    }
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
                    // the row's line header is the row's own
                    b.reset_line_state();
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
