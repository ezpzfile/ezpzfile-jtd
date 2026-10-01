//! Editing: caret, selection, text input, formatting, tables, undo.
//!
//! The document model is the single source of truth. The UI sends commands
//! and draws whatever [`crate::layout`] produces; it never edits text itself.

use crate::doc::{Align, Block, Cell, Document, Paragraph, Row, Run, Table};
use crate::layout::{self, Layout, PageSetup, Preedit};
use crate::style::CharStyle;
use serde::Serialize;

/// Where a paragraph lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PLoc {
    /// Top-level paragraph: block index.
    Top(usize),
    /// Paragraph in a table cell: (block, row, cell, paragraph).
    Cell(usize, usize, usize, usize),
}

/// A caret position: index into the flat paragraph list + character offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Default, Serialize)]
pub struct Pos {
    pub p: usize,
    pub off: usize,
}

// ---------------------------------------------------------------- paragraphs

pub fn plen(p: &Paragraph) -> usize {
    p.runs.iter().map(|r| r.text.chars().count()).sum()
}

fn byte_at(s: &str, ch: usize) -> usize {
    s.char_indices().nth(ch).map(|(b, _)| b).unwrap_or(s.len())
}

/// Make sure a run boundary exists at `off`; return the index of the run
/// that starts there (may be `runs.len()`).
fn split_at(p: &mut Paragraph, off: usize) -> usize {
    let mut acc = 0usize;
    for i in 0..p.runs.len() {
        let n = p.runs[i].text.chars().count();
        if off == acc {
            return i;
        }
        if off < acc + n {
            let b = byte_at(&p.runs[i].text, off - acc);
            let tail = p.runs[i].text.split_off(b);
            let mut r2 = p.runs[i].clone();
            r2.text = tail;
            r2.ruby = None; // keep the reading on the first half only
            p.runs.insert(i + 1, r2);
            return i + 1;
        }
        acc += n;
    }
    p.runs.len()
}

fn normalize(p: &mut Paragraph) {
    p.runs.retain(|r| !r.text.is_empty());
    let mut i = 0;
    while i + 1 < p.runs.len() {
        let (a, b) = (&p.runs[i], &p.runs[i + 1]);
        if a.ruby.is_none() && b.ruby.is_none() && !a.inline && !b.inline && a.style == b.style {
            let t = p.runs.remove(i + 1).text;
            p.runs[i].text.push_str(&t);
        } else {
            i += 1;
        }
    }
}

/// Style for text typed at `off`: the character before, else after.
fn style_at(p: &Paragraph, off: usize) -> CharStyle {
    let mut acc = 0usize;
    let mut last = None;
    for r in &p.runs {
        let n = r.text.chars().count();
        if off > acc && off <= acc + n {
            return r.style.clone();
        }
        if off == 0 && n > 0 {
            return r.style.clone();
        }
        acc += n;
        last = Some(&r.style);
    }
    last.cloned().unwrap_or_default()
}

fn insert_str(p: &mut Paragraph, off: usize, text: &str, style: &CharStyle) {
    if text.is_empty() {
        return;
    }
    // typing inside a ruby or inline run extends that run
    let mut acc = 0usize;
    for r in p.runs.iter_mut() {
        let n = r.text.chars().count();
        if (r.ruby.is_some() || r.inline) && off > acc && off < acc + n {
            let b = byte_at(&r.text, off - acc);
            r.text.insert_str(b, text);
            return;
        }
        acc += n;
    }
    let i = split_at(p, off);
    p.runs.insert(
        i,
        Run {
            text: text.to_string(),
            style: style.clone(),
            ruby: None,
            inline: false,
        },
    );
    normalize(p);
}

fn delete_range(p: &mut Paragraph, a: usize, b: usize) {
    if a >= b {
        return;
    }
    let i = split_at(p, a);
    let j = split_at(p, b);
    p.runs.drain(i..j);
    normalize(p);
}

/// Copy of characters [a, b).
fn slice(p: &Paragraph, a: usize, b: usize) -> Paragraph {
    let mut q = p.clone();
    q.page_break_before = false;
    let n = plen(&q);
    delete_range(&mut q, b.min(n), n);
    delete_range(&mut q, 0, a.min(b));
    q
}

fn append(dst: &mut Paragraph, src: Paragraph) {
    dst.runs.extend(src.runs);
    normalize(dst);
}

fn para_text(p: &Paragraph) -> String {
    p.plain_text()
}

fn for_each_run_in(p: &mut Paragraph, a: usize, b: usize, mut f: impl FnMut(&mut Run)) {
    if a >= b {
        return;
    }
    let i = split_at(p, a);
    let j = split_at(p, b);
    for r in &mut p.runs[i..j] {
        f(r);
    }
    normalize(p);
}

// ---------------------------------------------------------------- blocks

pub fn flatten(blocks: &[Block]) -> Vec<PLoc> {
    let mut v = Vec::new();
    for (bi, b) in blocks.iter().enumerate() {
        match b {
            Block::Paragraph(_) => v.push(PLoc::Top(bi)),
            Block::Table(t) => {
                for (ri, r) in t.rows.iter().enumerate() {
                    for (ci, c) in r.cells.iter().enumerate() {
                        for pi in 0..c.paragraphs.len() {
                            v.push(PLoc::Cell(bi, ri, ci, pi));
                        }
                    }
                }
            }
        }
    }
    v
}

fn para_ref(blocks: &[Block], loc: PLoc) -> &Paragraph {
    match loc {
        PLoc::Top(b) => match &blocks[b] {
            Block::Paragraph(p) => p,
            _ => unreachable!("top loc on a table"),
        },
        PLoc::Cell(b, r, c, p) => match &blocks[b] {
            Block::Table(t) => &t.rows[r].cells[c].paragraphs[p],
            _ => unreachable!("cell loc on a paragraph"),
        },
    }
}

