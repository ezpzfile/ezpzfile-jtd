//! Tokenizer for the text units of `/DocumentText`.
//!
//! Text is UTF-16BE. Structure is carried by control units and by
//! *self-describing records* that start with `0x001C`:
//!
//! ```text
//! 001C class len payload… len 0000 class 001F      ordinary record (len = total words)
//! 001C 0001 len … 001D  <text>  001E flen 0000 0001 001F   inline record
//! ```
//!
//! Record classes:
//! - `0x0010` line/paragraph header. Payload = `0000` then a TLV list
//!   `(tag, count, value×count)…` closed by `FFFF 0000`.
//! - `0x0030` table cell header: `0000 left right flags 0000`.
//! - `0x0000` context for the next inline record (e.g. 均等割付 width).
//! - `0x0020` table → text transition.
//! - `0x0001` inline segment (ruby base, ruby reading, distributed text, fields).
//!
//! Other control units: `000A` paragraph end, `000C` page break,
//! `000E` table row end, `0000` end of text.

use serde::Serialize;

pub const REC: u16 = 0x001c;
pub const TEXT_START: u16 = 0x001f;
pub const INLINE_TEXT: u16 = 0x001d;
pub const INLINE_END: u16 = 0x001e;
pub const PARA_END: u16 = 0x000a;
pub const PAGE_BREAK: u16 = 0x000c;
pub const ROW_END: u16 = 0x000e;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Token {
    /// Visible characters. `start` is the unit index of the first character.
    Text { start: usize, text: String },
    /// A `0x001C` record. `payload` excludes the 3 header and 4 footer words.
    Record {
        start: usize,
        class: u16,
        payload: Vec<u16>,
    },
    /// An inline segment. `header` is the full record header (ends with 001D).
    Inline {
        start: usize,
        text_start: usize,
        header: Vec<u16>,
        text: String,
    },
    /// A lone control unit.
    Control { start: usize, code: u16 },
}

fn is_control(u: u16) -> bool {
    u < 0x20 || (0x7f..=0x9f).contains(&u)
}

/// Units that are displayed as text even though they are below 0x20.
fn is_text_control(u: u16) -> bool {
    u == 0x0009
}

pub fn tokenize(u: &[u16]) -> Vec<Token> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < u.len() {
        let w = u[i];
        if w == REC && i + 2 < u.len() {
            let class = u[i + 1];
            let len = u[i + 2] as usize;
            if class == 0x0001 && len >= 3 && i + len <= u.len() && u[i + len - 1] == INLINE_TEXT {
                let ts = i + len;
                let mut k = ts;
                while k < u.len() && u[k] != INLINE_END && u[k] != REC {
                    k += 1;
                }
                let text = String::from_utf16_lossy(&u[ts..k]);
                out.push(Token::Inline {
                    start: i,
                    text_start: ts,
                    header: u[i..ts].to_vec(),
                    text,
                });
                // footer: 001E flen 0000 0001 001F
                if k < u.len() && u[k] == INLINE_END {
                    let flen = u.get(k + 1).copied().unwrap_or(0) as usize;
                    if flen >= 2 && k + flen <= u.len() && u[k + flen - 1] == TEXT_START {
                        i = k + flen;
                    } else {
                        i = k + 1;
                    }
                } else {
                    i = k;
                }
                continue;
            }
            if len >= 7
                && i + len <= u.len()
                && u[i + len - 1] == TEXT_START
                && u[i + len - 2] == class
                && u[i + len - 4] as usize == len
            {
                out.push(Token::Record {
                    start: i,
                    class,
                    payload: u[i + 3..i + len - 4].to_vec(),
                });
                i += len;
                continue;
            }
        }
        if is_control(w) && !is_text_control(w) {
            if w != TEXT_START {
                out.push(Token::Control { start: i, code: w });
            }
            i += 1;
            continue;
        }
        let s = i;
        while i < u.len() && (!is_control(u[i]) || is_text_control(u[i])) {
            i += 1;
        }
        out.push(Token::Text {
            start: s,
            text: String::from_utf16_lossy(&u[s..i]),
        });
    }
    out
}

/// Parse the TLV list of a `0x0010` record payload.
pub fn para_tlv(payload: &[u16]) -> Vec<(u16, Vec<u16>)> {
    let mut out = Vec::new();
    let mut j = 1usize;
    while j + 1 < payload.len() {
        let tag = payload[j];
        if tag == 0xffff {
            break;
        }
        let n = payload[j + 1] as usize;
        let end = (j + 2 + n).min(payload.len());
        out.push((tag, payload[j + 2..end].to_vec()));
        j = j + 2 + n;
    }
    out
}

/// Inline segment kind, from the selector words of the header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InlineKind {
    /// Visible text (ruby base, 均等割付 text, placeholder).
    Visible,
    /// Ruby (furigana) reading for the previous visible inline.
    RubyReading,
    /// Template instruction or other hidden text.
    Hidden,
}

pub fn inline_kind(header: &[u16]) -> InlineKind {
    // 001C 0001 0007 0000 a b 001D
    let a = header.get(4).copied().unwrap_or(0);
    let b = header.get(5).copied().unwrap_or(0);
    match (a, b) {
        (_, 0x0082) => InlineKind::RubyReading,
        (0, _) => InlineKind::Visible,
        (1, 0x0000) => InlineKind::Hidden,
        _ => InlineKind::Visible,
    }
}
