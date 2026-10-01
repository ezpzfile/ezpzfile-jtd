//! Writing Ichitaro documents — experimental.
//!
//! What is understood well enough to write:
//! - the CFB container ([`crate::cfbw`]),
//! - the `SsmgV.01` block store inside `/DocumentText`,
//! - text units and the character style event list.
//!
//! What is not understood yet: the layout caches (`/LineMark`, `/PageMark`,
//! `/DocumentTextPositionTables`) that point into the text by position.
//! The functions here let us produce test files that answer the open
//! question — does Ichitaro rebuild those caches, or reject the file?

use crate::error::{Error, Result};
use crate::ssmg;
use crate::style::{parse_events, StyleEvent};

/// Pack sub-streams into an `SsmgV.01` block store (256-byte blocks, in order).
pub fn ssmg_pack(subs: &[Vec<u8>]) -> Vec<u8> {
    const BS: usize = 256;
    let mut blocks: Vec<u8> = Vec::new();
    let mut dir: Vec<u32> = Vec::new();
    let mut next = 0u32;
    for (i, s) in subs.iter().enumerate() {
        let n = s.len().div_ceil(BS).max(1);
        let mut padded = s.clone();
        padded.resize(n * BS, 0);
        blocks.extend_from_slice(&padded);
        dir.extend_from_slice(&[i as u32, 0, s.len() as u32, n as u32, n as u32]);
        for k in 0..n as u32 {
            dir.push(next + k);
        }
        next += n as u32;
    }
    dir.push(0);
    let mut out = ssmg::MAGIC.to_vec();
    out.extend_from_slice(&(subs.len() as u32).to_be_bytes());
    out.extend_from_slice(&(BS as u32).to_be_bytes());
    out.extend_from_slice(&next.to_be_bytes());
    out.extend_from_slice(&blocks);
    for v in dir {
        out.extend_from_slice(&v.to_be_bytes());
    }
    out
}

/// A single-piece `/DocumentText` (`TextV.01`) opened for editing.
#[derive(Debug, Clone)]
pub struct TextV {
    pub units: Vec<u16>,
    pub style: Vec<u8>,
    /// Sub-streams after the first (kept verbatim).
    pub rest: Vec<Vec<u8>>,
}

impl TextV {
    pub fn parse(raw: &[u8]) -> Result<TextV> {
        let subs = ssmg::substreams(raw)?;
        let head = &subs[0];
        if !head.starts_with(b"TextV.01") {
            return Err(Error::Unsupported(
                "only single-piece (TextV.01) documents can be written for now".into(),
            ));
        }
        let n = u32::from_be_bytes([head[8], head[9], head[10], head[11]]) as usize;
        let end = 12 + n * 2;
        let units = head[12..end]
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        Ok(TextV {
            units,
            style: head[end..].to_vec(),
            rest: subs[1..].to_vec(),
        })
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut head = b"TextV.01".to_vec();
        head.extend_from_slice(&(self.units.len() as u32).to_be_bytes());
        for u in &self.units {
            head.extend_from_slice(&u.to_be_bytes());
        }
        head.extend_from_slice(&self.style);
        let mut subs = vec![head];
        subs.extend(self.rest.iter().cloned());
        ssmg_pack(&subs)
    }

    /// Insert text units at `pos`; the new units take the style in effect there.
    pub fn insert(&mut self, pos: usize, new: &[u16]) {
        let k = new.len() as u32;
        self.units.splice(pos..pos, new.iter().copied());
        let (events, used) = parse_events(&self.style);
        let tail = self.style[used..].to_vec();
        let mut out: Vec<u8> = Vec::new();
        let mut cur = 0usize;
        let mut done = false;
        for e in events {
            match e {
                StyleEvent::Run(n) => {
                    let n2 = if !done && pos >= cur && pos <= cur + n as usize {
                        done = true;
                        n + k
                    } else {
                        n
                    };
                    cur += n as usize;
                    out.push(0x00);
                    out.extend_from_slice(&n2.to_be_bytes());
                }
                StyleEvent::Set(props) => {
                    if !done && pos == cur {
                        // new units go before this change, in the previous state
                        out.push(0x00);
                        out.extend_from_slice(&k.to_be_bytes());
                        done = true;
                    }
                    out.push(0xfe);
                    for (id, v) in props {
                        out.push(id);
                        out.push(v.len() as u8);
                        out.extend_from_slice(&v);
                    }
                    out.extend_from_slice(&[0xff, 0x00]);
                    cur += 1;
                }
                StyleEvent::End => {
                    if !done {
                        out.push(0x00);
                        out.extend_from_slice(&k.to_be_bytes());
                        done = true;
                    }
                    out.push(0xff);
                }
            }
        }
        if !done {
            out.push(0x00);
            out.extend_from_slice(&k.to_be_bytes());
        }
        out.extend_from_slice(&tail);
        self.style = out;
    }

    /// First position inside a plain text run (after a 0x001F) — a safe place to edit.
    pub fn first_text_pos(&self) -> Option<usize> {
        let toks = crate::text::tokenize(&self.units);
        toks.iter().find_map(|t| match t {
            crate::text::Token::Text { start, text }
                if text.chars().any(|c| !c.is_whitespace() && c != '\u{3000}') =>
            {
                Some(*start)
            }
            _ => None,
        })
    }

    /// Sum of style coverage (runs + change events); must equal units.len().
    pub fn style_coverage(&self) -> usize {
        let (ev, _) = parse_events(&self.style);
        ev.iter()
            .map(|e| match e {
                StyleEvent::Run(n) => *n as usize,
                StyleEvent::Set(_) => 1,
                StyleEvent::End => 0,
            })
            .sum()
    }
}