fn para_mut(blocks: &mut [Block], loc: PLoc) -> &mut Paragraph {
    match loc {
        PLoc::Top(b) => match &mut blocks[b] {
            Block::Paragraph(p) => p,
            _ => unreachable!("top loc on a table"),
        },
        PLoc::Cell(b, r, c, p) => match &mut blocks[b] {
            Block::Table(t) => &mut t.rows[r].cells[c].paragraphs[p],
            _ => unreachable!("cell loc on a paragraph"),
        },
    }
}

fn cell_mut(blocks: &mut [Block], b: usize, r: usize, c: usize) -> &mut Cell {
    match &mut blocks[b] {
        Block::Table(t) => &mut t.rows[r].cells[c],
        _ => unreachable!(),
    }
}

/// Editable invariants: at least one block, cells hold a paragraph, a
/// paragraph after every table so the caret can leave it.
pub fn normalize_blocks(blocks: &mut Vec<Block>) {
    for b in blocks.iter_mut() {
        if let Block::Table(t) = b {
            for r in &mut t.rows {
                for c in &mut r.cells {
                    if c.paragraphs.is_empty() {
                        c.paragraphs.push(Paragraph::default());
                    }
                }
            }
        }
    }
    let mut i = 0;
    while i < blocks.len() {
        if matches!(blocks[i], Block::Table(_))
            && !matches!(blocks.get(i + 1), Some(Block::Paragraph(_)))
        {
            blocks.insert(i + 1, Block::Paragraph(Paragraph::default()));
        }
        i += 1;
    }
    if blocks.is_empty() {
        blocks.push(Block::Paragraph(Paragraph::default()));
    }
}

// ---------------------------------------------------------------- editor

#[derive(Clone)]
struct Snapshot {
    sheet: usize,
    blocks: Vec<Block>,
    caret: Pos,
    anchor: Pos,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    None,
    Typing,
    Deleting,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Move {
    Left,
    Right,
    Up,
    Down,
    LineStart,
    LineEnd,
    DocStart,
    DocEnd,
    PageUp,
    PageDown,
    WordLeft,
    WordRight,
}

#[derive(Serialize, Debug, Clone)]
pub struct Rect {
    pub page: usize,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Serialize, Debug, Clone)]
pub struct Status {
    pub page: usize,
    pub pages: usize,
    /// Line on the page's grid (行), 1-based.
    pub line: usize,
    /// Character column (字), 1-based, in full-width cells.
    pub col: usize,
    pub chars: usize,
    pub overwrite: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub modified: bool,
    pub in_table: bool,
    pub has_selection: bool,
    pub sheet: usize,
    pub sheets: Vec<String>,
}

#[derive(Serialize, Debug, Clone)]
pub struct CaretStyle {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub size: f32,
    pub color: Option<String>,
    pub align: Align,
}

pub const SIZES: [f32; 22] = [
    6.0, 7.0, 8.0, 9.0, 10.0, 10.5, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 22.0, 24.0, 28.0, 32.0,
    36.0, 40.0, 48.0, 56.0, 64.0, 72.0,
];
const UNDO_LIMIT: usize = 100;

pub struct Editor {
    pub doc: Document,
    pub sheet: usize,
    flat: Vec<PLoc>,
    pub caret: Pos,
    pub anchor: Pos,
    upstream: bool,
    goal_x: Option<f32>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    kind: Kind,
    pending: Option<CharStyle>,
    pub overwrite: bool,
    preedit: String,
    pub setup: PageSetup,
    pub show_marks: bool,
    layout: Option<Layout>,
    pub modified: bool,
}

impl Editor {
    pub fn new(mut doc: Document) -> Editor {
        for s in &mut doc.sheets {
            normalize_blocks(&mut s.blocks);
        }
        if doc.sheets.is_empty() {
            doc = Document::blank();
        }
        let flat = flatten(&doc.sheets[0].blocks);
        Editor {
            doc,
            sheet: 0,
            flat,
            caret: Pos::default(),
            anchor: Pos::default(),
            upstream: false,
            goal_x: None,
            undo: Vec::new(),
            redo: Vec::new(),
            kind: Kind::None,
            pending: None,
            overwrite: false,
            preedit: String::new(),
            setup: PageSetup::default(),
            show_marks: true,
            layout: None,
            modified: false,
        }
    }

    pub fn blank() -> Editor {
        Editor::new(Document::blank())
    }

    fn blocks(&self) -> &Vec<Block> {
        &self.doc.sheets[self.sheet].blocks
    }
    fn blocks_mut(&mut self) -> &mut Vec<Block> {
        &mut self.doc.sheets[self.sheet].blocks
    }
    fn para(&self, i: usize) -> &Paragraph {
        para_ref(self.blocks(), self.flat[i])
    }
    fn para_mut_at(&mut self, i: usize) -> &mut Paragraph {
        let loc = self.flat[i];
        para_mut(self.blocks_mut(), loc)
    }
    pub fn para_count(&self) -> usize {
        self.flat.len()
    }

    fn changed(&mut self) {
        self.layout = None;
        self.modified = true;
        self.flat = flatten(self.blocks());
        self.clamp();
    }

    fn clamp(&mut self) {
        let n = self.flat.len();
        for pos in [&mut self.caret, &mut self.anchor] {
            if pos.p >= n {
                pos.p = n.saturating_sub(1);
            }
        }
        let lc = plen(self.para(self.caret.p));
        let la = plen(self.para(self.anchor.p));
        self.caret.off = self.caret.off.min(lc);
        self.anchor.off = self.anchor.off.min(la);
    }

