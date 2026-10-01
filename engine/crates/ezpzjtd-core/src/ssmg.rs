//! `SsmgV.01`: the block container inside the `/DocumentText` stream.
//!
//! `/DocumentText` is not one flat buffer. It is a tiny block store
//! (similar to how an editor keeps a "piece table"):
//!
//! ```text
//! "SsmgV.01" | u32 sub_count | u32 block_size (256) | u32 block_count
//! block 0 .. block N-1                      (block_size bytes each)
//! directory: per sub-stream
//!     u32 index | u32 0 | u32 byte_len | u32 n | u32 n | u32 block_id × n
//! u32 0
//! ```
//!
//! All integers are big-endian. Each sub-stream is the concatenation of its
//! blocks, cut to `byte_len`.
//!
//! Two layouts of sub-stream 0 have been observed:
//!
//! - `TextV.01` + u32 unit_count: one piece. `unit_count` UTF-16BE units of
//!   text, then the style event list for that piece.
//! - `QLSTV.01` + u32 piece_count + (u32 unit_count, u32 sub_index) × n:
//!   a piece list. Piece *k* lives in sub-stream `sub_index`: `unit_count`
//!   units of text, then its own style event list. The logical document text
//!   is the pieces concatenated in list order. (Seen in larger documents.)

use crate::error::{Error, Result};

pub const MAGIC: &[u8; 8] = b"SsmgV.01";

/// One piece of document text plus its raw style event bytes.
#[derive(Debug, Clone)]
pub struct Piece {
    pub units: Vec<u16>,
    pub style: Vec<u8>,
}

fn be32(b: &[u8], o: usize) -> Option<u32> {
    b.get(o..o + 4)
        .map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
}

fn to_units(b: &[u8]) -> Vec<u16> {
    b.chunks_exact(2)
        .map(|c| u16::from_be_bytes([c[0], c[1]]))
        .collect()
}

/// Split an Ssmg container into its sub-streams.
pub fn substreams(b: &[u8]) -> Result<Vec<Vec<u8>>> {
    if b.len() < 20 || &b[..8] != MAGIC {
        return Err(Error::NotJtd(
            "DocumentText does not start with SsmgV.01".into(),
        ));
    }
    let sub_count = be32(b, 8).unwrap_or(0) as usize;
    let block_size = be32(b, 12).unwrap_or(0) as usize;
    let block_count = be32(b, 16).unwrap_or(0) as usize;
    if block_size == 0 || sub_count == 0 || sub_count > 4096 {
        return Err(Error::Corrupt(format!(
            "bad Ssmg header {sub_count}/{block_size}"
        )));
    }
    let dir = 20usize
        .checked_add(block_size.saturating_mul(block_count))
        .ok_or_else(|| Error::Corrupt("Ssmg size overflow".into()))?;
    let block = |id: usize| -> &[u8] {
        let s = 20 + id * block_size;
        if s >= b.len() || id >= block_count {
            &[]
        } else {
            &b[s..(s + block_size).min(b.len())]
        }
    };
    let mut out = Vec::with_capacity(sub_count);
    let mut o = dir;
    for _ in 0..sub_count {
        let (Some(_idx), Some(len), Some(n)) = (be32(b, o), be32(b, o + 8), be32(b, o + 12)) else {
            return Err(Error::Corrupt("truncated Ssmg directory".into()));
        };
        let n = n as usize;
        let mut data = Vec::with_capacity(len as usize);
        for k in 0..n {
            let id = be32(b, o + 20 + k * 4).unwrap_or(u32::MAX) as usize;
            data.extend_from_slice(block(id));
        }
        data.truncate(len as usize);
        out.push(data);
        o += 20 + n * 4;
    }
    Ok(out)
}

/// Decode `/DocumentText` into text pieces.
pub fn pieces(b: &[u8]) -> Result<Vec<Piece>> {
    let subs = substreams(b)?;
    let head = &subs[0];
    if head.starts_with(b"TextV.01") {
        let n = be32(head, 8).unwrap_or(0) as usize;
        let end = (12 + n * 2).min(head.len());
        return Ok(vec![Piece {
            units: to_units(&head[12..end]),
            style: head[end..].to_vec(),
        }]);
    }
    if head.starts_with(b"QLSTV.01") {
        let count = be32(head, 8).unwrap_or(0) as usize;
        let mut out = Vec::new();
        for k in 0..count {
            let (Some(n), Some(si)) = (be32(head, 12 + k * 8), be32(head, 16 + k * 8)) else {
                break;
            };
            let Some(d) = subs.get(si as usize) else {
                return Err(Error::Corrupt(format!(
                    "QLST points to missing sub-stream {si}"
                )));
            };
            let end = (n as usize * 2).min(d.len());
            out.push(Piece {
                units: to_units(&d[..end]),
                style: d[end..].to_vec(),
            });
        }
        return Ok(out);
    }
    Err(Error::Unsupported(format!(
        "unknown DocumentText layout {:?}",
        String::from_utf8_lossy(&head[..head.len().min(8)])
    )))
}
