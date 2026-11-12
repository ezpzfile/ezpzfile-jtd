//! Page layout on Ichitaro's character grid (字 × 行).
//!
//! Ichitaro lays text out on a fixed grid: a page has N characters per line
//! and M lines per page. A full-width character takes one cell, a half-width
//! one half a cell. We do the same, which keeps line breaks close to the
//! original and makes the layout cheap enough to redo on every keystroke.
//!
//! All coordinates are in points (1/72 inch), relative to the page's top-left.

use crate::doc::{Align, Block, LineFeed, Paragraph, Table};
use crate::style::CharStyle;
use serde::Serialize;
use std::rc::Rc;

/// Page geometry. Documents bring their own (文書スタイル, see `page.rs`);
/// new documents use the latest Ichitaro's default: A4 portrait, margins 30 mm,
/// 40 字 × 40 行, 10.5 pt.
#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq)]
pub struct PageSetup {
    pub width_mm: f32,
    pub height_mm: f32,
    pub margin_top_mm: f32,
    pub margin_bottom_mm: f32,
    #[serde(default = "thirty")]
    pub margin_left_mm: f32,
    #[serde(default = "thirty")]
    pub margin_right_mm: f32,
    pub chars_per_line: u32,
    pub lines_per_page: u32,
    pub font_pt: f32,
    /// `/Font` indices whose face is a Gothic (sans) face; the rest are drawn
    /// as Mincho. Filled from the document's font table, not saved.
    #[serde(skip)]
    pub gothic: Vec<u16>,
    /// Face class of each `/Font` index (see [`face_class`]), and the face
    /// used for text without a font of its own. Filled from the document.
    #[serde(skip)]
    pub faces: Vec<u8>,
    #[serde(skip)]
    pub base_face: u8,
    /// Face names by `/Font` index and the document's standard face, for
    /// Word export. Filled from the document.
    #[serde(skip)]
    pub font_names: Vec<String>,
    #[serde(skip)]
    pub base_font: Option<u16>,
    /// Picture frames by id, `(id, x, y, w, h)` in 1/100 mm: drawn where
    /// their anchor's line is (see `frames.rs`). Filled from the document.
    #[serde(skip)]
    pub frames: Vec<(u16, i32, i32, u32, u32)>,
    /// Text boxes by frame id: `(id, x, y)` of the frame (1/100 mm, like
    /// `frames`) and the box's text laid out from the box's top left.
    #[serde(skip)]
    pub boxes: Boxes,
    /// Frames placed on the page rather than from their anchor's line.
    #[serde(skip)]
    pub fixed: Vec<u16>,
    /// Frames the text goes round (回り込み), by id.
    #[serde(skip)]
    pub wrap: Vec<u16>,
    /// Lines as tall as their characters plus a small gap, not on the page's
    /// line grid (text boxes).
    #[serde(skip)]
    pub free_lines: bool,
    /// 縦組み: the page is laid out turned a quarter turn (width and height
    /// swapped, the right margin on top), so lines run top to bottom and
    /// follow each other from right to left once the drawing is turned back.
    /// The editor turns it back when drawing (see `PageStyle::setup`).
    #[serde(default)]
    pub vertical: bool,
    /// 段組: columns and the gap between them (mm). Lines fill a column,
    /// then the next; 字数 counts one column.
    #[serde(default = "one_col")]
    pub columns: u32,
    #[serde(default)]
    pub col_gap_mm: f32,
    /// The page cut across in halves, each with its own columns (see
    /// `PageStyle::rows`)
    #[serde(default = "one_col")]
    pub rows: u32,
}

fn one_col() -> u32 {
    1
}

fn thirty() -> f32 {
    30.0
}

impl Default for PageSetup {
    fn default() -> Self {
        PageSetup {
            width_mm: 210.0,
            height_mm: 297.0,
            margin_top_mm: 30.0,
            margin_bottom_mm: 30.0,
            margin_left_mm: 30.0,
            margin_right_mm: 30.0,
            chars_per_line: 40,
            lines_per_page: 40,
            font_pt: 10.5,
            gothic: Vec::new(),
            faces: Vec::new(),
            base_face: 0,
            font_names: Vec::new(),
            base_font: None,
            frames: Vec::new(),
            boxes: Default::default(),
            fixed: Vec::new(),
            wrap: Vec::new(),
            free_lines: false,
            vertical: false,
            columns: 1,
            col_gap_mm: 0.0,
            rows: 1,
        }
    }
}

pub const MM: f32 = 72.0 / 25.4;

/// Most laid-out lines in one sheet (each line carries its characters'
/// positions). Far above real documents; see `limits.rs`.
pub const MAX_LINES: usize = 400_000;
/// How much of a pitch the tallest character may need and still take one
/// pitch. 11 pt in an 11.6 pt pitch (A4, 65 lines) and 16 pt in a 15.7 pt
/// pitch (B4 landscape, 42 lines: the a320 handout) both stay one pitch in
/// 一太郎ビューア.
const NEED_FIT: f32 = 0.95;

/// A character cell in columns, in character sizes (see `layout_with`).
const COL_CELL: f32 = 1.031;

/// Text moved below a text box across the column keeps this far below it
/// (一太郎ビューア on a B4 handout: the line after the 持ち物 box, which has a
/// shadow, starts one line further down). Lines go right under a picture
/// (the school name 1.8 mm under the title picture) and beside a narrow box.
const WRAP_GAP: f32 = 4.0 * MM;

fn is_plain_kind(k: &u8) -> bool {
    *k <= 1
}

/// Stroke width (points) of a ruled line of Ichitaro line type `k` (線種),
/// as the latest 一太郎 draws them: types 2, 5, 9, 11, 13 are medium, 3 and 6
/// thick, 16 a hairline.
pub fn line_width(k: u8) -> f32 {
    match k {
        2 | 5 | 9 | 11 | 13 => 1.3,
        3 | 6 | 15 => 2.0,
        16 => 0.4,
        _ => 0.7,
    }
}

