//! WebAssembly bindings.
//!
//! - [`JtdDocument`]: read-only viewer API.
//! - [`JtdEditor`]: the editor engine. The UI sends commands and draws the
//!   per-page display lists it gets back; it never edits text itself.
//!
//! Nothing leaves the device: all parsing, layout and export run here.

use ezjtd_core::doc::Align;
use ezjtd_core::edit::{Editor, Move};
use wasm_bindgen::prelude::*;

fn js_err(e: impl std::fmt::Display) -> JsError {
    JsError::new(&e.to_string())
}

fn json<T: serde::Serialize>(v: &T) -> String {
    serde_json::to_string(v).unwrap_or_else(|_| "null".into())
}

#[wasm_bindgen]
pub struct JtdDocument {
    inner: ezjtd_core::Document,
}

#[wasm_bindgen]
impl JtdDocument {
    #[wasm_bindgen(constructor)]
    pub fn new(bytes: &[u8]) -> Result<JtdDocument, JsError> {
        Ok(JtdDocument {
            inner: ezjtd_core::open(bytes.to_vec()).map_err(js_err)?,
        })
    }
    pub fn text(&self) -> String {
        self.inner.plain_text()
    }
    pub fn html(&self) -> String {
        ezjtd_core::export::to_html(&self.inner)
    }
    pub fn css() -> String {
        ezjtd_core::export::HTML_CSS.to_string()
    }
    pub fn markdown(&self) -> String {
        ezjtd_core::export::to_markdown(&self.inner)
    }
    pub fn json(&self) -> String {
        json(&self.inner)
    }
    #[wasm_bindgen(js_name = summaryJson)]
    pub fn summary_json(&self) -> String {
        json(&self.inner.summary)
    }
    #[wasm_bindgen(js_name = sheetCount)]
    pub fn sheet_count(&self) -> usize {
        self.inner.sheets.len()
    }
}

#[wasm_bindgen]
pub struct JtdEditor {
    ed: Editor,
    /// The .jtd the document came from (or was last saved as): saving as
    /// .jtd patches this file.
    original: Option<Vec<u8>>,
    warnings: Vec<String>,
}

#[wasm_bindgen]
impl JtdEditor {
    /// New blank document.
    #[wasm_bindgen(constructor)]
    pub fn new() -> JtdEditor {
        JtdEditor {
            ed: Editor::blank(),
            original: None,
            warnings: Vec::new(),
        }
    }

    /// Open an Ichitaro file.
    pub fn open(bytes: &[u8]) -> Result<JtdEditor, JsError> {
        let doc = ezjtd_core::open(bytes.to_vec()).map_err(js_err)?;
        Ok(JtdEditor {
            ed: Editor::new(doc),
            original: Some(bytes.to_vec()),
            warnings: Vec::new(),
        })
    }

    // ---------------------------------------------------------- drawing
    #[wasm_bindgen(js_name = pageCount)]
    pub fn page_count(&mut self) -> usize {
        self.ed.layout().pages.len()
    }
    /// Display list of one page: `{w, h, items:[…]}` in points.
    #[wasm_bindgen(js_name = pageJson)]
    pub fn page_json(&mut self, i: usize) -> String {
        self.ed
            .layout()
            .pages
            .get(i)
            .map(json)
            .unwrap_or_else(|| "null".into())
    }
    #[wasm_bindgen(js_name = caretJson)]
    pub fn caret_json(&mut self) -> String {
        json(&self.ed.caret_rect())
    }
    #[wasm_bindgen(js_name = selectionJson)]
    pub fn selection_json(&mut self) -> String {
        json(&self.ed.selection_rects())
    }
    #[wasm_bindgen(js_name = statusJson)]
    pub fn status_json(&mut self) -> String {
        json(&self.ed.status())
    }
    #[wasm_bindgen(js_name = styleJson)]
    pub fn style_json(&self) -> String {
        json(&self.ed.caret_style())
    }
    #[wasm_bindgen(js_name = previewsJson)]
    pub fn previews_json(&mut self) -> String {
        json(&self.ed.page_previews())
    }
    #[wasm_bindgen(js_name = summaryJson)]
    pub fn summary_json(&self) -> String {
        json(&self.ed.doc.summary)
    }
    #[wasm_bindgen(js_name = setupJson)]
    pub fn setup_json(&self) -> String {
        json(&self.ed.setup)
    }