    fn snapshot(&mut self, kind: Kind) {
        if kind != Kind::None && kind == self.kind {
            return; // coalesce consecutive typing / deleting
        }
        self.undo.push(Snapshot {
            sheet: self.sheet,
            blocks: self.blocks().clone(),
            caret: self.caret,
            anchor: self.anchor,
        });
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.kind = kind;
    }

    fn restore(&mut self, s: Snapshot) {
        self.sheet = s.sheet;
        self.doc.sheets[self.sheet].blocks = s.blocks;
        self.caret = s.caret;
        self.anchor = s.anchor;
        self.changed();
    }

    pub fn undo(&mut self) -> bool {
        let Some(s) = self.undo.pop() else {
            return false;
        };
        let cur = Snapshot {
            sheet: self.sheet,
            blocks: self.blocks().clone(),
            caret: self.caret,
            anchor: self.anchor,
        };
        self.redo.push(cur);
        self.restore(s);
        self.kind = Kind::None;
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(s) = self.redo.pop() else {
            return false;
        };
        let cur = Snapshot {
            sheet: self.sheet,
            blocks: self.blocks().clone(),
            caret: self.caret,
            anchor: self.anchor,
        };
        self.undo.push(cur);
        self.restore(s);
        self.kind = Kind::None;
        true
    }

    pub fn set_sheet(&mut self, i: usize) {
        if i < self.doc.sheets.len() && i != self.sheet {
            self.sheet = i;
            self.caret = Pos::default();
            self.anchor = Pos::default();
            self.flat = flatten(self.blocks());
            self.layout = None;
            self.kind = Kind::None;
        }
    }

    // ------------------------------------------------------------ selection

    pub fn has_selection(&self) -> bool {
        self.caret != self.anchor
    }

    fn ordered(&self) -> (Pos, Pos) {
        if self.anchor <= self.caret {
            (self.anchor, self.caret)
        } else {
            (self.caret, self.anchor)
        }
    }

    pub fn select_all(&mut self) {
        self.anchor = Pos { p: 0, off: 0 };
        let last = self.flat.len() - 1;
        self.caret = Pos {
            p: last,
            off: plen(self.para(last)),
        };
        self.kind = Kind::None;
    }

    pub fn selected_text(&self) -> String {
        let (a, b) = self.ordered();
        let mut out = String::new();
        for i in a.p..=b.p {
            let p = self.para(i);
            let n = plen(p);
            let s = if i == a.p { a.off } else { 0 };
            let e = if i == b.p { b.off } else { n };
            let t: String = para_text(p)
                .chars()
                .skip(s)
                .take(e.saturating_sub(s))
                .collect();
            out.push_str(&t);
            if i < b.p {
                let same_cell = match (self.flat[i], self.flat[i + 1]) {
                    (PLoc::Cell(b1, r1, c1, _), PLoc::Cell(b2, r2, c2, _)) => {
                        (b1, r1, c1) == (b2, r2, c2)
                    }
                    _ => true,
                };
                let same_row = match (self.flat[i], self.flat[i + 1]) {
                    (PLoc::Cell(b1, r1, _, _), PLoc::Cell(b2, r2, _, _)) => (b1, r1) == (b2, r2),
                    _ => false,
                };
                out.push(if !same_cell && same_row { '\t' } else { '\n' });
            }
        }
        out
    }

    /// Delete the selection. Returns true if anything was selected.
    fn delete_selection(&mut self) -> bool {
        let (a, b) = self.ordered();
        if a == b {
            return false;
        }
        if a.p == b.p {
            let p = self.para_mut_at(a.p);
            delete_range(p, a.off, b.off);
        } else {
            let (la, lb) = (self.flat[a.p], self.flat[b.p]);
            match (la, lb) {
                (PLoc::Top(ba), PLoc::Top(bb)) => {
                    let tail = {
                        let pb = para_ref(self.blocks(), lb);
                        slice(pb, b.off, plen(pb))
                    };
                    let pa = para_mut(self.blocks_mut(), la);
                    let n = plen(pa);
                    delete_range(pa, a.off, n);
                    append(pa, tail);
                    self.blocks_mut().drain(ba + 1..=bb);
                }
                (PLoc::Cell(t1, r1, c1, pa), PLoc::Cell(t2, r2, c2, pb))
                    if (t1, r1, c1) == (t2, r2, c2) =>
                {
                    let tail = {
                        let q = para_ref(self.blocks(), lb);
                        slice(q, b.off, plen(q))
                    };
                    let cell = cell_mut(self.blocks_mut(), t1, r1, c1);
                    let first = &mut cell.paragraphs[pa];
                    let n = plen(first);
                    delete_range(first, a.off, n);
                    append(first, tail);
                    cell.paragraphs.drain(pa + 1..=pb);
                }
                _ => {
                    // Across containers: clear text only, keep structure.
                    for i in a.p..=b.p {
                        let n = plen(self.para(i));
                        let s = if i == a.p { a.off } else { 0 };
                        let e = if i == b.p { b.off } else { n };
                        let p = self.para_mut_at(i);
                        delete_range(p, s, e);
                    }
                }
            }
        }
        normalize_blocks(self.blocks_mut());
        self.caret = a;
        self.anchor = a;
        self.changed();
        true
    }

    // ------------------------------------------------------------ typing

