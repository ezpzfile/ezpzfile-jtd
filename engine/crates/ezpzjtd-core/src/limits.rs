//! Resource limits for untrusted files.
//!
//! A `.jtd` is a small file system with its own length fields, block lists and
//! page settings. A crafted file can claim far more than it holds (a block
//! listed thousands of times, a length of 4 GB, one character per page), and a
//! reader that trusts those numbers can be made to allocate gigabytes or lay
//! out millions of pages from a few kilobytes.
//!
//! The reader therefore checks every count against the bytes that are really
//! there, and stops at the limits below. The defaults are far above anything a
//! real Ichitaro document needs (the largest public samples use well under 1%
//! of them) while keeping a browser tab safe. Embedders can tighten them with
//! [`crate::doc::open_with`] or the wasm `openWith` / `withLimits` calls.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Limits {
    /// Largest input file accepted, in bytes.
    pub max_bytes: usize,
    /// Most text units (UTF-16) read from all sheets together.
    pub max_text_units: usize,
    /// Most sheets (シート) read; further sheets are skipped with a warning.
    pub max_sheets: usize,
    /// Most pages laid out; the rest of the document is cut with a warning
    /// ([`crate::layout::Layout::truncated`]).
    pub max_pages: usize,
    /// Decoded text streams may be at most this many times the input size.
    pub max_expand: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_bytes: 256 << 20,
            max_text_units: 8_000_000,
            max_sheets: 64,
            max_pages: 2_000,
            max_expand: 4,
        }
    }
}

impl Limits {
    /// Parse limits from JSON (`{"maxPages": 500, …}`); missing keys keep
    /// their defaults, zero means "use the default".
    pub fn from_json(s: &str) -> Limits {
        let mut l: Limits = serde_json_lite(s).unwrap_or_default();
        let d = Limits::default();
        if l.max_bytes == 0 {
            l.max_bytes = d.max_bytes;
        }
        if l.max_text_units == 0 {
            l.max_text_units = d.max_text_units;
        }
        if l.max_sheets == 0 {
            l.max_sheets = d.max_sheets;
        }
        if l.max_pages == 0 {
            l.max_pages = d.max_pages;
        }
        if l.max_expand == 0 {
            l.max_expand = d.max_expand;
        }
        l
    }
}

/// Tiny reader for `{"key": number, …}` so the core crate needs no JSON
/// dependency. Unknown keys and non-numbers are ignored.
fn serde_json_lite(s: &str) -> Option<Limits> {
    let mut l = Limits::default();
    let body = s.trim().strip_prefix('{')?.strip_suffix('}')?;
    for pair in body.split(',') {
        let Some((k, v)) = pair.split_once(':') else { continue };
        let k = k.trim().trim_matches('"');
        let Ok(v) = v.trim().parse::<f64>() else { continue };
        if !(v.is_finite() && v >= 0.0) {
            continue;
        }
        let v = v.min(usize::MAX as f64) as usize;
        match k {
            "maxBytes" => l.max_bytes = v,
            "maxTextUnits" => l.max_text_units = v,
            "maxSheets" => l.max_sheets = v,
            "maxPages" => l.max_pages = v,
            "maxExpand" => l.max_expand = v,
            _ => {}
        }
    }
    Some(l)
}