    // ---------------------------------------------------------- input
    #[wasm_bindgen(js_name = insertText)]
    pub fn insert_text(&mut self, s: &str) {
        self.ed.insert_text(s)
    }
    pub fn enter(&mut self) {
        self.ed.enter()
    }
    pub fn backspace(&mut self) {
        self.ed.backspace()
    }
    #[wasm_bindgen(js_name = deleteForward)]
    pub fn delete_forward(&mut self) {
        self.ed.delete_forward()
    }
    #[wasm_bindgen(js_name = pageBreak)]
    pub fn page_break(&mut self) {
        self.ed.page_break()
    }
    #[wasm_bindgen(js_name = setPreedit)]
    pub fn set_preedit(&mut self, s: &str) {
        self.ed.set_preedit(s)
    }

    /// `left right up down home end docStart docEnd pageUp pageDown wordLeft wordRight`
    #[wasm_bindgen(js_name = moveCaret)]
    pub fn move_caret(&mut self, kind: &str, extend: bool) {
        let m = match kind {
            "left" => Move::Left,
            "right" => Move::Right,
            "up" => Move::Up,
            "down" => Move::Down,
            "home" => Move::LineStart,
            "end" => Move::LineEnd,
            "docStart" => Move::DocStart,
            "docEnd" => Move::DocEnd,
            "pageUp" => Move::PageUp,
            "pageDown" => Move::PageDown,
            "wordLeft" => Move::WordLeft,
            "wordRight" => Move::WordRight,
            _ => return,
        };
        self.ed.move_caret(m, extend)
    }
    pub fn click(&mut self, page: usize, x: f32, y: f32, extend: bool) {
        self.ed.click(page, x, y, extend)
    }
    #[wasm_bindgen(js_name = selectWord)]
    pub fn select_word(&mut self, page: usize, x: f32, y: f32) {
        self.ed.select_word(page, x, y)
    }
    #[wasm_bindgen(js_name = selectAll)]
    pub fn select_all(&mut self) {
        self.ed.select_all()
    }
    #[wasm_bindgen(js_name = selectedText)]
    pub fn selected_text(&self) -> String {
        self.ed.selected_text()
    }
    pub fn cut(&mut self) -> String {
        let t = self.ed.selected_text();
        if self.ed.has_selection() {
            self.ed.backspace();
        }
        t
    }
    pub fn undo(&mut self) -> bool {
        self.ed.undo()
    }
    pub fn redo(&mut self) -> bool {
        self.ed.redo()
    }

    // ---------------------------------------------------------- format
    pub fn bold(&mut self) {
        self.ed.toggle_bold()
    }
    pub fn italic(&mut self) {
        self.ed.toggle_italic()
    }
    pub fn underline(&mut self) {
        self.ed.toggle_underline()
    }
    #[wasm_bindgen(js_name = setSize)]
    pub fn set_size(&mut self, pt: f32) {
        self.ed.set_size(pt)
    }
    #[wasm_bindgen(js_name = sizeStep)]
    pub fn size_step(&mut self, up: bool) {
        self.ed.size_step(up)
    }
    /// `"#rrggbb"`, or empty for automatic colour.
    #[wasm_bindgen(js_name = setColor)]
    pub fn set_color(&mut self, c: &str) {
        self.ed.set_color(if c.is_empty() {
            None
        } else {
            Some(c.to_string())
        })
    }
    /// `left` / `center` / `right`
    pub fn align(&mut self, a: &str) {
        self.ed.set_align(match a {
            "center" => Align::Center,
            "right" => Align::Right,
            _ => Align::Left,
        })
    }

    // ---------------------------------------------------------- tables
    #[wasm_bindgen(js_name = insertTable)]
    pub fn insert_table(&mut self, rows: usize, cols: usize) -> bool {
        self.ed.insert_table(rows, cols)
    }
    #[wasm_bindgen(js_name = insertRow)]
    pub fn insert_row(&mut self, below: bool) -> bool {
        self.ed.insert_row(below)
    }
    #[wasm_bindgen(js_name = deleteRow)]
    pub fn delete_row(&mut self) -> bool {
        self.ed.delete_row()
    }
    #[wasm_bindgen(js_name = nextCell)]
    pub fn next_cell(&mut self, back: bool) -> bool {
        self.ed.next_cell(back)
    }

