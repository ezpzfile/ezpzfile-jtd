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
        let Some(s) = id.checked_mul(block_size).and_then(|x| x.checked_add(20)) else {
            return &[];
        };
        if s >= b.len() || id >= block_count {
            &[]
        } else {
            &b[s..s.saturating_add(block_size).min(b.len())]
        }
    };
    /*
     * Every count here comes from the file, so each is checked against the
     * bytes that are really there. A block may be listed only once per
     * sub-stream would be the honest layout, but rather than reject reuse we
     * cap the total: all sub-streams together may not exceed the container
     * size (plus slack). Without this, a 100 KB file listing one 64 KB block
     * 2,000 times expanded to 128 MB, and a length field of 4 GB was
     * allocated up front.
     */
    let budget = b.len().saturating_mul(2).saturating_add(64 << 10);
    let mut total = 0usize;
    let mut out = Vec::with_capacity(sub_count.min(b.len() / 20 + 1));
    let mut o = dir;
    for _ in 0..sub_count {
        let (Some(_idx), Some(len), Some(n)) = (be32(b, o), be32(b, o + 8), be32(b, o + 12)) else {
            return Err(Error::Corrupt("truncated Ssmg directory".into()));
        };
        // the block ids must be in the file: 4 bytes each
        let listed = b.len().saturating_sub(o + 20) / 4;
        let n = (n as usize).min(listed);
        let len = (len as usize).min(n.saturating_mul(block_size));
        if total.saturating_add(len) > budget {
            return Err(Error::TooLarge(format!(
                "DocumentText sub-streams claim {} bytes from a {}-byte stream",
                total.saturating_add(len),
                b.len()
            )));
        }
        let mut data = Vec::with_capacity(len.min(b.len()));
        for k in 0..n {
            if data.len() >= len {
                break;
            }
            let id = be32(b, o + 20 + k * 4).unwrap_or(u32::MAX) as usize;
            let blk = block(id);
            let take = blk.len().min(len - data.len());
            data.extend_from_slice(&blk[..take]);
        }
        total += data.len();
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
        let end = n.saturating_mul(2).saturating_add(12).min(head.len()).max(12.min(head.len()));
        if head.len() < 12 {
            return Ok(vec![Piece { units: Vec::new(), style: Vec::new() }]);
        }
        return Ok(vec![Piece {
            units: to_units(&head[12..end]),
            style: head[end..].to_vec(),
        }]);
    }
    if head.starts_with(b"QLSTV.01") {
        // each piece entry is 8 bytes of the head; more cannot be listed
        let count = (be32(head, 8).unwrap_or(0) as usize).min(head.len().saturating_sub(12) / 8);
        // pieces may share a sub-stream; together they may not exceed the input (plus slack)
        let budget = b.len().saturating_mul(2).saturating_add(64 << 10);
        let mut total = 0usize;
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
            let end = (n as usize).saturating_mul(2).min(d.len());
            total = total.saturating_add(d.len());
            if total > budget {
                return Err(Error::TooLarge(format!(
                    "DocumentText pieces repeat {total} bytes from a {}-byte stream",
                    b.len()
                )));
            }
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