impl PageSetup {
    /// The same page with values a layout can use: paper 50 to 1500 mm, margins
    /// leaving at least 10% of the page, 4 to 400 characters, 2 to 400 lines,
    /// 4 to 100 pt. Values read from a file are already cut this way
    /// (`page.rs`); this also covers values set from outside (the page dialog,
    /// JSON from an embedding page).
    pub fn sane(&self) -> PageSetup {
        let d = PageSetup::default();
        let fin = |v: f32, lo: f32, hi: f32, def: f32| if v.is_finite() && (lo..=hi).contains(&v) { v } else { def };
        let w = fin(self.width_mm, 50.0, 1500.0, d.width_mm);
        let h = fin(self.height_mm, 50.0, 1500.0, d.height_mm);
        let m = |a: f32, z: f32, total: f32, da: f32, dz: f32| {
            let ok = a.is_finite() && z.is_finite() && a >= 0.0 && z >= 0.0 && a + z < total * 0.9;
            if ok { (a, z) } else { (da.min(total * 0.2), dz.min(total * 0.2)) }
        };
        let (t, b) = m(self.margin_top_mm, self.margin_bottom_mm, h, d.margin_top_mm, d.margin_bottom_mm);
        let (l, r) = m(self.margin_left_mm, self.margin_right_mm, w, d.margin_left_mm, d.margin_right_mm);
        PageSetup {
            width_mm: w,
            height_mm: h,
            margin_top_mm: t,
            margin_bottom_mm: b,
            margin_left_mm: l,
            margin_right_mm: r,
            chars_per_line: self.chars_per_line.clamp(4, 400),
            lines_per_page: self.lines_per_page.clamp(2, 400),
            font_pt: fin(self.font_pt, 4.0, 100.0, d.font_pt),
            gothic: self.gothic.clone(),
            faces: self.faces.clone(),
            base_face: self.base_face,
            font_names: self.font_names.clone(),
            base_font: self.base_font,
            frames: self.frames.clone(),
            boxes: self.boxes.clone(),
            fixed: self.fixed.clone(),
            wrap: self.wrap.clone(),
            free_lines: self.free_lines,
            vertical: self.vertical,
            columns: self.columns.clamp(1, 10),
            col_gap_mm: if self.col_gap_mm.is_finite() { self.col_gap_mm.clamp(0.0, 100.0) } else { 0.0 },
            rows: self.rows.clamp(1, 2),
        }
    }
    pub fn page_w(&self) -> f32 {
        self.width_mm * MM
    }
    pub fn page_h(&self) -> f32 {
        self.height_mm * MM
    }
    /// Width between the left and right margins.
    pub fn text_w(&self) -> f32 {
        let w = self.page_w() - (self.margin_left_mm + self.margin_right_mm) * MM;
        if w > self.font_pt * 2.0 {
            w
        } else {
            self.chars_per_line as f32 * self.font_pt
        }
    }
    pub fn left(&self) -> f32 {
        let w = self.page_w() - (self.margin_left_mm + self.margin_right_mm) * MM;
        if w > self.font_pt * 2.0 {
            self.margin_left_mm * MM
        } else {
            ((self.page_w() - self.text_w()) / 2.0).max(10.0)
        }
    }
    /// One character cell: the text width split into `chars_per_line`
    /// cells. Ichitaro spreads the characters over the line this way
    /// (字間 = cell − character size).
    pub fn cell(&self) -> f32 {
        self.text_w() / self.chars_per_line.max(1) as f32
    }
    /// Horizontal scale from character size to advance (1 + 字間). When the
    /// document asks for more characters than fit, Ichitaro places them at
    /// the narrower cell pitch too (seen with 90 字 on landscape A4).
    pub fn spread(&self) -> f32 {
        (self.cell() / self.font_pt).clamp(0.5, 4.0)
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
    /// One table-grid unit. Ichitaro tables are 4 units per character cell.
    pub fn unit(&self) -> f32 {
        self.cell() / 4.0
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
        /// Gothic (sans) face instead of Mincho.
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        g: bool,
        /// Face class (see [`face_class`]): 0 Mincho, 1 Gothic, 2 rounded
        /// Gothic, 3 textbook, 4 brush, 5 pop.
        #[serde(skip_serializing_if = "is_zero")]
        f: u8,
        #[serde(skip_serializing_if = "Option::is_none")]
        color: Option<String>,
        /// Half-width characters are drawn left-aligned in a half cell.
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        pre: bool,
    },
    /// A picture of a frame (`id` = frame id), drawn under the text.
    Image {
        id: u16,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    },
    Line {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        w: f32,
        #[serde(skip_serializing_if = "Option::is_none")]
        color: Option<String>,
        /// Ichitaro line type (線種) for ruled lines: 0/1 solid, 2-3 thicker,
        /// 4-6 dashed, 7 double, 8 dotted, 9 dashed, 10-11 dot-dash,
        /// 12-14 wavy, 15 hatched, 16 light. See `line_width`.
        #[serde(skip_serializing_if = "is_plain_kind")]
        k: u8,
    },
    /// A frame line round a text box (`r` corner radius).
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        r: f32,
        lw: f32,
        color: String,
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        dot: bool,
        /// No line, only the fill.
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        noline: bool,
        /// Fill: one colour or gradient stops (0-1, colour), at `angle` degrees.
        #[serde(skip_serializing_if = "Vec::is_empty")]
        fill: Vec<(f32, String)>,
        #[serde(skip_serializing_if = "is_zero_f")]
        angle: f32,
        /// Shadow offset (pt) and colour.
        #[serde(skip_serializing_if = "Option::is_none")]
        shadow: Option<(f32, f32, String)>,
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
    /// 縦組み only: items given on the paper as it is (not turned), such as
    /// the text of a box that writes across on a 縦組み page.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub flat: Vec<Item>,
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
    /// The page limit ([`crate::limits::Limits::max_pages`]) was reached and
    /// the rest of the sheet was not laid out.
    pub truncated: bool,
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
const CLOSE_MARK: &str = "、。，．）」』】〕〉》";
const OPEN_MARK: &str = "（「『【〔〈《";
const NO_END: &str = "(（[［｛「『【〔〈《〘〖‘“";

#[derive(Clone)]
struct G {
    ch: char,
    w: f32,
    /// Shared by every glyph of a run: before 0.3.3 each glyph held its own
    /// copy of the run's style and property map (see `style::SharedProps`).
    style: Rc<CharStyle>,
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
    max_pages: usize,
    /// 段組: columns of the page, the current one, its distance from the
    /// first, and where each column's items and lines start (moved right
    /// once the layout is done).
    cols: u32,
    col: u32,
    col_step: f32,
    dx: f32,
    segs: Vec<(usize, usize, usize, f32)>,
    /// Frames placed so far that text goes round: page, column, x1, y1, x2,
    /// y2, and whether a line with no room beside it moves below it (a
    /// picture) or is set over it (a text box).
    wraps: Vec<(usize, u32, f32, f32, f32, f32, bool, u16)>,
    /// frames of the paragraph being laid out: its first line does not go
    /// round them (a handout's first line holds the title picture)
    own: Vec<u16>,
    /// Halves of the page cut across (1 or 2) and the current one
    rows: u32,
    row: u32,
    breaking: bool,
    /// Frames fixed on the page that text goes round, by the page and half
    /// they landed on in a first pass: set up when that half starts, so the
    /// lines above their anchors already go round them (一太郎ビューア: a
    /// handout's route map of small boxes keeps the text beside it clear)
    pre_frames: std::collections::HashMap<(usize, u32), Vec<u16>>,
    /// where each fixed frame landed in this pass
    landed: std::collections::HashMap<u16, (usize, u32)>,
}

impl<'a> Ctx<'a> {
    /// Top of the text in the current half. Each half is laid out like a
    /// page of half the paper's height with the page's margins and line
    /// pitch (a320: 19 lines a half, the lower half's first line 12 mm below
    /// the middle of the paper).
    fn top(&self) -> f32 {
        self.paper_top() + self.setup.top()
    }
    /// Bottom of the text in the current half
    fn bottom(&self) -> f32 {
        if self.rows <= 1 {
            self.setup.bottom()
        } else {
            self.paper_top() + self.setup.page_h() / self.rows as f32 - self.setup.margin_bottom_mm * MM
        }
    }
    /// Where frames fixed on the page are measured from: the paper's top,
    /// or the middle of the paper in the lower half
    fn paper_top(&self) -> f32 {
        self.row as f32 * self.setup.page_h() / self.rows.max(1) as f32
    }

    /// The next column, or the next half or page after the last column.
    fn new_page(&mut self) {
        if self.col + 1 < self.cols && !self.out.pages.is_empty() {
            self.col += 1;
            self.dx = self.col as f32 * self.col_step;
            let n = self.out.pages[self.page].items.len();
            self.segs.push((self.page, n, self.out.lines.len(), self.dx));
            self.y = self.top();
            return;
        }
        self.col = 0;
        self.dx = 0.0;
        if self.row + 1 < self.rows && !self.out.pages.is_empty() && self.col_was_used() {
            self.row += 1;
            self.y = self.top();
            let n = self.out.pages[self.page].items.len();
            self.segs.push((self.page, n, self.out.lines.len(), 0.0));
            self.preregister();
            return;
        }
        self.row = 0;
        self.real_new_page();
        self.segs.push((self.page, self.out.pages[self.page].items.len(), self.out.lines.len(), 0.0));
    }

    /// Set up the frames fixed on the page that the first pass found in this
    /// half, for every column (a frame can sit in a column other than its
    /// anchor's). Their own drawing still happens at their anchors.
    fn preregister(&mut self) {
        let Some(ids) = self.pre_frames.get(&(self.page, self.row)).cloned() else { return };
        let setup = self.setup;
        let mm = MM / 100.0;
        let top = self.paper_top();
        for id in ids {
            // (x, y, w, h, push, gap below): a picture keeps no gap, a box
            // across the column keeps WRAP_GAP
            let rect = if let Some(&(_, fx, fy, fw, fh)) = setup.frames.iter().find(|f| f.0 == id) {
                Some((fx as f32 * mm, top + fy as f32 * mm, fw as f32 * mm, fh as f32 * mm, true, 0.0))
            } else {
                setup.boxes.0.iter().find(|b| b.0 == id).map(|(_, fx, fy, fw, fh, _, _)| {
                    let bw = *fw as f32 * mm;
                    let full = bw >= setup.text_w() * 0.9;
                    (*fx as f32 * mm, top + *fy as f32 * mm, bw, *fh as f32 * mm, full, if full { WRAP_GAP } else { 0.0 })
                })
            };
            let Some((x, y, w, h, push, gap)) = rect else { continue };
            for c in 0..self.cols {
                let off = c as f32 * self.col_step;
                self.wraps.push((self.page, c, x - off, y, x + w - off, y + h + gap, push, id));
            }
        }
    }

    /// The widest stretch of `l..r` that no frame the text goes round covers
    /// between `y1` and `y2`, or the lowest bottom of the frames in the way
    /// when no stretch holds a few characters.
    fn free_span(&self, l: f32, r: f32, y1: f32, y2: f32, min_w: f32) -> Result<(f32, f32), f32> {
        match self.free_span_of(l, r, y1, y2, min_w, false) {
            Ok(s) => Ok(s),
            // a text box leaves no room: the text is set over it unless a
            // picture is in the way too (checked on a school handout)
            Err(_) => self.free_span_of(l, r, y1, y2, min_w, true),
        }
    }

    fn free_span_of(&self, l: f32, r: f32, y1: f32, y2: f32, min_w: f32, only_push: bool) -> Result<(f32, f32), f32> {
        let mut segs = vec![(l, r)];
        let mut low = f32::MIN;
        for &(pg, col, x1, fy1, x2, fy2, push, id) in &self.wraps {
            if self.own.contains(&id) {
                continue;
            }
            // a frame that starts in the lower part of the line leaves it
            // whole (一太郎ビューア: a handout's school name line keeps its
            // width over a picture 2 mm into it)
            let low_edge = y2 - (y2 - y1) * 0.4;
            if (only_push && !push) || pg != self.page || col != self.col || fy1 >= low_edge || fy2 <= y1 || x2 <= l || x1 >= r {
                continue;
            }
            low = low.max(fy2);
            let mut next = Vec::new();
            for (a, b) in segs {
                if x2 <= a || x1 >= b {
                    next.push((a, b));
                    continue;
                }
                if x1 > a {
                    next.push((a, x1));
                }
                if x2 < b {
                    next.push((x2, b));
                }
            }
            segs = next;
        }
        match segs.into_iter().filter(|s| s.1 - s.0 >= min_w).max_by(|a, b| (a.1 - a.0).total_cmp(&(b.1 - b.0))) {
            Some(s) => Ok(s),
            None => Err(low),
        }
    }

    /// Always true: kept apart so a page break can skip the lower half
    fn col_was_used(&self) -> bool {
        !self.breaking
    }

    /// A page break (改ページ): the next page, whatever column this is.
    fn break_page(&mut self) {
        self.col = self.cols;
        self.breaking = true;
        self.new_page();
        self.breaking = false;
    }

    /// Move each column's items and lines to their column.
    fn place_columns(&mut self) {
        if self.cols <= 1 {
            return;
        }
        let segs = std::mem::take(&mut self.segs);
        for (k, &(page, from, lfrom, dx)) in segs.iter().enumerate() {
            let next = segs.get(k + 1);
            let lto = next.map_or(self.out.lines.len(), |n| n.2);
            if dx != 0.0 {
                let items = &mut self.out.pages[page].items;
                let to = next.filter(|n| n.0 == page).map_or(items.len(), |n| n.1);
                for it in &mut items[from..to] {
                    *it = it.moved(dx, 0.0);
                }
                for l in &mut self.out.lines[lfrom..lto] {
                    l.xs.iter_mut().for_each(|x| *x += dx);
                    l.left += dx;
                    l.right += dx;
                }
            }
        }
    }

    fn real_new_page(&mut self) {
        // stop at the page limit: keep drawing onto the last page and let the
        // callers see `truncated` and stop
        if self.out.pages.len() >= self.max_pages {
            self.out.truncated = true;
            self.y = self.setup.top();
            return;
        }
        self.out.pages.push(Page {
            w: self.setup.page_w(),
            h: self.setup.page_h(),
            items: Vec::new(),
            flat: Vec::new(),
        });
        self.page = self.out.pages.len() - 1;
        self.row = 0;
        self.y = self.setup.top();
        self.preregister();
    }
    fn items(&mut self) -> &mut Vec<Item> {
        &mut self.out.pages[self.page].items
    }
}

fn glyphs(p: &Paragraph, base: f32, spread: f32, pre: Option<(usize, &str)>) -> Vec<G> {
    let mut out = Vec::new();
    let mut off = 0usize;
    let push_pre = |out: &mut Vec<G>, style: &CharStyle, txt: &str| {
        let mut st = style.clone();
        st.underline = Some(1);
        let st = Rc::new(st);
        for ch in txt.chars() {
            let size = st.size_pt.unwrap_or(base) * spread;
            out.push(G {
                ch,
                w: if is_half(ch) { size / 2.0 } else { size },
                style: Rc::clone(&st),
                off: None,
                run: usize::MAX,
                pre: true,
            });
        }
    };
    for (ri, r) in p.runs.iter().enumerate() {
        let shared = Rc::new(r.style.clone());
        for ch in r.text.chars() {
            if let Some((po, txt)) = pre {
                if po == off {
                    push_pre(&mut out, &r.style, txt);
                }
            }
            let size = r.style.size_pt.unwrap_or(base) * spread;
            let w = if ch == '\t' {
                size * 4.0
            } else if is_half(ch) {
                // 字間 goes to full-width characters only: a half-width
                // character (spaces that right-align a date line) is half the
                // character size (一太郎ビューア: 55 half spaces and 10 full
                // characters fit a 37-character line of a B4 handout)
                r.style.size_pt.unwrap_or(base) * spread.min(1.0) / 2.0
            } else {
                size
            };
            out.push(G {
                ch,
                w,
                style: Rc::clone(&shared),
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
    // 約物の連続: a closing mark right before another bracket or mark takes
    // half a cell (一太郎ビューア sets 「す。（車」 one character shorter, so a
    // handout's line ending in 「しょう）」 still fits)
    for k in 0..out.len().saturating_sub(1) {
        if CLOSE_MARK.contains(out[k].ch) && (CLOSE_MARK.contains(out[k + 1].ch) || OPEN_MARK.contains(out[k + 1].ch)) {
            out[k].w /= 2.0;
        }
    }
    out
}

/// Break glyphs into lines: the first line fits `first`, the rest `width`.
fn break_lines_2(gs: &[G], first: f32, width: f32) -> Vec<(usize, usize)> {
    if (first - width).abs() < 0.01 {
        return break_lines(gs, width);
    }
    let head = break_lines(gs, first);
    let Some(&(_, e)) = head.first() else {
        return head;
    };
    let mut out = vec![(0, e)];
    if e < gs.len() {
        out.extend(
            break_lines(&gs[e..], width)
                .into_iter()
                .map(|(a, b)| (a + e, b + e)),
        );
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

/// Lay out one paragraph inside [left, right]. `para` is the flat index;
/// `row_feed` is the line feed of the table row the paragraph is in.
#[allow(clippy::too_many_arguments)]
fn layout_para(
    ctx: &mut Ctx,
    p: &Paragraph,
    para: usize,
    left: f32,
    right: f32,
    pre: Option<&Preedit>,
    in_cell: bool,
    row_feed: Option<LineFeed>,
) -> f32 {
    let setup = ctx.setup;
    let base = setup.font_pt;
    let pitch = setup.pitch();
    let pre_here = pre.filter(|x| x.para == para).map(|x| (x.off, x.text));
    let gs = glyphs(p, base, setup.spread(), pre_here);
    // indents: the first line has its own left and right (from the margins)
    let (outer_l, outer_r) = (left, right);
    let [il, ir, fl, fr] = p.indent.map(|i| i.points(setup.cell())).unwrap_or_default();
    let first = (outer_l + fl, (outer_r - fr).max(outer_l + fl + base));
    let (left, right) = (outer_l + il, (outer_r - ir).max(outer_l + il + base));
    let width = (right - left).max(base);
    let ranges = break_lines_2(&gs, (first.1 - first.0).max(base), width);
    // Text going round frames (回り込み): when frames that it goes round are
    // on this page or anchored here, break line by line into the room left
    // beside them, and move below a frame that leaves no room.
    let wrapping = !in_cell
        && (ctx.wraps.iter().any(|w| w.0 == ctx.page) || p.frames.iter().any(|f| setup.wrap.contains(f)));
    let mut frames_done = false;
    if wrapping && !p.frames.is_empty() {
        if ctx.y + pitch > ctx.bottom() + 0.5 && ctx.y > ctx.top() + 0.5 {
            ctx.new_page();
        }
        // frames on the page now (the lines go round them); frames that sit on
        // the anchor's line once that line has found its place (a drawing on an
        // empty line moved below the title picture, ca202f)
        let y = ctx.y;
        let fixed: Vec<u16> = p.frames.iter().copied().filter(|f| setup.fixed.contains(f)).collect();
        push_frames(ctx, &fixed, y);
        frames_done = fixed.len() == p.frames.len();
    }
    let mut used = 0.0;
    let mut nlines = ranges.len();
    let mut model_pos = 0usize;
    let (mut li, mut pos) = (0usize, 0usize);
    loop {
        if ctx.out.truncated {
            break;
        }
        let (a, b, left, right) = if !wrapping {
            let Some(&(a, b)) = ranges.get(li) else { break };
            let (l, r) = if li == 0 { first } else { (left, right) };
            (a, b, l, r)
        } else {
            if li > 0 && pos >= gs.len() {
                break;
            }
            let (l0, r0) = if li == 0 { first } else { (left, right) };
            let mut lr = (l0, r0);
            // (only for a line with nothing to show: text still goes round its own picture)
            ctx.own = if li == 0 && gs.iter().all(|g| g.ch.is_whitespace()) { p.frames.clone() } else { Vec::new() };
            // an empty line (or one of spaces) is not moved round frames:
            // the lines under a handout's title pictures keep their places
            // in 一太郎ビューア (a320: the line after them lands at 84 mm)
            // ...unless it anchors a frame that sits on its line (a school
            // handout's empty line holding a drawing goes below the title
            // picture, ca202f)
            // (a line of spaces holding drawings stays: a380's choir figures)
            let floating = li == 0 && gs.is_empty() && p.frames.iter().any(|f| !setup.fixed.contains(f));
            let tries = if gs.iter().all(|g| g.ch.is_whitespace()) && !floating { 0 } else { 64 };
            for _ in 0..tries {
                match ctx.free_span(l0, r0, ctx.y, ctx.y + pitch, (base * 3.0).min(r0 - l0)) {
                    Ok(sp) => {
                        lr = sp;
                        break;
                    }
                    Err(low) => {
                        // the next line of the grid below the frame
                        let top = ctx.top();
                        let y = top + ((low - top) / pitch).ceil().max(0.0) * pitch;
                        if y + pitch > ctx.bottom() + 0.5 {
                            ctx.new_page();
                        } else {
                            ctx.y = y.max(ctx.y + 0.01);
                        }
                    }
                }
            }
            let rest = &gs[pos..];
            let e = break_lines(rest, (lr.1 - lr.0).max(base)).first().map_or(rest.len(), |r| r.1);
            let e = if rest.is_empty() { 0 } else { e.max(1) };
            let (a, b) = (pos, pos + e);
            pos = b;
            nlines = if pos >= gs.len() { li + 1 } else { li + 2 };
            (a, b, lr.0, lr.1)
        };
        let line = &gs[a..b];
        let width = (right - left).max(base);
        let max_size = line
            .iter()
            .map(|g| g.style.size_pt.unwrap_or(base))
            .fold(base, f32::max);
        let has_ruby = line
            .iter()
            .any(|g| g.run != usize::MAX && p.runs[g.run].ruby.is_some());
        // A line takes one pitch while its tallest character fits in it
        // (Ichitaro: 65 lines of 11 pt on A4 with 15 mm margins, a pitch of
        // 11.6 pt, stay one pitch each), else as many as it needs.
        // ruby half the size rides in the gap above (a B4 handout: 11 pt with
        // ruby in a 15.7 pt pitch stays one pitch)
        let need = max_size * NEED_FIT + if has_ruby { max_size * 0.4 } else { 0.0 };
        // whole line pitches that hold the tallest character (computed, not
        // counted up one pitch at a time: a tiny pitch made that loop run
        // millions of times per line)
        let h = if setup.free_lines {
            // a text box: 10.5 pt lines 13.4 pt apart and 21 pt lines 24 pt
            // apart in 一太郎ビューア
            max_size + base * 0.28 + if has_ruby { max_size * 0.5 } else { 0.0 }
        } else if pitch > 0.0 && pitch.is_finite() {
            pitch * (need / pitch).ceil().max(1.0)
        } else {
            need.max(1.0)
        };
        if !in_cell && ctx.y + h > ctx.bottom() + 0.5 && ctx.y > ctx.top() + 0.5 {
            ctx.new_page();
        }
        let top = ctx.y;
        // pictures anchored in this paragraph: from the text area's left and this line's top
        if li == 0 && !p.frames.is_empty() && !frames_done {
            let rest: Vec<u16> = if wrapping { p.frames.iter().copied().filter(|f| !setup.fixed.contains(f)).collect() } else { p.frames.clone() };
            push_frames(ctx, &rest, top);
        }
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
                let face = ctx.setup.face_of(st.font);
                let gothic = matches!(face, 1 | 2 | 5);
                ctx.items().push(Item::Text {
                    xs: xsv,
                    y: baseline,
                    s,
                    size,
                    b: st.bold == Some(true),
                    i: st.italic == Some(true),
                    g: gothic,
                    f: face,
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
                    k: 0,
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
                        let rf = ctx.setup.face_of(line[k].style.font);
                        ctx.items().push(Item::Text {
                            xs: xsr,
                            y: baseline - size * 1.0,
                            s: rt.clone(),
                            size: rs,
                            b: false,
                            i: false,
                            g: matches!(rf, 1 | 2 | 5),
                            f: rf,
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
        // 改行幅: the paragraph's own feed after each of its lines
        let adv = if p.no_line {
            0.0
        } else {
            p.feed.or(row_feed).and_then(|f| f.points(pitch)).unwrap_or(h)
        };
        ctx.y += adv;
        used += adv;
        li += 1;
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
    if width_units as f32 * unit > setup.text_w() + setup.cell() * 2.0 {
        unit = (setup.text_w() + setup.cell() * 2.0) / width_units as f32;
    }
    let xu = |u: u16| left + u as f32 * unit;
    let pad = unit * 1.5;
    let line_c = Some("#222".to_string());
    for row in &t.rows {
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
                max_pages: usize::MAX,
                cols: 1,
                col: 0,
                col_step: 0.0,
                dx: 0.0,
                segs: Vec::new(),
                wraps: Vec::new(),
                own: Vec::new(),
                rows: 1,
                row: 0,
                breaking: false,
                pre_frames: Default::default(),
                landed: Default::default(),
            };
            scratch.out.pages.push(Page {
                w: 0.0,
                h: 0.0,
                items: Vec::new(),
                flat: Vec::new(),
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
                    row.feed,
                );
            }
            heights.push(h);
            *flat_start += c.paragraphs.len();
        }
        // the row's own line feed (改行幅), else one line
        let one = row
            .feed
            .and_then(|f| f.points(setup.pitch()))
            .unwrap_or(setup.pitch());
        let row_h = heights.iter().cloned().fold(one, f32::max);
        if ctx.y + row_h > ctx.bottom() + 0.5 && ctx.y > ctx.top() + 0.5 {
            ctx.new_page();
        }
        let top = ctx.y;
        // pictures anchored on this line: from the text area's left and the line's top
        push_frames(ctx, &row.frames, top);
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
                    row.feed,
                );
            }
            *flat_start += c.paragraphs.len();
        }
        let bottom = top + row_h;
        let mid = (top + bottom) / 2.0;
        let g = row.lines();
        let line = |x1: f32, y1: f32, x2: f32, y2: f32, k: u8| Item::Line {
            x1,
            y1,
            x2,
            y2,
            w: line_width(k),
            color: if k == 16 {
                Some("#888".to_string())
            } else {
                line_c.clone()
            },
            k,
        };
        for (i, &(x, up, down)) in g.verticals.iter().enumerate() {
            let (ku, kd) = g.vertical_kinds.get(i).copied().unwrap_or_default();
            if up && down && ku != kd {
                // the two halves have their own types
                ctx.items().push(line(xu(x), top, xu(x), mid, ku));
                ctx.items().push(line(xu(x), mid, xu(x), bottom, kd));
            } else {
                let (y1, y2) = (if up { top } else { mid }, if down { bottom } else { mid });
                ctx.items()
                    .push(line(xu(x), y1, xu(x), y2, if up { ku } else { kd }));
            }
        }
        for (i, &(x1, x2)) in g.below.iter().enumerate() {
            let k = g.below_kinds.get(i).copied().unwrap_or(0);
            ctx.items().push(line(xu(x1), bottom, xu(x2), bottom, k));
        }
        for (i, &(x1, x2)) in g.middle.iter().enumerate() {
            let k = g.middle_kinds.get(i).copied().unwrap_or(0);
            ctx.items().push(line(xu(x1), mid, xu(x2), mid, k));
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
    layout_with(blocks, setup, show_marks, pre, crate::limits::Limits::default().max_pages)
}

/// [`layout`] with a page limit. Past it the layout stops and
/// [`Layout::truncated`] is set.
pub fn layout_with(
    blocks: &[Block],
    setup: &PageSetup,
    show_marks: bool,
    pre: Option<&Preedit>,
    max_pages: usize,
) -> Layout {
    let full = setup.sane();
    // 段組: lay out in the first column, each column as wide as Ichitaro
    // makes it (the text width less the gaps, shared out), then move the
    // later columns over
    // 縦組み: the layout page is the paper turned, so columns across it are
    // the 段 stacked top and bottom on the paper (a162 ともだち通信: two 段)
    let cols = full.columns.clamp(1, 10);
    let gap = full.col_gap_mm.clamp(0.0, 100.0) * MM;
    let col_w = (full.text_w() - gap * (cols - 1) as f32) / cols as f32;
    let (cols, col_w) = if col_w > full.font_pt * 2.0 { (cols, col_w) } else { (1, full.text_w()) };
    let mut one = full.clone();
    if cols > 1 {
        // In columns a character takes its own cell, 1.031 of its size (4.0 mm
        // for 11 pt in 一太郎ビューア on six B4 and A4 handouts), and a line
        // holds 字数 cells from the column's left: a narrower 字数 leaves the
        // rest of the column empty, and right-aligned lines end there. A
        // 字数 meant for the whole width (a320: 83) is fitted to the column
        // (41 there).
        let cell = full.font_pt * COL_CELL;
        let fit = (col_w / cell).round().max(4.0) as u32;
        let line_w = if one.chars_per_line <= fit {
            (one.chars_per_line as f32 * cell).min(col_w)
        } else {
            one.chars_per_line = fit;
            col_w
        };
        one.margin_right_mm = (full.page_w() - full.left() - line_w) / MM;
    }
    let setup = &one;
    // two passes when frames fixed on the page wrap text: the first finds
    // the half each lands on, the second sets them up as that half starts
    let (out, landed) = layout_pass(blocks, setup, &full, show_marks, pre, max_pages, cols, col_w, gap, Default::default());
    // (縦組み turns frames on the page: not set up ahead there yet)
    // (a long document with such frames lays out once: typing stays quick)
    if landed.is_empty() || full.vertical || blocks.len() > 4000 {
        return out;
    }
    let mut pre_frames: std::collections::HashMap<(usize, u32), Vec<u16>> = Default::default();
    for (id, at) in landed {
        pre_frames.entry(at).or_default().push(id);
    }
    layout_pass(blocks, setup, &full, show_marks, pre, max_pages, cols, col_w, gap, pre_frames).0
}

#[allow(clippy::too_many_arguments)]
fn layout_pass(
    blocks: &[Block],
    setup: &PageSetup,
    full: &PageSetup,
    show_marks: bool,
    pre: Option<&Preedit>,
    max_pages: usize,
    cols: u32,
    col_w: f32,
    gap: f32,
    pre_frames: std::collections::HashMap<(usize, u32), Vec<u16>>,
) -> (Layout, std::collections::HashMap<u16, (usize, u32)>) {
    let mut ctx = Ctx {
        setup,
        show_marks,
        out: Layout::default(),
        page: 0,
        y: 0.0,
        max_pages: max_pages.max(1),
        cols,
        col: 0,
        col_step: col_w + gap,
        dx: 0.0,
        segs: Vec::new(),
        wraps: Vec::new(),
        own: Vec::new(),
        rows: if cols > 1 && !full.vertical { full.rows.clamp(1, 2) } else { 1 },
        row: 0,
        breaking: false,
        pre_frames,
        landed: Default::default(),
    };
    ctx.new_page();
    let mut flat = 0usize;
    for b in blocks {
        // stop at the page limit, and at an element limit for pages that do
        // not break (a huge table cell): drawing millions of items is the
        // same problem as millions of pages
        if !ctx.out.truncated && ctx.out.lines.len() > MAX_LINES {
            ctx.out.truncated = true;
        }
        if ctx.out.truncated {
            break;
        }
        match b {
            Block::Paragraph(p) => {
                if p.page_break_before && ctx.y > ctx.top() + 0.5 {
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
                    ctx.break_page();
                }
                layout_para(
                    &mut ctx,
                    p,
                    flat,
                    setup.left(),
                    setup.left() + setup.text_w(),
                    pre,
                    false,
                    None,
                );
                flat += 1;
            }
            Block::Table(t) => layout_table(&mut ctx, t, &mut flat, pre),
        }
    }
    ctx.place_columns();
    (ctx.out, ctx.landed)
}

/// Is this face a Gothic (sans) face? Names as Ichitaro and Windows write them.
pub fn is_gothic(name: &str) -> bool {
    let n = name.to_lowercase();
    ["ゴシック", "ｺﾞｼｯｸ", "gothic", "丸ゴ", "メイリオ", "meiryo", "游ゴ", "ui gothic", "arial", "helvetica", "sans", "ヒラギノ角", "角ゴ"]
        .iter()
        .any(|k| n.contains(k))
}

fn is_zero_f(v: &f32) -> bool {
    *v == 0.0
}

fn is_zero(v: &u8) -> bool {
    *v == 0
}

impl PageSetup {
    /// The face class of text set in `/Font` index `font` (`None` or an
    /// index past the table: the document's standard face).
    pub fn face_of(&self, font: Option<u16>) -> u8 {
        font.and_then(|f| self.faces.get(f as usize).copied()).unwrap_or(self.base_face)
    }

    /// Fill the font fields from the document's font table.
    pub fn fill_fonts(&mut self, fonts: &[crate::doc::FontInfo]) {
        self.faces = fonts.iter().map(|f| face_class(&f.name)).collect();
        self.font_names = fonts.iter().map(|f| f.name.clone()).collect();
        self.base_font = base_font(fonts);
        self.base_face = self.base_font.map_or(0, |i| self.faces[i as usize]);
        self.gothic = (0..fonts.len() as u16).filter(|&i| matches!(self.faces[i as usize], 1 | 2 | 5)).collect();
    }
}

/// Symbol faces Ichitaro adds for its own marks (JustUnitMark and others).
pub fn is_mark_font(name: &str) -> bool {
    name.starts_with("Just") && name.contains("Mark")
}

/// The face of text without a font of its own: the first Japanese
/// (Shift_JIS) face of the table that is not a mark face. Checked with
/// 一太郎ビューア by swapping the face: a table that starts with
/// JustUnitMark and Times New Roman sets its plain text in the third face.
pub fn base_font(fonts: &[crate::doc::FontInfo]) -> Option<u16> {
    fonts
        .iter()
        .position(|f| f.charset == 0x80 && !is_mark_font(&f.name))
        .or_else(|| fonts.iter().position(|f| !is_mark_font(&f.name)))
        .map(|i| i as u16)
}

/// Face class of a face name: 0 Mincho (and serif), 1 Gothic (and sans),
/// 2 rounded Gothic (丸ゴシック), 3 textbook (教科書体), 4 brush and
/// handwriting (楷書, 行書, 毛筆, 勘亭流), 5 pop (POP体).
pub fn face_class(name: &str) -> u8 {
    let n = name.to_lowercase();
    let has = |k: &[&str]| k.iter().any(|k| n.contains(k));
    if has(&["pop", "ﾎﾟｯﾌﾟ", "ポップ"]) {
        5
    } else if has(&["丸ｺﾞ", "丸ゴ", "丸ｺ\u{ff9e}", "まるご", "maru", "rounded"]) {
        2
    } else if has(&["教科書", "kyokasho"]) {
        3
    } else if has(&["楷書", "行書", "毛筆", "勘亭", "隷書", "草書", "正楷", "手書", "ペン字", "古印", "宋朝"]) {
        4
    } else if is_gothic(name) {
        1
    } else {
        0
    }
}

/// Laid-out text boxes shared between copies of a setup (see
/// [`PageSetup::boxes`]).
#[derive(Debug, Clone, Default)]
pub struct Boxes(pub std::sync::Arc<Vec<(u16, i32, i32, u32, u32, Vec<Item>, bool)>>);

impl PartialEq for Boxes {
    fn eq(&self, o: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.0, &o.0)
    }
}

/// Pictures and text boxes anchored on a line whose top is `top`: from the
/// text area's left and the line's top. Pictures go under the text (the
/// editor draws images first), box text with the rest.
fn push_frames(ctx: &mut Ctx, ids: &[u16], top: f32) {
    let setup = ctx.setup;
    let (tl, mm) = (setup.left(), MM / 100.0);
    for id in ids {
        // a frame on the page is measured from the paper's top left (checked
        // with 一太郎ビューア on a handout with A4 margins of 20 and 17 mm), and
        // stays where it is whatever column anchors it
        let fixed = setup.fixed.contains(id);
        if fixed && setup.wrap.contains(id) {
            ctx.landed.insert(*id, (ctx.page, ctx.row));
        }
        // set up already when this half started (see `Ctx::preregister`)
        let ready = fixed && ctx.pre_frames.get(&(ctx.page, ctx.row)).is_some_and(|v| v.contains(id));
        let top = if fixed { ctx.paper_top() } else { top };
        let tl = if fixed { -ctx.dx } else { tl };
        // 縦組み: a frame on the page is given on the paper as it is; the
        // layout page is the paper turned a quarter turn
        // (one floating in the text is measured from the text area's top
        // left on the paper: the 学校だより scroll of a 縦組み handout). The
        // column's shift along the layout page is taken back, as the
        // column's items are moved by it afterwards
        let vert = setup.vertical;
        let (px0, py0) = if fixed { (0.0, 0.0) } else { (setup.margin_bottom_mm * MM, setup.margin_left_mm * MM) };
        let dx = ctx.dx;
        let turn = |x: f32, y: f32, w: f32, h: f32| {
            if vert {
                (y - dx, setup.page_h() - (x + w), h, w)
            } else {
                (x, y, w, h)
            }
        };
        let (tl, top) = if vert { (px0, py0) } else { (tl, top) };
        if let Some(&(_, fx, fy, fw, fh)) = setup.frames.iter().find(|f| f.0 == *id) {
            let (x, y, w, h) = turn(tl + fx as f32 * mm, top + fy as f32 * mm, fw as f32 * mm, fh as f32 * mm);
            ctx.items().push(Item::Image { id: *id, x, y, w, h });
            if setup.wrap.contains(id) && !ready {
                ctx.wraps.push((ctx.page, ctx.col, x, y, x + w, y + h, true, *id));
            }
        }
        if let Some((_, fx, fy, fw, fh, items, flat)) = setup.boxes.0.iter().find(|b| b.0 == *id) {
            // 縦組み: a fixed box is placed on the paper and turned like a
            // picture; its text was laid out for that (see `box_items`)
            let (dx, dy, bw, bh) = turn(tl + *fx as f32 * mm, top + *fy as f32 * mm, *fw as f32 * mm, *fh as f32 * mm);
            if *flat {
                // a box writing across on a 縦組み page: its items are on the
                // paper, from the box's top left there
                let (px, py) = (setup.page_h() - (dy + bh), dx + ctx.dx);
                for it in items {
                    let m = it.moved(px, py);
                    ctx.out.pages[ctx.page].flat.push(m);
                }
            } else {
                for it in items {
                    ctx.items().push(it.moved(dx, dy));
                }
            }
            if setup.wrap.contains(id) && !ready {
                // a box across the whole column moves the text below it (the
                // 持ち物 box of a B4 handout); a narrower one leaves the
                // text over it where no room is beside it
                let full = bw >= (setup.text_w() * 0.9).min(ctx.col_step.max(setup.text_w()));
                ctx.wraps.push((ctx.page, ctx.col, dx, dy, dx + bw, dy + bh + if full { WRAP_GAP } else { 0.0 }, full, *id));
            }
        }
    }
}

impl Item {
    /// The item moved by `(dx, dy)`.
    pub fn moved(&self, dx: f32, dy: f32) -> Item {
        let mut it = self.clone();
        match &mut it {
            Item::Text { xs, y, .. } => {
                xs.iter_mut().for_each(|x| *x += dx);
                *y += dy;
            }
            Item::Image { x, y, .. } | Item::Mark { x, y, .. } | Item::Rect { x, y, .. } => {
                *x += dx;
                *y += dy;
            }
            Item::Line { x1, y1, x2, y2, .. } => {
                *x1 += dx;
                *x2 += dx;
                *y1 += dy;
                *y2 += dy;
            }
        }
        it
    }
}

/// Lay out the text of each text box (frame of kind 2) inside the box, once:
/// the box's own margins, the document's character size and faces, one
/// character per cell and a line pitch of 1.25 characters (the box keeps as
/// many whole lines as fit, at least one). Text past the box is cut off.
pub fn box_items(doc: &crate::doc::Document, setup: &PageSetup) -> Vec<(u16, i32, i32, u32, u32, Vec<Item>, bool)> {
    let mut out = Vec::new();
    // the k-th text-box frame of /Frame shows the k-th box of /LayoutBoxText
    // (checked on a school handout: sizes and texts agree box by box)
    for (k, f) in doc.frames.iter().filter(|f| f.kind == 2).enumerate() {
        let Some(b) = doc.boxes.get(k) else { continue };
        if (b.blocks.is_empty() && !doc.pictures.iter().any(|p| p.layout == Some(b.layout))) || f.w == 0 || f.h == 0 {
            continue;
        }
        let font = b.size_pt.unwrap_or(setup.font_pt).clamp(4.0, 100.0);
        // 縦組み page: a box is placed turned on the paper. A box writing
        // top to bottom is laid out in layout terms (its height on the paper
        // is the line length); one writing across is laid out on the paper
        // as it is and drawn unturned (`flat`)
        let turned = setup.vertical;
        let flat = turned && !b.vertical;
        let [pl, pr, pt, pb] = b.pad.map(|v| v as f32 / 100.0);
        let (bw, bh) = (f.w as f32 / 100.0, f.h as f32 / 100.0);
        // paper (left, right, top, bottom) → layout (top, bottom, right, left)
        let (pl, pr, pt, pb, bw, bh) = if turned && !flat { (pt, pb, pr, pl, bh, bw) } else { (pl, pr, pt, pb, bw, bh) };
        let inner_w = (bw - pl - pr).max(1.0) * MM;
        let inner_h = (bh - pt - pb).max(1.0) * MM;
        // line pitch 1.25 characters (a handout's box: seven lines of
        // 10.5 pt in 33 mm, and two lines of 21 pt and one of 15 pt in 23 mm)
        let pitch = font * 1.25;
        let lines = ((inner_h / pitch).floor() as u32).max(1);
        // room below the box too, so that anchors on later lines still place
        // their pictures; text past the box is cut off below
        let n = (lines + 200).min(((1400.0 * MM - pt * MM) / pitch).floor().max(2.0) as u32).max(2);
        let width = bw.max(60.0);
        let height = (pt + pitch * n as f32 / MM + 1.0).max(60.0);
        let sub = PageSetup {
            width_mm: width,
            height_mm: height,
            margin_left_mm: pl,
            margin_right_mm: width - bw + pr,
            margin_top_mm: pt,
            margin_bottom_mm: height - pt - pitch * n as f32 / MM,
            chars_per_line: ((inner_w / font).floor() as u32).max(1),
            lines_per_page: n,
            font_pt: font,
            gothic: setup.gothic.clone(),
            faces: setup.faces.clone(),
            base_face: setup.base_face,
            font_names: setup.font_names.clone(),
            base_font: setup.base_font,
            // pictures and figures anchored in the box's own text (children
            // drawn in an empty box next to a heading)
            frames: setup.frames.clone(),
            free_lines: true,
            ..PageSetup::default()
        };
        let l = layout_with(&b.blocks, &sub, false, None, 1);
        let Some(page) = l.pages.first() else { continue };
        // text past the box's bottom is not shown
        let max_y = bh * MM;
        let mut items: Vec<Item> = Vec::new();
        // figures kept with this box's layout and not anchored in its text:
        // placed from the box's top left
        let anchored: Vec<u16> = b.blocks.iter().flat_map(|bl| match bl {
            crate::doc::Block::Paragraph(p) => p.frames.clone(),
            crate::doc::Block::Table(t) => t.rows.iter().flat_map(|r| r.frames.clone()).collect(),
        }).collect();
        for pic in doc.pictures.iter().filter(|p| p.layout == Some(b.layout) && !anchored.contains(&p.item)) {
            if let Some(g) = doc.frames.iter().find(|g| g.id == pic.item) {
                let mm = MM / 100.0;
                items.push(Item::Image { id: g.id, x: g.x as f32 * mm, y: g.y as f32 * mm, w: g.w as f32 * mm, h: g.h as f32 * mm });
            }
        }
        if let Some(bd) = &b.border {
            items.push(Item::Rect {
                x: 0.0,
                y: 0.0,
                w: bw * MM,
                h: bh * MM,
                r: (bd.radius_mm * MM).min(bw.min(bh) * MM / 2.0),
                lw: bd.width_mm * MM,
                color: bd.color.clone(),
                dot: bd.dotted,
                noline: !bd.line,
                fill: bd.fill.clone(),
                angle: bd.angle,
                shadow: bd.shadow.as_ref().map(|(x, y, c)| (x * MM, y * MM, c.clone())),
            });
        }
        items.extend(page
            .items
            .iter()
            .filter(|it| match it {
                Item::Text { y, .. } => *y <= max_y,
                Item::Mark { .. } => false,
                _ => true,
            })
            .cloned());
        out.push((f.id, f.x, f.y, f.w, f.h, items, flat));
    }
    out
}
