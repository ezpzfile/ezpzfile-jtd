//! WebAssembly bindings: open a `.jtd` in the browser, nothing leaves the device.
//!
//! ```js
//! import init, { JtdDocument } from "./ezjtd_wasm.js";
//! await init();
//! const doc = new JtdDocument(new Uint8Array(await file.arrayBuffer()));
//! viewer.innerHTML = doc.html();
//! ```

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct JtdDocument {
    inner: ezjtd_core::Document,
}

#[wasm_bindgen]
impl JtdDocument {
    #[wasm_bindgen(constructor)]
    pub fn new(bytes: &[u8]) -> Result<JtdDocument, JsError> {
        let inner = ezjtd_core::open(bytes.to_vec()).map_err(|e| JsError::new(&e.to_string()))?;
        Ok(JtdDocument { inner })
    }

    /// Plain text of all sheets.
    pub fn text(&self) -> String {
        self.inner.plain_text()
    }

    /// HTML fragment (paragraphs, tables, ruby). Pair with `css()`.
    pub fn html(&self) -> String {
        ezjtd_core::export::to_html(&self.inner)
    }

    /// Stylesheet for `html()`.
    pub fn css() -> String {
        ezjtd_core::export::HTML_CSS.to_string()
    }

    pub fn markdown(&self) -> String {
        ezjtd_core::export::to_markdown(&self.inner)
    }

    /// Full document model as JSON.
    pub fn json(&self) -> String {
        serde_json::to_string(&self.inner).unwrap_or_default()
    }

    /// Summary information (title, author, dates, original path …) as JSON.
    #[wasm_bindgen(js_name = summaryJson)]
    pub fn summary_json(&self) -> String {
        serde_json::to_string(&self.inner.summary).unwrap_or_default()
    }

    #[wasm_bindgen(js_name = sheetCount)]
    pub fn sheet_count(&self) -> usize {
        self.inner.sheets.len()
    }
}
