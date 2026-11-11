//! LHA `-lh5-` decoding (LZ77 with an 8 KiB window and static Huffman blocks).
//!
//! Compressed Ichitaro files (`.jttc`) keep the real document in the stream
//! `/JSCompDocument`, packed the way the LHA archiver packs a file. This is a
//! small, self-contained decoder of that method (also `-lh4-`, `-lh6-`,
//! `-lh7-`, which differ only in the window size), plus a reader for the
//! LHA member header in front of the data.
//!
//! Codes are canonical: shorter codes first, and within one length in symbol
//! order (the same rule as the original `make_table`), so a symbol can be read
//! one bit at a time from the count of codes of each length.

use crate::error::{Error, Result};

const NC: usize = 510; // 256 literals + lengths 3..=256
const NT: usize = 19;
const TBIT: u32 = 5;
const CBIT: u32 = 9;

struct Bits<'a> {
    b: &'a [u8],
    pos: usize, // bit position
}

impl<'a> Bits<'a> {
    fn get(&mut self, n: u32) -> Result<u32> {
        let mut v = 0u32;
        for _ in 0..n {
            v = (v << 1) | self.bit()?;
        }
        Ok(v)
    }
    fn bit(&mut self) -> Result<u32> {
        let byte = self.pos >> 3;
        // LHA reads past the end as zeros near the end of a member; allow a few bytes of slack
        let v = if byte < self.b.len() {
            (self.b[byte] >> (7 - (self.pos & 7))) & 1
        } else if byte < self.b.len() + 8 {
            0
        } else {
            return Err(Error::Corrupt("compressed data ends early".into()));
        };
        self.pos += 1;
        Ok(v as u32)
    }
    /// The next `n` bits without consuming them (missing bits read as 0).
    fn peek(&self, n: u32) -> u32 {
        let mut v = 0u32;
        for k in 0..n as usize {
            let p = self.pos + k;
            let byte = p >> 3;
            let bit = if byte < self.b.len() { (self.b[byte] >> (7 - (p & 7))) & 1 } else { 0 };
            v = (v << 1) | bit as u32;
        }
        v
    }
}

/// A canonical Huffman code. `single` is set when the block uses one symbol
/// only (it is then read with no bits).
struct Code {
    count: [u16; 17],
    symbols: Vec<u16>,
    single: Option<u16>,
}

impl Code {
    fn single(sym: u16) -> Code {
        Code { count: [0; 17], symbols: Vec::new(), single: Some(sym) }
    }
    fn from_lengths(len: &[u8]) -> Result<Code> {
        let mut count = [0u16; 17];
        for &l in len {
            if l > 16 {
                return Err(Error::Corrupt("code length over 16".into()));
            }
            count[l as usize] += 1;
        }
        count[0] = 0;
        // over-subscribed codes are damage
        let mut left: i32 = 1;
        for l in 1..=16 {
            left = (left << 1) - count[l] as i32;
            if left < 0 {
                return Err(Error::Corrupt("bad Huffman table".into()));
            }
        }
        let mut offs = [0u16; 17];
        for l in 1..16 {
            offs[l + 1] = offs[l] + count[l];
        }
        let mut symbols = vec![0u16; len.iter().filter(|&&l| l > 0).count()];
        for (s, &l) in len.iter().enumerate() {
            if l > 0 {
                symbols[offs[l as usize] as usize] = s as u16;
                offs[l as usize] += 1;
            }
        }
        Ok(Code { count, symbols, single: None })
    }
    fn read(&self, bits: &mut Bits) -> Result<u16> {
        if let Some(s) = self.single {
            return Ok(s);
        }
        let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
        for l in 1..=16 {
            code |= bits.bit()? as i32;
            let c = self.count[l] as i32;
            if code - c < first {
                return self
                    .symbols
                    .get((index + (code - first)) as usize)
                    .copied()
                    .ok_or_else(|| Error::Corrupt("bad Huffman code".into()));
            }
            index += c;
            first += c;
            first <<= 1;
            code <<= 1;
        }
        Err(Error::Corrupt("bad Huffman code".into()))
    }
}

fn read_pt_len(bits: &mut Bits, nn: usize, nbit: u32, special: Option<usize>) -> Result<Code> {
    let n = bits.get(nbit)? as usize;
    if n == 0 {
        let c = bits.get(nbit)? as usize;
        if c >= nn {
            return Err(Error::Corrupt("bad table".into()));
        }
        return Ok(Code::single(c as u16));
    }
    if n > nn {
        return Err(Error::Corrupt("bad table size".into()));
    }
    let mut len = vec![0u8; nn];
    let mut i = 0;
    while i < n {
        let mut c = bits.peek(3);
        if c == 7 {
            // 111 then a run of 1s ended by a 0: 7 + the number of extra 1s
            let mut k = 3;
            while bits.peek(k + 1) & 1 == 1 {
                c += 1;
                k += 1;
                if c > 16 {
                    return Err(Error::Corrupt("bad code length".into()));
                }
            }
            bits.get(c - 3)?;
        } else {
            bits.get(3)?;
        }
        len[i] = c as u8;
        i += 1;
        if Some(i) == special {
            let z = bits.get(2)? as usize;
            for _ in 0..z {
                if i < nn {
                    len[i] = 0;
                    i += 1;
                }
            }
        }
    }
    Code::from_lengths(&len)
}

