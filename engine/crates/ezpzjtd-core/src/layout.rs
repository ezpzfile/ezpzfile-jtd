//! Page layout on Ichitaro's character grid (字 × 行).
//!
//! Ichitaro lays text out on a fixed grid: a page has N characters per line
//! and M lines per page. A full-width character takes one cell, a half-width
//! one half a cell. We do the same, which keeps line breaks close to the
//! original and makes the layout cheap enough to redo on every keystroke.
//!
//! All coordinates are in points (1/72 inch), relative to the page's top-left.

use crate::doc::{Align, Block, Paragraph, Table};
use crate::style::CharStyle;
use serde::Serialize;

/// Page geometry. Defaults to Ichitaro's A4 portrait, 40 字 × 36 行, 10.5 pt.
#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq)]
pub struct PageSetup {
    pub width_mm: f32,
    pub height_mm: f32,
    pub margin_top_mm: f32,
    pub margin_bottom_mm: f32,
    pub chars_per_line: u32,
    pub lines_per_page: u32,
    pub font_pt: f32,
}

impl Default for PageSetup {
    fn default() -> Self {
        PageSetup {
            width_mm: 210.0,
            height_mm: 297.0,
            margin_top_mm: 35.0,
            margin_bottom_mm: 30.0,
            chars_per_line: 40,
            lines_per_page: 36,
            font_pt: 10.5,
        }
    }
}

pub const MM: f32 = 72.0 / 25.4;

impl PageSetup {
    pub fn page_w(&self) -> f32 {
        self.width_mm * MM
    }
    pub fn page_h(&self) -> f32 {
        self.height_mm * MM
    }
    pub fn text_w(&self) -> f32 {
        self.chars_per_line as f32 * self.font_pt
    }
    pub fn left(&self) -> f32 {
        ((self.page_w() - self.text_w()) / 2.0).max(10.0)
    }
    pub fn top(&self) -> f32 {
        self.margin_top_mm * MM
    }
    pub fn bottom(&self) -> f32 {
        self.page_h() - self.margin_bottom_mm * MM
    }
    /// Line pitch: text height divided by lines per page.
    pub fn pitch(&self) -> f32 {
        (self.bottom() - self.top()) / self.lines_per_page.max(1) as f32
    }
    /// One table-grid unit. Ichitaro tables are 4 units per character.
    pub fn unit(&self) -> f32 {
        self.font_pt / 4.0
    }
}

