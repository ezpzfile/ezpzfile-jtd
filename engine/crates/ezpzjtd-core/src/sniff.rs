//! What kind of file is this? Used to say why a file does not open and what
//! to do instead, and to unwrap compressed Ichitaro files (`.jttc`).
//!
//! Only the first bytes and, for compound files, the stream names are looked
//! at. Nothing here reads the document text.

use crate::cfb::Cfb;
use crate::error::{Error, Result};
use crate::lh5;

/// A short, stable name for the kind of file. Sent with "could not open"
/// reports, so keep the names.
pub fn kind(bytes: &[u8]) -> &'static str {
    if bytes.is_empty() {
        return "empty";
    }
    if Cfb::is_cfb(bytes) {
        let Ok(c) = Cfb::open(bytes.to_vec()) else { return "cfb_damaged" };
        if c.find("/DocumentText").is_some() {
            return "jtd";
        }
        if c.find("/JSCompDocument").is_some() {
            return "jttc";
        }
        if c.find("/WordDocument").is_some() {
            return "doc";
        }
        if c.find("/Workbook").is_some() || c.find("/Book").is_some() {
            return "xls";
        }
        if c.find("/PowerPoint Document").is_some() {
            return "ppt";
        }
        if c.entries().iter().any(|e| e.path.contains("Hwp") || e.path == "/FileHeader") {
            return "hwp";
        }
        // other JustSystems compound files (Hanako, Sanshiro, …) keep JSRV streams
        if c.entries().iter().any(|e| e.path.contains("JSRV")) {
            return "js_other";
        }
        // a cut-off or damaged file still opens as a compound file, with warnings
        if !c.warnings.is_empty() || c.entries().len() <= 1 {
            return "cfb_damaged";
        }
        return "cfb_other";
    }
    let starts = |m: &[u8]| bytes.starts_with(m);
    if starts(b"{\\rtf") {
        return "rtf";
    }
    if starts(b"%PDF") {
        return "pdf";
    }
    if starts(b"PK\x03\x04") {
        return "zip";
    }
    if bytes.len() > 7 && bytes[2] == b'-' && bytes[3] == b'l' && bytes[6] == b'-' {
        return "lzh";
    }
    if starts(b"\x89PNG") || starts(b"\xff\xd8\xff") || starts(b"GIF8") {
        return "image";
    }
    let head = &bytes[..bytes.len().min(512)];
    let lower: Vec<u8> = head.iter().map(|b| b.to_ascii_lowercase()).collect();
    let has = |s: &[u8]| lower.windows(s.len()).any(|w| w == s);
    if has(b"<html") || has(b"<!doctype html") {
        return "html";
    }
    // Ichitaro 3 to 7 (.jxw, .jaw, .jbw, .jfw, .jvw, .jsw) are not compound files.
    // Their headers are not mapped yet; plain text is told apart by having no control bytes.
    let ctrl = head
        .iter()
        .filter(|&&b| b < 0x20 && b != b'\r' && b != b'\n' && b != b'\t' && b != 0x1a)
        .count();
    if ctrl == 0 {
        return "text";
    }
    "unknown"
}

/// For a compressed Ichitaro file (`/JSCompDocument`), the document inside it.
/// `None` when this is not a compressed Ichitaro file.
pub fn unwrap_compressed(bytes: &[u8], max_out: usize) -> Result<Option<Vec<u8>>> {
    if !Cfb::is_cfb(bytes) {
        return Ok(None);
    }
    let c = Cfb::open(bytes.to_vec())?;
    if c.find("/DocumentText").is_some() {
        return Ok(None);
    }
    let Some(stream) = c.read("/JSCompDocument") else { return Ok(None) };
    match lh5::unpack_first(&stream, max_out)? {
        Some(inner) => Ok(Some(inner)),
        None => Err(Error::Unsupported(
            "compressed Ichitaro document (.jttc): the packing inside is not one we know yet".into(),
        )),
    }
}