fn read_c_len(bits: &mut Bits, pt: &Code) -> Result<Code> {
    let n = bits.get(CBIT)? as usize;
    if n == 0 {
        let c = bits.get(CBIT)? as usize;
        if c >= NC {
            return Err(Error::Corrupt("bad table".into()));
        }
        return Ok(Code::single(c as u16));
    }
    if n > NC {
        return Err(Error::Corrupt("bad table size".into()));
    }
    let mut len = vec![0u8; NC];
    let mut i = 0;
    while i < n {
        let c = pt.read(bits)? as usize;
        if c <= 2 {
            let run = match c {
                0 => 1,
                1 => bits.get(4)? as usize + 3,
                _ => bits.get(CBIT)? as usize + 20,
            };
            for _ in 0..run {
                if i < NC {
                    len[i] = 0;
                    i += 1;
                }
            }
        } else {
            len[i] = (c - 2) as u8;
            i += 1;
        }
    }
    Code::from_lengths(&len)
}

/// Decode `out_len` bytes of `-lhN-` data. `dicbit` is 12 (lh4), 13 (lh5),
/// 15 (lh6) or 16 (lh7).
pub fn decode(data: &[u8], out_len: usize, dicbit: u32) -> Result<Vec<u8>> {
    let np = dicbit as usize + 1;
    let pbit = if dicbit <= 13 { 4 } else { 5 };
    let mut bits = Bits { b: data, pos: 0 };
    let mut out: Vec<u8> = Vec::with_capacity(out_len);
    let mut block = 0u32;
    let (mut c_code, mut p_code) = (Code::single(0), Code::single(0));
    while out.len() < out_len {
        if block == 0 {
            block = bits.get(16)?;
            if block == 0 {
                return Err(Error::Corrupt("empty compressed block".into()));
            }
            let pt = read_pt_len(&mut bits, NT, TBIT, Some(3))?;
            c_code = read_c_len(&mut bits, &pt)?;
            p_code = read_pt_len(&mut bits, np, pbit, None)?;
        }
        block -= 1;
        let c = c_code.read(&mut bits)? as usize;
        if c < 256 {
            out.push(c as u8);
            continue;
        }
        let len = c - 253;
        let j = p_code.read(&mut bits)? as u32;
        let dist = if j == 0 { 0 } else { (1u32 << (j - 1)) + bits.get(j - 1)? } as usize;
        let from = out
            .len()
            .checked_sub(dist + 1)
            .ok_or_else(|| Error::Corrupt("copy before the start".into()))?;
        for k in 0..len {
            if out.len() >= out_len {
                break;
            }
            let b = out[from + k];
            out.push(b);
        }
    }
    Ok(out)
}

/// One LHA member found in a byte string.
pub struct Member {
    pub method: [u8; 5],
    pub original_size: usize,
    /// Where the packed data starts and how long it is.
    pub data: std::ops::Range<usize>,
}

fn le16(b: &[u8], o: usize) -> Option<usize> {
    Some(u16::from_le_bytes([*b.get(o)?, *b.get(o + 1)?]) as usize)
}
fn le32(b: &[u8], o: usize) -> Option<usize> {
    Some(u32::from_le_bytes([*b.get(o)?, *b.get(o + 1)?, *b.get(o + 2)?, *b.get(o + 3)?]) as usize)
}

/// Read the LHA header that starts at `at` (the `-lhN-` method id is at `at + 2`).
pub fn member_at(b: &[u8], at: usize) -> Option<Member> {
    let m = b.get(at + 2..at + 7)?;
    if &m[..3] != b"-lh" || m[4] != b'-' {
        return None;
    }
    let method = [m[0], m[1], m[2], m[3], m[4]];
    let mut packed = le32(b, at + 7)?;
    let original_size = le32(b, at + 11)?;
    let level = *b.get(at + 20)?;
    let start = match level {
        0 => at + 2 + *b.get(at)? as usize,
        1 => {
            // level 1: the extended headers after the base header count in the packed size
            let mut p = at + 2 + *b.get(at)? as usize;
            let mut next = le16(b, p - 2)?;
            while next > 0 {
                packed = packed.checked_sub(next)?;
                let n = le16(b, p + next - 2)?;
                p += next;
                next = n;
            }
            p
        }
        2 => at + le16(b, at)?,
        3 => at + le32(b, at + 24)?,
        _ => return None,
    };
    let end = start.checked_add(packed)?;
    if end > b.len() + 8 || original_size == 0 {
        return None;
    }
    Some(Member { method, original_size, data: start..end.min(b.len()) })
}

/// Find the first LHA member in `b` and unpack it.
pub fn unpack_first(b: &[u8], max_out: usize) -> Result<Option<Vec<u8>>> {
    let mut i = 0;
    while i + 21 < b.len() {
        if b[i + 2] == b'-' && b[i + 3] == b'l' && b[i + 4] == b'h' && b[i + 6] == b'-' {
            if let Some(m) = member_at(b, i) {
                if m.original_size > max_out {
                    return Err(Error::TooLarge(format!(
                        "compressed document unpacks to {} bytes",
                        m.original_size
                    )));
                }
                let data = &b[m.data.clone()];
                let out = match m.method[3] {
                    b'0' => data.get(..m.original_size).map(|d| d.to_vec()),
                    b'4' => Some(decode(data, m.original_size, 12)?),
                    b'5' => Some(decode(data, m.original_size, 13)?),
                    b'6' => Some(decode(data, m.original_size, 15)?),
                    b'7' => Some(decode(data, m.original_size, 16)?),
                    _ => None,
                };
                if out.is_some() {
                    return Ok(out);
                }
            }
        }
        i += 1;
    }
    Ok(None)
}