/// Something to draw.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "t")]
pub enum Item {
    /// Characters of one style; `xs[i]` is the left edge of character `i`.
    Text {
        xs: Vec<f32>,
        y: f32,
        s: String,
        size: f32,
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        b: bool,
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        i: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        color: Option<String>,
        /// Half-width characters are drawn left-aligned in a half cell.
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        pre: bool,
    },
    Line {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        w: f32,
        #[serde(skip_serializing_if = "Option::is_none")]
        color: Option<String>,
    },
    /// Editing marks (編集記号): "ret" paragraph end, "sp" full-width space,
    /// "pb" page break, "tab".
    Mark {
        k: &'static str,
        x: f32,
        y: f32,
        size: f32,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct Page {
    pub w: f32,
    pub h: f32,
    pub items: Vec<Item>,
}

/// One laid-out line, used for caret placement and hit testing.
#[derive(Debug, Clone)]
pub struct LineBox {
    pub page: usize,
    pub top: f32,
    pub height: f32,
    pub baseline: f32,
    /// Index into the editor's flat paragraph list.
    pub para: usize,
    pub start: usize,
    pub end: usize,
    /// x of every character boundary from `start` to `end` (len = end-start+1).
    pub xs: Vec<f32>,
    /// Horizontal extent of the container (text area or cell interior).
    pub left: f32,
    pub right: f32,
    /// Last line of its paragraph.
    pub last: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Layout {
    pub pages: Vec<Page>,
    pub lines: Vec<LineBox>,
}

/// Preedit (IME composition) text shown at a position, not part of the model.
pub struct Preedit<'a> {
    pub para: usize,
    pub off: usize,
    pub text: &'a str,
}

pub fn is_half(c: char) -> bool {
    let u = c as u32;
    (0x20..0x7f).contains(&u) || (0xff61..=0xff9f).contains(&u) || (0xa0..0x250).contains(&u)
}

const NO_START: &str = "、。，．,.)）]］｝」』】〕〉》〙〗’”ゝゞーァィゥェォッャュョヮヵヶぁぃぅぇぉっゃゅょゎゕゖ・：；？！!?";
const HANG: &str = "、。，．,.";
const NO_END: &str = "(（[［｛「『【〔〈《〘〖‘“";

#[derive(Clone)]
struct G {
    ch: char,
    w: f32,
    style: CharStyle,
    /// Model offset of this character (None for preedit characters).
    off: Option<usize>,
    run: usize,
    pre: bool,
}

struct Ctx<'a> {
    setup: &'a PageSetup,
    show_marks: bool,
    out: Layout,
    page: usize,
    y: f32,
}

impl<'a> Ctx<'a> {
    fn new_page(&mut self) {
        self.out.pages.push(Page {
            w: self.setup.page_w(),
            h: self.setup.page_h(),
            items: Vec::new(),
        });
        self.page = self.out.pages.len() - 1;
        self.y = self.setup.top();
    }
    fn items(&mut self) -> &mut Vec<Item> {
        &mut self.out.pages[self.page].items
    }
}

fn glyphs(p: &Paragraph, base: f32, pre: Option<(usize, &str)>) -> Vec<G> {
    let mut out = Vec::new();
    let mut off = 0usize;
    let push_pre = |out: &mut Vec<G>, style: &CharStyle, txt: &str| {
        let mut st = style.clone();
        st.underline = Some(1);
        for ch in txt.chars() {
            let size = st.size_pt.unwrap_or(base);
            out.push(G {
                ch,
                w: if is_half(ch) { size / 2.0 } else { size },
                style: st.clone(),
                off: None,
                run: usize::MAX,
                pre: true,
            });
        }
    };
    for (ri, r) in p.runs.iter().enumerate() {
        for ch in r.text.chars() {
            if let Some((po, txt)) = pre {
                if po == off {
                    push_pre(&mut out, &r.style, txt);
                }
            }
            let size = r.style.size_pt.unwrap_or(base);
            let w = if ch == '\t' {
                size * 4.0
            } else if is_half(ch) {
                size / 2.0
            } else {
                size
            };
            out.push(G {
                ch,
                w,
                style: r.style.clone(),
                off: Some(off),
                run: ri,
                pre: false,
            });
            off += 1;
        }
    }
    if let Some((po, txt)) = pre {
        if po >= off {
            let st = p.runs.last().map(|r| r.style.clone()).unwrap_or_default();
            push_pre(&mut out, &st, txt);
        }
    }
    out
}

/// Break glyphs into lines that fit `width`. Returns index ranges.
fn break_lines(gs: &[G], width: f32) -> Vec<(usize, usize)> {
    let mut lines = Vec::new();
    let mut s = 0usize;
    let mut w = 0.0f32;
    let mut i = 0usize;
    while i < gs.len() {
        let g = &gs[i];
        if w + g.w > width + 0.01 && i > s {
            let mut cut = i;
            if NO_START.contains(g.ch) {
                if HANG.contains(g.ch) {
                    cut = i + 1; // ぶら下げ: let the comma hang past the margin
                } else if cut - s > 1 {
                    cut -= 1; // 追い出し: carry the previous character over
                }
            }
            while cut - s > 1 && NO_END.contains(gs[cut - 1].ch) {
                cut -= 1;
            }
            lines.push((s, cut));
            s = cut;
            w = 0.0;
            i = cut;
            continue;
        }
        w += g.w;
        i += 1;
    }
    if s < gs.len() || lines.is_empty() {
        lines.push((s, gs.len()));
    }
    lines
}

/// Lay out one paragraph inside [left, right]. `para` is the flat index.
fn layout_para(
    ctx: &mut Ctx,
    p: &Paragraph,
    para: usize,
    left: f32,
    right: f32,
    pre: Option<&Preedit>,
    in_cell: bool,
) -> f32 {
    let setup = ctx.setup;
    let base = setup.font_pt;
    let pitch = setup.pitch();
    let pre_here = pre.filter(|x| x.para == para).map(|x| (x.off, x.text));
    let gs = glyphs(p, base, pre_here);
    let width = (right - left).max(base);
    let ranges = break_lines(&gs, width);
    let mut used = 0.0;
    let nlines = ranges.len();
    let mut model_pos = 0usize;
    for (li, &(a, b)) in ranges.iter().enumerate() {
        let line = &gs[a..b];
        let max_size = line
            .iter()
            .map(|g| g.style.size_pt.unwrap_or(base))
            .fold(base, f32::max);
        let has_ruby = line
            .iter()
            .any(|g| g.run != usize::MAX && p.runs[g.run].ruby.is_some());
        let mut h = pitch;
        let need = max_size * 1.25 + if has_ruby { max_size * 0.5 } else { 0.0 };
        while h < need {
            h += pitch;
        }
        if !in_cell && ctx.y + h > setup.bottom() + 0.5 && ctx.y > setup.top() + 0.5 {
            ctx.new_page();
        }
        let top = ctx.y;
        let baseline = top + h - (pitch - base) / 2.0 - base * 0.12;
        let lw: f32 = line.iter().map(|g| g.w).sum();
        let x0 = match p.align {
            Align::Center => left + ((width - lw) / 2.0).max(0.0),
            Align::Right => left + (width - lw).max(0.0),
            _ => left,
        };
        // Per-character x positions
        let mut x = x0;
        let mut gx = Vec::with_capacity(line.len());
        for g in line {
            gx.push(x);
            x += g.w;
        }
        let end_x = x;
        // Model offsets covered by this line and the x of each boundary.
        let start = model_pos;
        let mut xs = Vec::new();
        let mut after = x0;
        for (k, g) in line.iter().enumerate() {
            if let Some(o) = g.off {
                xs.push(gx[k]);
                after = gx[k] + g.w;
                model_pos = o + 1;
            }
        }
        let end = model_pos;
        xs.push(after);
        let is_last = li + 1 == nlines;
        ctx.out.lines.push(LineBox {
            page: ctx.page,
            top,
            height: h,
            baseline,
            para,
            start,
            end,
            xs,
            left,
            right,
            last: is_last,
        });
        // Drawing: group consecutive glyphs with the same look
        let mut k = 0;
        while k < line.len() {
            let st = &line[k].style;
            let mut j = k + 1;
            while j < line.len() && line[j].style.same_look(st) && line[j].pre == line[k].pre {
                j += 1;
            }
            let size = st.size_pt.unwrap_or(base);
            let mut s = String::new();
            let mut xsv = Vec::new();
            for q in k..j {
                let c = line[q].ch;
                if c == '\u{3000}' || c == '\t' {
                    if ctx.show_marks {
                        let k2 = if c == '\t' { "tab" } else { "sp" };
                        let (mx, my) = (gx[q], baseline);
                        ctx.items().push(Item::Mark {
                            k: k2,
                            x: mx,
                            y: my,
                            size,
                        });
                    }
                    continue;
                }
                s.push(c);
                xsv.push(gx[q]);
            }
            if !s.trim().is_empty() {
                ctx.items().push(Item::Text {
                    xs: xsv,
                    y: baseline,
                    s,
                    size,
                    b: st.bold == Some(true),
                    i: st.italic == Some(true),
                    color: st.color.clone(),
                    pre: line[k].pre,
                });
            }
            if let Some(u) = st.underline {
                let x1 = gx[k];
                let x2 = if j < line.len() { gx[j] } else { end_x };
                ctx.items().push(Item::Line {
                    x1,
                    y1: baseline + size * 0.14,
                    x2,
                    y2: baseline + size * 0.14,
                    w: if u == 2 { 1.4 } else { 0.6 },
                    color: if line[k].pre {
                        Some("#2f5bd3".into())
                    } else {
                        st.color.clone()
                    },
                });
            }
            k = j;
        }
        // Ruby over runs on this line
        let mut k = 0;
        while k < line.len() {
            let r = line[k].run;
            let mut j = k + 1;
            while j < line.len() && line[j].run == r {
                j += 1;
            }
            if r != usize::MAX {
                if let Some(rt) = &p.runs[r].ruby {
                    let first_on_line = line[k].off.map(|o| o == run_start(p, r)).unwrap_or(false);
                    if first_on_line {
                        let x1 = gx[k];
                        let x2 = if j < line.len() { gx[j] } else { end_x };
                        let size = line[k].style.size_pt.unwrap_or(base);
                        let rs = size * 0.5;
                        let n = rt.chars().count().max(1) as f32;
                        let span = (x2 - x1).max(rs * n);
                        let step = span / n;
                        let start_x = (x1 + x2) / 2.0 - span / 2.0;
                        let xsr: Vec<f32> = (0..rt.chars().count())
                            .map(|q| start_x + step * q as f32 + (step - rs) / 2.0)
                            .collect();
                        ctx.items().push(Item::Text {
                            xs: xsr,
                            y: baseline - size * 1.0,
                            s: rt.clone(),
                            size: rs,
                            b: false,
                            i: false,
                            color: line[k].style.color.clone(),
                            pre: false,
                        });
                    }
                }
            }
            k = j;
        }
        if is_last && ctx.show_marks && width >= base * 1.5 {
            ctx.items().push(Item::Mark {
                k: "ret",
                x: end_x,
                y: baseline,
                size: base,
            });
        }
        ctx.y += h;
        used += h;
    }
    used
}

fn run_start(p: &Paragraph, r: usize) -> usize {
    p.runs[..r].iter().map(|x| x.text.chars().count()).sum()
}

/// Cell spans snapped so that each cell reaches the next cell's left edge.
pub fn snapped_row(cells: &[(u16, u16)]) -> Vec<(u16, u16)> {
    let n = cells.len();
    (0..n)
        .map(|i| {
            let l = cells[i].0;
            let r = if i + 1 < n {
                cells[i + 1].0
            } else {
                cells[i].1.max(l + 1)
            };
            (l, r.max(l + 1))
        })
        .collect()
}

fn layout_table(ctx: &mut Ctx, t: &Table, flat_start: &mut usize, pre: Option<&Preedit>) {
    let setup = ctx.setup;
    let left = setup.left();
    let width_units = t
        .width
        .max(
            t.rows
                .iter()
                .flat_map(|r| r.cells.iter().map(|c| c.right))
                .max()
                .unwrap_or(1),
        )
        .max(1);
    let mut unit = setup.unit();
    if width_units as f32 * unit > setup.text_w() + setup.font_pt * 2.0 {
        unit = (setup.text_w() + setup.font_pt * 2.0) / width_units as f32;
    }
    let xu = |u: u16| left + u as f32 * unit;
    let pad = unit * 1.5;
    let line_c = Some("#222".to_string());
    let soft_c = Some("#c9c9c9".to_string());
    let nrows = t.rows.len();
    for (ri, row) in t.rows.iter().enumerate() {
        let spans = snapped_row(
            &row.cells
                .iter()
                .map(|c| (c.left, c.right))
                .collect::<Vec<_>>(),
        );
        // Measure: lay out each cell into a scratch context to get its height.
        let mut heights = Vec::new();
        for (ci, c) in row.cells.iter().enumerate() {
            let (l, r) = spans[ci];
            let mut scratch = Ctx {
                setup,
                show_marks: false,
                out: Layout::default(),
                page: 0,
                y: 0.0,
            };
            scratch.out.pages.push(Page {
                w: 0.0,
                h: 0.0,
                items: Vec::new(),
            });
            let mut h = 0.0;
            for (pi, p) in c.paragraphs.iter().enumerate() {
                h += layout_para(
                    &mut scratch,
                    p,
                    *flat_start + pi,
                    xu(l) + pad,
                    xu(r) - pad,
                    pre,
                    true,
                );
            }
            heights.push(h);
            *flat_start += c.paragraphs.len();
        }
        let row_h = heights.iter().cloned().fold(setup.pitch(), f32::max);
        if ctx.y + row_h > setup.bottom() + 0.5 && ctx.y > setup.top() + 0.5 {
            ctx.new_page();
        }
        let top = ctx.y;
        // Draw for real
        *flat_start -= row.cells.iter().map(|c| c.paragraphs.len()).sum::<usize>();
        for (ci, c) in row.cells.iter().enumerate() {
            let (l, r) = spans[ci];
            ctx.y = top;
            for (pi, p) in c.paragraphs.iter().enumerate() {
                layout_para(
                    ctx,
                    p,
                    *flat_start + pi,
                    xu(l) + pad,
                    xu(r) - pad,
                    pre,
                    true,
                );
            }
            *flat_start += c.paragraphs.len();
        }
        let bottom = top + row_h;
        if row.ruled() {
            let prev_ruled = ri > 0 && t.rows[ri - 1].ruled();
            let next_ruled = ri + 1 < nrows && t.rows[ri + 1].ruled();
            let x_first = spans.first().map(|s| xu(s.0)).unwrap_or(left);
            let x_last = spans.last().map(|s| xu(s.1)).unwrap_or(left);
            for &(l, _) in &spans {
                ctx.items().push(Item::Line {
                    x1: xu(l),
                    y1: top,
                    x2: xu(l),
                    y2: bottom,
                    w: 0.7,
                    color: line_c.clone(),
                });
            }
            ctx.items().push(Item::Line {
                x1: x_last,
                y1: top,
                x2: x_last,
                y2: bottom,
                w: 0.7,
                color: line_c.clone(),
            });
            ctx.items().push(Item::Line {
                x1: x_first,
                y1: top,
                x2: x_last,
                y2: top,
                w: if prev_ruled { 0.4 } else { 0.7 },
                color: if prev_ruled {
                    soft_c.clone()
                } else {
                    line_c.clone()
                },
            });
            if !next_ruled {
                ctx.items().push(Item::Line {
                    x1: x_first,
                    y1: bottom,
                    x2: x_last,
                    y2: bottom,
                    w: 0.7,
                    color: line_c.clone(),
                });
            }
        }
        ctx.y = bottom;
    }
}

/// Lay out a sheet. `flat` order: top-level paragraphs and table cell
/// paragraphs in document order (row-major, cell by cell).
pub fn layout(
    blocks: &[Block],
    setup: &PageSetup,
    show_marks: bool,
    pre: Option<&Preedit>,
) -> Layout {
    let mut ctx = Ctx {
        setup,
        show_marks,
        out: Layout::default(),
        page: 0,
        y: 0.0,
    };
    ctx.new_page();
    let mut flat = 0usize;
    for b in blocks {
        match b {
            Block::Paragraph(p) => {
                if p.page_break_before && ctx.y > setup.top() + 0.5 {
                    if show_marks {
                        let y = ctx.y - setup.pitch() * 0.3;
                        let (l, r) = (setup.left(), setup.left() + setup.text_w());
                        ctx.items().push(Item::Mark {
                            k: "pb",
                            x: l,
                            y,
                            size: r - l,
                        });
                    }
                    ctx.new_page();
                }
                layout_para(
                    &mut ctx,
                    p,
                    flat,
                    setup.left(),
                    setup.left() + setup.text_w(),
                    pre,
                    false,
                );
                flat += 1;
            }
            Block::Table(t) => layout_table(&mut ctx, t, &mut flat, pre),
        }
    }
    ctx.out
}