    pub fn insert_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let single = text.chars().count() == 1 && !text.contains('\n');
        self.snapshot(if single && !self.has_selection() {
            Kind::Typing
        } else {
            Kind::None
        });
        self.delete_selection();
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        for (k, seg) in text.split('\n').enumerate() {
            if k > 0 {
                self.split_paragraph(false);
            }
            if seg.is_empty() {
                continue;
            }
            let pos = self.caret;
            let style = self
                .pending
                .clone()
                .unwrap_or_else(|| style_at(self.para(pos.p), pos.off));
            let n = seg.chars().count();
            let overwrite = self.overwrite;
            let p = self.para_mut_at(pos.p);
            if overwrite {
                let len = plen(p);
                delete_range(p, pos.off, (pos.off + n).min(len));
            }
            insert_str(p, pos.off, seg, &style);
            self.caret.off += n;
        }
        self.anchor = self.caret;
        self.upstream = false;
        self.goal_x = None;
        self.changed();
    }

    fn split_paragraph(&mut self, page_break: bool) {
        let pos = self.caret;
        let loc = self.flat[pos.p];
        let tail = {
            let p = self.para(pos.p);
            let mut t = slice(p, pos.off, plen(p));
            t.align = p.align;
            t.page_break_before = page_break;
            t
        };
        {
            let p = self.para_mut_at(pos.p);
            let n = plen(p);
            delete_range(p, pos.off, n);
        }
        match loc {
            PLoc::Top(b) => self.blocks_mut().insert(b + 1, Block::Paragraph(tail)),
            PLoc::Cell(b, r, c, pi) => cell_mut(self.blocks_mut(), b, r, c)
                .paragraphs
                .insert(pi + 1, tail),
        }
        self.flat = flatten(self.blocks());
        self.caret = Pos {
            p: pos.p + 1,
            off: 0,
        };
        self.anchor = self.caret;
        self.layout = None;
        self.modified = true;
    }

    pub fn enter(&mut self) {
        self.snapshot(Kind::None);
        self.delete_selection();
        self.split_paragraph(false);
        self.changed();
    }

    /// 改ページ (Ctrl+Y): start a new page at the caret.
    pub fn page_break(&mut self) {
        if !matches!(self.flat[self.caret.p], PLoc::Top(_)) {
            return;
        }
        self.snapshot(Kind::None);
        self.delete_selection();
        if self.caret.off == 0 {
            self.para_mut_at(self.caret.p).page_break_before = true;
        } else {
            self.split_paragraph(true);
        }
        self.changed();
    }

    pub fn backspace(&mut self) {
        if self.has_selection() {
            self.snapshot(Kind::None);
            self.delete_selection();
            return;
        }
        let pos = self.caret;
        if pos.off > 0 {
            self.snapshot(Kind::Deleting);
            let p = self.para_mut_at(pos.p);
            delete_range(p, pos.off - 1, pos.off);
            self.caret.off -= 1;
            self.anchor = self.caret;
            self.changed();
            return;
        }
        if self.para(pos.p).page_break_before {
            self.snapshot(Kind::None);
            self.para_mut_at(pos.p).page_break_before = false;
            self.changed();
            return;
        }
        if pos.p == 0 {
            return;
        }
        let (prev, cur) = (self.flat[pos.p - 1], self.flat[pos.p]);
        let mergeable = match (prev, cur) {
            (PLoc::Top(a), PLoc::Top(b)) => a + 1 == b,
            (PLoc::Cell(a, r, c, _), PLoc::Cell(b, r2, c2, _)) => (a, r, c) == (b, r2, c2),
            _ => false,
        };
        if mergeable {
            self.snapshot(Kind::None);
            let n = plen(self.para(pos.p - 1));
            self.anchor = Pos {
                p: pos.p - 1,
                off: n,
            };
            self.caret = Pos { p: pos.p, off: 0 };
            // select the paragraph break and delete it
            let (a, b) = (self.anchor, self.caret);
            self.anchor = a;
            self.caret = b;
            self.delete_selection();
        } else if let (PLoc::Cell(..), PLoc::Top(b)) = (prev, cur) {
            // Empty paragraph right after a table: remove it if another follows.
            if plen(self.para(pos.p)) == 0 && b + 1 < self.blocks().len() {
                self.snapshot(Kind::None);
                self.blocks_mut().remove(b);
                self.caret = Pos {
                    p: pos.p - 1,
                    off: plen(self.para(pos.p - 1)),
                };
                self.anchor = self.caret;
                self.changed();
            } else {
                self.move_caret(Move::Left, false);
            }
        }
    }

    pub fn delete_forward(&mut self) {
        if self.has_selection() {
            self.snapshot(Kind::None);
            self.delete_selection();
            return;
        }
        let pos = self.caret;
        let n = plen(self.para(pos.p));
        if pos.off < n {
            self.snapshot(Kind::Deleting);
            let p = self.para_mut_at(pos.p);
            delete_range(p, pos.off, pos.off + 1);
            self.changed();
            return;
        }
        if pos.p + 1 >= self.flat.len() {
            return;
        }
        let (cur, next) = (self.flat[pos.p], self.flat[pos.p + 1]);
        let mergeable = match (cur, next) {
            (PLoc::Top(a), PLoc::Top(b)) => a + 1 == b,
            (PLoc::Cell(a, r, c, _), PLoc::Cell(b, r2, c2, _)) => (a, r, c) == (b, r2, c2),
            _ => false,
        };
        if mergeable {
            self.snapshot(Kind::None);
            self.anchor = Pos {
                p: pos.p + 1,
                off: 0,
            };
            self.delete_selection();
            self.caret = pos;
            self.anchor = pos;
        }
    }

    // ------------------------------------------------------------ formatting

    fn style_here(&self) -> CharStyle {
        if let Some(p) = &self.pending {
            return p.clone();
        }
        let (a, _) = self.ordered();
        let p = self.para(a.p);
        if self.has_selection() {
            // style of the first selected character
            style_at(p, (a.off + 1).min(plen(p)))
        } else {
            style_at(p, a.off)
        }
    }

    pub fn caret_style(&self) -> CaretStyle {
        let s = self.style_here();
        CaretStyle {
            bold: s.bold == Some(true),
            italic: s.italic == Some(true),
            underline: s.underline.is_some(),
            size: s.size_pt.unwrap_or(self.setup.font_pt),
            color: s.color.clone(),
            align: self.para(self.caret.p).align,
        }
    }

    /// Apply `f` to every run in the selection, or to the pending style.
    fn format(&mut self, f: impl Fn(&mut CharStyle)) {
        if !self.has_selection() {
            let mut st = self.style_here();
            f(&mut st);
            self.pending = Some(st);
            return;
        }
        self.snapshot(Kind::None);
        let (a, b) = self.ordered();
        for i in a.p..=b.p {
            let n = plen(self.para(i));
            let s = if i == a.p { a.off } else { 0 };
            let e = if i == b.p { b.off } else { n };
            let p = self.para_mut_at(i);
            for_each_run_in(p, s, e, |r| f(&mut r.style));
        }
        self.changed();
    }

    pub fn toggle_bold(&mut self) {
        let on = self.style_here().bold != Some(true);
        self.format(|s| s.bold = if on { Some(true) } else { None });
    }
    pub fn toggle_italic(&mut self) {
        let on = self.style_here().italic != Some(true);
        self.format(|s| s.italic = if on { Some(true) } else { None });
    }
    pub fn toggle_underline(&mut self) {
        let on = self.style_here().underline.is_none();
        self.format(|s| s.underline = if on { Some(1) } else { None });
    }
    pub fn set_size(&mut self, pt: f32) {
        let base = self.setup.font_pt;
        self.format(|s| {
            s.size_pt = if (pt - base).abs() < 0.01 {
                None
            } else {
                Some(pt)
            }
        });
    }
    /// Ctrl+↑ / Ctrl+↓: next / previous size in the size list.
    pub fn size_step(&mut self, up: bool) {
        let cur = self.style_here().size_pt.unwrap_or(self.setup.font_pt);
        let next = if up {
            SIZES
                .iter()
                .copied()
                .find(|&s| s > cur + 0.01)
                .unwrap_or(cur)
        } else {
            SIZES
                .iter()
                .rev()
                .copied()
                .find(|&s| s < cur - 0.01)
                .unwrap_or(cur)
        };
        self.set_size(next);
    }
    pub fn set_color(&mut self, color: Option<String>) {
        self.format(move |s| s.color = color.clone());
    }

    pub fn set_align(&mut self, align: Align) {
        self.snapshot(Kind::None);
        let (a, b) = self.ordered();
        for i in a.p..=b.p {
            self.para_mut_at(i).align = align;
        }
        self.changed();
    }

    // ------------------------------------------------------------ tables

    pub fn in_table(&self) -> bool {
        matches!(self.flat[self.caret.p], PLoc::Cell(..))
    }

    fn make_row(&self, cols: usize, width: u16) -> Row {
        let w = (width as usize / cols.max(1)) as u16;
        let cells = (0..cols)
            .map(|c| Cell {
                left: c as u16 * w + 2,
                right: (c as u16 + 1) * w - 2,
                paragraphs: vec![Paragraph::default()],
            })
            .collect();
        Row {
            cells,
            rules: vec![(0x13, w); cols + 1],
        }
    }

    /// 罫線 → 表作成: insert a ruled table after the caret's paragraph.
    pub fn insert_table(&mut self, rows: usize, cols: usize) -> bool {
        let PLoc::Top(b) = self.flat[self.caret.p] else {
            return false;
        };
        let (rows, cols) = (rows.clamp(1, 100), cols.clamp(1, 20));
        self.snapshot(Kind::None);
        let width = (self.setup.chars_per_line * 4) as u16;
        let t = Table {
            width,
            rows: (0..rows).map(|_| self.make_row(cols, width)).collect(),
        };
        let empty = plen(self.para(self.caret.p)) == 0;
        let at = if empty { b } else { b + 1 };
        if empty {
            self.blocks_mut()[b] = Block::Table(t);
        } else {
            self.blocks_mut().insert(at, Block::Table(t));
        }
        normalize_blocks(self.blocks_mut());
        self.flat = flatten(self.blocks());
        let first = self
            .flat
            .iter()
            .position(|l| matches!(l, PLoc::Cell(bb, 0, 0, 0) if *bb == at))
            .unwrap_or(0);
        self.caret = Pos { p: first, off: 0 };
        self.anchor = self.caret;
        self.changed();
        true
    }

    pub fn insert_row(&mut self, below: bool) -> bool {
        let PLoc::Cell(b, r, _, _) = self.flat[self.caret.p] else {
            return false;
        };
        self.snapshot(Kind::None);
        let blocks = self.blocks_mut();
        if let Block::Table(t) = &mut blocks[b] {
            let mut row = t.rows[r].clone();
            for c in &mut row.cells {
                c.paragraphs = vec![Paragraph::default()];
            }
            t.rows.insert(if below { r + 1 } else { r }, row);
        }
        self.changed();
        true
    }

    pub fn delete_row(&mut self) -> bool {
        let PLoc::Cell(b, r, _, _) = self.flat[self.caret.p] else {
            return false;
        };
        self.snapshot(Kind::None);
        let blocks = self.blocks_mut();
        let remove_table = matches!(&blocks[b], Block::Table(t) if t.rows.len() <= 1);
        if remove_table {
            blocks.remove(b);
        } else if let Block::Table(t) = &mut blocks[b] {
            t.rows.remove(r);
        }
        normalize_blocks(self.blocks_mut());
        self.flat = flatten(self.blocks());
        self.caret.off = 0;
        self.anchor = self.caret;
        self.changed();
        true
    }

    /// Tab / Shift+Tab inside a table: move to the next / previous cell.
    pub fn next_cell(&mut self, back: bool) -> bool {
        let PLoc::Cell(b, r, c, _) = self.flat[self.caret.p] else {
            return false;
        };
        let target = if back {
            (0..self.caret.p).rev().find(|&i| matches!(self.flat[i], PLoc::Cell(bb, rr, cc, 0) if bb == b && (rr, cc) != (r, c)))
        } else {
            (self.caret.p + 1..self.flat.len()).find(|&i| matches!(self.flat[i], PLoc::Cell(bb, rr, cc, 0) if bb == b && (rr, cc) != (r, c)))
        };
        if let Some(i) = target {
            self.caret = Pos { p: i, off: 0 };
            self.anchor = self.caret;
            self.kind = Kind::None;
            return true;
        }
        false
    }

    // ------------------------------------------------------------ find

    pub fn find(&mut self, query: &str, backward: bool) -> bool {
        if query.is_empty() {
            return false;
        }
        let q: Vec<char> = query.chars().collect();
        let n = self.flat.len();
        let (a, b) = self.ordered();
        let start = if backward { a } else { b };
        for step in 0..=n {
            let i = if backward {
                (start.p + n - step % n) % n
            } else {
                (start.p + step) % n
            };
            let chars: Vec<char> = para_text(self.para(i)).chars().collect();
            let hits: Vec<usize> = (0..chars.len().saturating_sub(q.len() - 1))
                .filter(|&k| chars[k..k + q.len()] == q[..])
                .collect();
            let pick = if backward {
                hits.into_iter()
                    .rev()
                    .find(|&k| step > 0 || k + q.len() <= start.off)
            } else {
                hits.into_iter().find(|&k| step > 0 || k >= start.off)
            };
            if let Some(k) = pick {
                self.anchor = Pos { p: i, off: k };
                self.caret = Pos {
                    p: i,
                    off: k + q.len(),
                };
                self.kind = Kind::None;
                return true;
            }
        }
        false
    }

    pub fn replace_selection_if(&mut self, query: &str, with: &str) -> bool {
        if self.has_selection() && self.selected_text() == query {
            self.insert_text(with);
            return true;
        }
        false
    }

    pub fn replace_all(&mut self, query: &str, with: &str) -> usize {
        if query.is_empty() {
            return 0;
        }
        self.snapshot(Kind::None);
        let qn = query.chars().count();
        let mut count = 0;
        for i in 0..self.flat.len() {
            loop {
                let chars: Vec<char> = para_text(self.para(i)).chars().collect();
                let q: Vec<char> = query.chars().collect();
                let Some(k) =
                    (0..chars.len().saturating_sub(qn - 1)).find(|&k| chars[k..k + qn] == q[..])
                else {
                    break;
                };
                let st = style_at(self.para(i), k + 1);
                let p = self.para_mut_at(i);
                delete_range(p, k, k + qn);
                insert_str(p, k, with, &st);
                count += 1;
                if with.contains(query) {
                    break;
                }
            }
        }
        self.changed();
        count
    }

    // ------------------------------------------------------------ layout

    pub fn set_preedit(&mut self, text: &str) {
        if self.preedit != text {
            self.preedit = text.to_string();
            self.layout = None;
        }
    }

    pub fn set_show_marks(&mut self, on: bool) {
        self.show_marks = on;
        self.layout = None;
    }

    pub fn layout(&mut self) -> &Layout {
        if self.layout.is_none() {
            let pre = if self.preedit.is_empty() {
                None
            } else {
                Some(Preedit {
                    para: self.caret.p,
                    off: self.caret.off,
                    text: &self.preedit,
                })
            };
            let l = layout::layout(
                &self.doc.sheets[self.sheet].blocks,
                &self.setup,
                self.show_marks,
                pre.as_ref(),
            );
            self.layout = Some(l);
        }
        self.layout.as_ref().unwrap()
    }

    fn line_of(&mut self, pos: Pos, upstream: bool) -> Option<usize> {
        let lines = &self.layout().lines;
        let cands: Vec<usize> = (0..lines.len())
            .filter(|&i| {
                lines[i].para == pos.p && lines[i].start <= pos.off && pos.off <= lines[i].end
            })
            .collect();
        if upstream {
            cands.first().copied()
        } else {
            cands.last().copied()
        }
    }

    fn x_of(&mut self, li: usize, off: usize) -> f32 {
        let l = &self.layout().lines[li];
        l.xs[(off - l.start).min(l.xs.len() - 1)]
    }

    pub fn caret_rect(&mut self) -> Option<Rect> {
        let pos = self.caret;
        let li = self.line_of(pos, self.upstream)?;
        let x = self.x_of(li, pos.off);
        let cap = self.setup.pitch().max(self.setup.font_pt * 1.4);
        let l = &self.layout().lines[li];
        let h = l.height.min(cap);
        Some(Rect {
            page: l.page,
            x,
            y: l.top + l.height - h,
            w: 1.0,
            h,
        })
    }

    pub fn selection_rects(&mut self) -> Vec<Rect> {
        if !self.has_selection() {
            return Vec::new();
        }
        let (a, b) = self.ordered();
        let lines = self.layout().lines.clone();
        let mut out = Vec::new();
        for l in &lines {
            if l.para < a.p || l.para > b.p {
                continue;
            }
            let s = if l.para == a.p {
                a.off.max(l.start)
            } else {
                l.start
            };
            let e = if l.para == b.p {
                b.off.min(l.end)
            } else {
                l.end
            };
            if s > e || (l.para == a.p && a.off > l.end) || (l.para == b.p && b.off < l.start) {
                continue;
            }
            let x1 = l.xs[s - l.start];
            let mut x2 = l.xs[e - l.start];
            if l.last && l.para < b.p {
                x2 += self.setup.font_pt / 2.0; // show the selected paragraph mark
            }
            if x2 > x1 {
                out.push(Rect {
                    page: l.page,
                    x: x1,
                    y: l.top,
                    w: x2 - x1,
                    h: l.height,
                });
            }
        }
        out
    }

    /// Mouse: put the caret at a point. `extend` keeps the anchor (Shift / drag).
    pub fn click(&mut self, page: usize, x: f32, y: f32, extend: bool) {
        let Some((pos, up)) = self.hit(page, x, y) else {
            return;
        };
        self.caret = pos;
        self.upstream = up;
        if !extend {
            self.anchor = pos;
        }
        self.goal_x = None;
        self.pending = None;
        self.kind = Kind::None;
    }

    fn hit(&mut self, page: usize, x: f32, y: f32) -> Option<(Pos, bool)> {
        let lines = &self.layout().lines;
        let mut best: Option<(f32, usize)> = None;
        for (i, l) in lines.iter().enumerate() {
            if l.page != page {
                continue;
            }
            let dy = if y < l.top {
                l.top - y
            } else if y > l.top + l.height {
                y - l.top - l.height
            } else {
                0.0
            };
            let dx = if x < l.left {
                l.left - x
            } else if x > l.right {
                x - l.right
            } else {
                0.0
            };
            let score = dy * 1000.0 + dx;
            if best.map(|b| score < b.0).unwrap_or(true) {
                best = Some((score, i));
            }
        }
        let li = match best {
            Some((_, i)) => i,
            None => {
                // page without lines (e.g. after a page break): go to the nearest line before
                let i = lines.iter().rposition(|l| l.page <= page)?;
                i
            }
        };
        let l = &lines[li];
        let mut k = 0;
        while k + 1 < l.xs.len() && x > (l.xs[k] + l.xs[k + 1]) / 2.0 {
            k += 1;
        }
        let off = l.start + k;
        Some((Pos { p: l.para, off }, off == l.end && !l.last))
    }

    /// Double click: select the word (run of the same character class).
    pub fn select_word(&mut self, page: usize, x: f32, y: f32) {
        let Some((pos, _)) = self.hit(page, x, y) else {
            return;
        };
        let chars: Vec<char> = para_text(self.para(pos.p)).chars().collect();
        if chars.is_empty() {
            return;
        }
        let at = pos.off.min(chars.len() - 1);
        let cls = char_class(chars[at]);
        let mut a = at;
        while a > 0 && char_class(chars[a - 1]) == cls {
            a -= 1;
        }
        let mut b = at + 1;
        while b < chars.len() && char_class(chars[b]) == cls {
            b += 1;
        }
        self.anchor = Pos { p: pos.p, off: a };
        self.caret = Pos { p: pos.p, off: b };
        self.kind = Kind::None;
    }

    pub fn move_caret(&mut self, m: Move, extend: bool) {
        self.kind = Kind::None;
        self.pending = None;
        if !extend && self.has_selection() && matches!(m, Move::Left | Move::Right) {
            let (a, b) = self.ordered();
            self.caret = if m == Move::Left { a } else { b };
            self.anchor = self.caret;
            self.goal_x = None;
            return;
        }
        let pos = self.caret;
        let mut up = false;
        let new = match m {
            Move::Left => {
                if pos.off > 0 {
                    Pos {
                        p: pos.p,
                        off: pos.off - 1,
                    }
                } else if pos.p > 0 {
                    Pos {
                        p: pos.p - 1,
                        off: plen(self.para(pos.p - 1)),
                    }
                } else {
                    pos
                }
            }
            Move::Right => {
                if pos.off < plen(self.para(pos.p)) {
                    Pos {
                        p: pos.p,
                        off: pos.off + 1,
                    }
                } else if pos.p + 1 < self.flat.len() {
                    Pos {
                        p: pos.p + 1,
                        off: 0,
                    }
                } else {
                    pos
                }
            }
            Move::WordLeft | Move::WordRight => {
                let chars: Vec<char> = para_text(self.para(pos.p)).chars().collect();
                if m == Move::WordLeft {
                    if pos.off == 0 {
                        if pos.p > 0 {
                            Pos {
                                p: pos.p - 1,
                                off: plen(self.para(pos.p - 1)),
                            }
                        } else {
                            pos
                        }
                    } else {
                        let cls = char_class(chars[pos.off - 1]);
                        let mut k = pos.off - 1;
                        while k > 0 && char_class(chars[k - 1]) == cls {
                            k -= 1;
                        }
                        Pos { p: pos.p, off: k }
                    }
                } else if pos.off >= chars.len() {
                    if pos.p + 1 < self.flat.len() {
                        Pos {
                            p: pos.p + 1,
                            off: 0,
                        }
                    } else {
                        pos
                    }
                } else {
                    let cls = char_class(chars[pos.off]);
                    let mut k = pos.off + 1;
                    while k < chars.len() && char_class(chars[k]) == cls {
                        k += 1;
                    }
                    Pos { p: pos.p, off: k }
                }
            }
            Move::DocStart => Pos { p: 0, off: 0 },
            Move::DocEnd => {
                let last = self.flat.len() - 1;
                Pos {
                    p: last,
                    off: plen(self.para(last)),
                }
            }
            Move::LineStart | Move::LineEnd => {
                let Some(li) = self.line_of(pos, self.upstream) else {
                    return;
                };
                let l = self.layout().lines[li].clone();
                if m == Move::LineStart {
                    Pos {
                        p: pos.p,
                        off: l.start,
                    }
                } else {
                    up = !l.last;
                    Pos {
                        p: pos.p,
                        off: l.end,
                    }
                }
            }
            Move::Up | Move::Down | Move::PageUp | Move::PageDown => {
                let Some(li) = self.line_of(pos, self.upstream) else {
                    return;
                };
                let gx = match self.goal_x {
                    Some(g) => g,
                    None => self.x_of(li, pos.off),
                };
                self.goal_x = Some(gx);
                let lines = self.layout().lines.clone();
                let cur = &lines[li];
                let page_h = self.setup.page_h();
                // absolute y across pages
                let ay = |l: &layout::LineBox| l.page as f32 * page_h + l.top;
                let cy = ay(cur);
                let target_y = match m {
                    Move::Up => cy - 0.5,
                    Move::Down => cy + cur.height + 0.5,
                    Move::PageUp => cy - page_h,
                    _ => cy + page_h,
                };
                let down = matches!(m, Move::Down | Move::PageDown);
                let mut best: Option<(f32, usize)> = None;
                for (i, l) in lines.iter().enumerate() {
                    let ly = ay(l);
                    let ok = if down { ly > cy + 0.5 } else { ly < cy - 0.5 };
                    if !ok {
                        continue;
                    }
                    let dy = if matches!(m, Move::Up | Move::Down) {
                        if down {
                            ly - target_y
                        } else {
                            target_y - (ly + l.height)
                        }
                    } else {
                        (ly - target_y).abs()
                    };
                    let dx = if gx < l.left {
                        l.left - gx
                    } else if gx > l.right {
                        gx - l.right
                    } else {
                        0.0
                    };
                    let score = dy.abs() * 1000.0 + dx;
                    if best.map(|b| score < b.0).unwrap_or(true) {
                        best = Some((score, i));
                    }
                }
                match best {
                    Some((_, i)) => {
                        let l = &lines[i];
                        let mut k = 0;
                        while k + 1 < l.xs.len() && gx > (l.xs[k] + l.xs[k + 1]) / 2.0 {
                            k += 1;
                        }
                        let off = l.start + k;
                        up = off == l.end && !l.last;
                        Pos { p: l.para, off }
                    }
                    None => {
                        if down {
                            let last = self.flat.len() - 1;
                            Pos {
                                p: last,
                                off: plen(self.para(last)),
                            }
                        } else {
                            Pos { p: 0, off: 0 }
                        }
                    }
                }
            }
        };
        if !matches!(m, Move::Up | Move::Down | Move::PageUp | Move::PageDown) {
            self.goal_x = None;
        }
        self.caret = new;
        self.upstream = up;
        if !extend {
            self.anchor = new;
        }
    }

    // ------------------------------------------------------------ queries

    pub fn status(&mut self) -> Status {
        let pages = self.layout().pages.len();
        let (mut page, mut line, mut col) = (1, 1, 1);
        if let Some(li) = self.line_of(self.caret, self.upstream) {
            let x = self.x_of(li, self.caret.off);
            let l = self.layout().lines[li].clone();
            page = l.page + 1;
            line = (((l.top + l.height / 2.0 - self.setup.top()) / self.setup.pitch())
                .floor()
                .max(0.0) as usize)
                + 1;
            col = (((x - l.left) / self.setup.font_pt + 0.001).floor().max(0.0) as usize) + 1;
        }
        let chars = self
            .flat
            .iter()
            .map(|&l| plen(para_ref(self.blocks(), l)))
            .sum();
        Status {
            page,
            pages,
            line,
            col,
            chars,
            overwrite: self.overwrite,
            can_undo: !self.undo.is_empty(),
            can_redo: !self.redo.is_empty(),
            modified: self.modified,
            in_table: self.in_table(),
            has_selection: self.has_selection(),
            sheet: self.sheet,
            sheets: self.doc.sheets.iter().map(|s| s.name.clone()).collect(),
        }
    }

    /// First words of each page, for the jump palette.
    pub fn page_previews(&mut self) -> Vec<String> {
        let n = self.layout().pages.len();
        let lines = self.layout().lines.clone();
        let mut out = vec![String::new(); n];
        for l in &lines {
            if out[l.page].is_empty() && l.end > l.start {
                let t: String = para_text(self.para(l.para))
                    .chars()
                    .skip(l.start)
                    .take((l.end - l.start).min(24))
                    .collect();
                let t = t.trim().to_string();
                if !t.is_empty() {
                    out[l.page] = t;
                }
            }
        }
        out
    }

    /// Caret to the start of a page (jump palette / Ctrl+J).
    pub fn goto_page(&mut self, page: usize) {
        let lines = self.layout().lines.clone();
        if let Some(l) = lines.iter().find(|l| l.page == page) {
            self.caret = Pos {
                p: l.para,
                off: l.start,
            };
            self.anchor = self.caret;
            self.upstream = false;
        }
    }
}

fn char_class(c: char) -> u8 {
    let u = c as u32;
    if c.is_whitespace() || c == '\u{3000}' {
        0
    } else if c.is_alphanumeric() && u < 0x3000 {
        1
    } else if (0x3040..=0x309f).contains(&u) {
        2
    } else if (0x30a0..=0x30ff).contains(&u) || (0xff66..=0xff9f).contains(&u) {
        3
    } else if (0x4e00..=0x9fff).contains(&u) || (0x3400..=0x4dbf).contains(&u) || c == '々' {
        4
    } else if (0xff10..=0xff5a).contains(&u) {
        5
    } else {
        6
    }
}