    // ---------------------------------------------------------- find
    pub fn find(&mut self, q: &str, backward: bool) -> bool {
        self.ed.find(q, backward)
    }
    pub fn replace(&mut self, q: &str, with: &str) -> bool {
        let done = self.ed.replace_selection_if(q, with);
        self.ed.find(q, false);
        done
    }
    #[wasm_bindgen(js_name = replaceAll)]
    pub fn replace_all(&mut self, q: &str, with: &str) -> usize {
        self.ed.replace_all(q, with)
    }

    // ---------------------------------------------------------- view
    #[wasm_bindgen(js_name = setShowMarks)]
    pub fn set_show_marks(&mut self, on: bool) {
        self.ed.set_show_marks(on)
    }
    #[wasm_bindgen(js_name = setOverwrite)]
    pub fn set_overwrite(&mut self, on: bool) {
        self.ed.overwrite = on
    }
    #[wasm_bindgen(js_name = setSheet)]
    pub fn set_sheet(&mut self, i: usize) {
        self.ed.set_sheet(i)
    }
    #[wasm_bindgen(js_name = gotoPage)]
    pub fn goto_page(&mut self, i: usize) {
        self.ed.goto_page(i)
    }

    // ---------------------------------------------------------- save
    /// Word document of all sheets.
    #[wasm_bindgen(js_name = toDocx)]
    pub fn to_docx(&self) -> Vec<u8> {
        ezjtd_core::docx::to_docx(&self.ed.doc, None, &self.ed.setup)
    }
    #[wasm_bindgen(js_name = toHtml)]
    pub fn to_html(&self) -> String {
        ezjtd_core::export::to_html_page(&self.ed.doc)
    }
    #[wasm_bindgen(js_name = toText)]
    pub fn to_text(&self) -> String {
        self.ed.doc.plain_text()
    }
    #[wasm_bindgen(js_name = toMarkdown)]
    pub fn to_markdown(&self) -> String {
        ezjtd_core::export::to_markdown(&self.ed.doc)
    }
    /// PDF from the rendered pages: `jpegs` is every page's JPEG one after
    /// another, `lens` their byte lengths, `dims` pixel width/height pairs.
    #[wasm_bindgen(js_name = toPdf)]
    pub fn to_pdf(&mut self, jpegs: &[u8], lens: &[u32], dims: &[u32], title: &str) -> Vec<u8> {
        let mut images = Vec::new();
        let mut o = 0usize;
        for (k, &n) in lens.iter().enumerate() {
            let n = n as usize;
            images.push(ezjtd_core::pdf::PageImage {
                jpeg: jpegs[o..(o + n).min(jpegs.len())].to_vec(),
                px_w: dims.get(2 * k).copied().unwrap_or(1),
                px_h: dims.get(2 * k + 1).copied().unwrap_or(1),
            });
            o += n;
        }
        let pages = self.ed.layout().pages.clone();
        ezjtd_core::pdf::to_pdf(&pages, &images, title)
    }
    /// Saving as .jtd is possible (the document was opened from a .jtd).
    #[wasm_bindgen(js_name = canSaveJtd)]
    pub fn can_save_jtd(&self) -> bool {
        self.original.is_some()
    }
    /// The document as an Ichitaro file, made by patching the original.
    /// Fails (with a message for the user) when an edit cannot be stored yet;
    /// nothing is written in that case.
    #[wasm_bindgen(js_name = toJtd)]
    pub fn to_jtd(&mut self) -> Result<Vec<u8>, JsError> {
        let orig = self
            .original
            .as_ref()
            .ok_or_else(|| JsError::new("新規文書は一太郎形式でまだ保存できません"))?;
        let s = ezjtd_core::save::save(orig, &self.ed.doc).map_err(js_err)?;
        self.warnings = s.warnings;
        self.original = Some(s.bytes.clone());
        Ok(s.bytes)
    }
    /// Notes from the last .jtd save (JSON array of strings).
    #[wasm_bindgen(js_name = saveWarnings)]
    pub fn save_warnings(&self) -> String {
        json(&self.warnings)
    }
    #[wasm_bindgen(js_name = markSaved)]
    pub fn mark_saved(&mut self) {
        self.ed.modified = false
    }
}

impl Default for JtdEditor {
    fn default() -> Self {
        Self::new()
    }
}
