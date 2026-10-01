//! Compound File Binary (CFB / OLE2) reader.
//!
//! Ichitaro documents, like HWP 5.0 and legacy Word `.doc`, are CFB files:
//! one file that holds a small file system of named streams and storages.
//!
//! This reader is deliberately *lenient*. Some real-world `.jtd` files carry
//! FAT inconsistencies (duplicate or looping sector pointers) that strict
//! readers reject. We follow chains with a visited set, stop at the first
//! invalid pointer and keep whatever was readable.

use crate::error::{Error, Result};
use std::collections::HashSet;

pub const SIGNATURE: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];

const FREESECT: u32 = 0xFFFF_FFFF;
const ENDOFCHAIN: u32 = 0xFFFF_FFFE;
const NOSTREAM: u32 = 0xFFFF_FFFF;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryKind {
    Root,
    Storage,
    Stream,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Entry {
    /// Full path such as `/ObjectSheets/DocSheet/DOCS_0000/DocumentText`.
    pub path: String,
    /// Raw entry name (may contain control characters such as `\u{5}`).
    pub name: String,
    pub kind: EntryKind,
    pub size: u64,
    #[serde(skip)]
    start: u32,
}

impl Entry {
    /// Name with control characters escaped (`\x05SummaryInformation`).
    pub fn display_path(&self) -> String {
        escape_controls(&self.path)
    }
}

pub fn escape_controls(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if (c as u32) < 0x20 {
            out.push_str(&format!("\\x{:02x}", c as u32));
        } else {
            out.push(c);
        }
    }
    out
}

struct RawDir {
    name: String,
    kind: u8,
    left: u32,
    right: u32,
    child: u32,
    start: u32,
    size: u64,
}

pub struct Cfb {
    data: Vec<u8>,
    sector_shift: u32,
    mini_sector_size: usize,
    mini_cutoff: u64,
    fat: Vec<u32>,
    minifat: Vec<u32>,
    mini_stream: Vec<u8>,
    entries: Vec<Entry>,
    /// Problems that were tolerated while opening.
    pub warnings: Vec<String>,
}

fn u16le(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn u32le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
fn u64le(b: &[u8], o: usize) -> u64 {
    let mut a = [0u8; 8];
    a.copy_from_slice(&b[o..o + 8]);
    u64::from_le_bytes(a)
}

impl Cfb {
    pub fn is_cfb(bytes: &[u8]) -> bool {
        bytes.len() >= 512 && bytes[..8] == SIGNATURE
    }

    pub fn open(data: Vec<u8>) -> Result<Self> {
        if !Self::is_cfb(&data) {
            return Err(Error::NotCfb);
        }
        let major = u16le(&data, 26);
        let sector_shift = u16le(&data, 30) as u32;
        let mini_shift = u16le(&data, 32) as u32;
        if !(sector_shift == 9 || sector_shift == 12) || mini_shift > 12 {
            return Err(Error::Corrupt(format!(
                "unsupported sector shift {sector_shift}/{mini_shift} (v{major})"
            )));
        }
        let num_fat = u32le(&data, 44) as usize;
        let first_dir = u32le(&data, 48);
        let mini_cutoff = u32le(&data, 56) as u64;
        let first_minifat = u32le(&data, 60);
        let first_difat = u32le(&data, 68);
        let num_difat = u32le(&data, 72) as usize;

        let mut cfb = Cfb {
            data,
            sector_shift,
            mini_sector_size: 1usize << mini_shift,
            mini_cutoff,
            fat: Vec::new(),
            minifat: Vec::new(),
            mini_stream: Vec::new(),
            entries: Vec::new(),
            warnings: Vec::new(),
        };

        // --- DIFAT: list of FAT sector numbers ---
        let mut fat_sectors: Vec<u32> = (0..109)
            .map(|i| u32le(&cfb.data, 76 + i * 4))
            .filter(|&s| s < 0xFFFF_FFFA)
            .collect();
        let mut difat = first_difat;
        let mut seen = HashSet::new();
        let per = cfb.sector_size() / 4;
        for _ in 0..num_difat {
            if difat >= 0xFFFF_FFFA || !seen.insert(difat) {
                break;
            }
            let Some(sec) = cfb.sector(difat) else {
                cfb.warnings
                    .push(format!("DIFAT sector {difat} out of range"));
                break;
            };
            let sec = sec.to_vec();
            for i in 0..per - 1 {
                let s = u32le(&sec, i * 4);
                if s < 0xFFFF_FFFA {
                    fat_sectors.push(s);
                }
            }
            difat = u32le(&sec, (per - 1) * 4);
        }
        if fat_sectors.len() > num_fat && num_fat > 0 {
            fat_sectors.truncate(num_fat);
        }

        // --- FAT ---
        let mut fat = Vec::with_capacity(fat_sectors.len() * per);
        for &s in &fat_sectors {
            match cfb.sector(s) {
                Some(sec) => {
                    for i in 0..per {
                        fat.push(u32le(sec, i * 4));
                    }
                }
                None => cfb.warnings.push(format!("FAT sector {s} out of range")),
            }
        }
        cfb.fat = fat;

        // --- Directory ---
        let dir_bytes = cfb.read_chain(first_dir, None, false);
        let mut raw = Vec::new();
        for chunk in dir_bytes.chunks_exact(128) {
            let name_len = (u16le(chunk, 64) as usize).min(64);
            let units: Vec<u16> = (0..name_len.saturating_sub(2) / 2)
                .map(|i| u16le(chunk, i * 2))
                .collect();
            let size = if major == 3 {
                u32le(chunk, 120) as u64
            } else {
                u64le(chunk, 120)
            };
            raw.push(RawDir {
                name: String::from_utf16_lossy(&units),
                kind: chunk[66],
                left: u32le(chunk, 68),
                right: u32le(chunk, 72),
                child: u32le(chunk, 76),
                start: u32le(chunk, 116),
                size,
            });
        }
        if raw.is_empty() || raw[0].kind != 5 {
            return Err(Error::Corrupt("missing root directory entry".into()));
        }

        // --- MiniFAT and mini stream ---
        let minifat_bytes = cfb.read_chain(first_minifat, None, false);
        cfb.minifat = minifat_bytes.chunks_exact(4).map(|c| u32le(c, 0)).collect();
        cfb.mini_stream = cfb.read_chain(raw[0].start, Some(raw[0].size), false);

        // --- Walk the red-black tree into paths ---
        let mut entries = vec![Entry {
            path: "/".into(),
            name: raw[0].name.clone(),
            kind: EntryKind::Root,
            size: raw[0].size,
            start: raw[0].start,
        }];
        let mut visited = HashSet::new();
        visited.insert(0u32);
        walk(
            &raw,
            raw[0].child,
            "",
            &mut entries,
            &mut visited,
            &mut cfb.warnings,
            0,
        );
        cfb.entries = entries;
        Ok(cfb)
    }

    fn sector_size(&self) -> usize {
        1usize << self.sector_shift
    }

    fn sector(&self, n: u32) -> Option<&[u8]> {
        let size = self.sector_size();
        let off = (n as usize + 1).checked_mul(size)?;
        if off >= self.data.len() {
            return None;
        }
        let end = (off + size).min(self.data.len());
        Some(&self.data[off..end])
    }

    fn read_chain(&self, start: u32, size: Option<u64>, mini: bool) -> Vec<u8> {
        let mut out = Vec::new();
        let mut cur = start;
        let mut seen = HashSet::new();
        let limit = size.map(|s| s as usize);
        while cur != ENDOFCHAIN && cur != FREESECT && cur < 0xFFFF_FFFA {
            if !seen.insert(cur) {
                break; // loop in the chain: stop leniently
            }
            if mini {
                let ss = self.mini_sector_size;
                let off = cur as usize * ss;
                if off >= self.mini_stream.len() {
                    break;
                }
                let end = (off + ss).min(self.mini_stream.len());
                out.extend_from_slice(&self.mini_stream[off..end]);
                cur = *self.minifat.get(cur as usize).unwrap_or(&ENDOFCHAIN);
            } else {
                let Some(sec) = self.sector(cur) else { break };
                out.extend_from_slice(sec);
                cur = *self.fat.get(cur as usize).unwrap_or(&ENDOFCHAIN);
            }
            if let Some(l) = limit {
                if out.len() >= l {
                    break;
                }
            }
        }
        if let Some(l) = limit {
            out.truncate(l);
        }
        out
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn streams(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter().filter(|e| e.kind == EntryKind::Stream)
    }

    pub fn find(&self, path: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.path == path)
    }

    pub fn read_entry(&self, e: &Entry) -> Vec<u8> {
        if e.kind != EntryKind::Stream {
            return Vec::new();
        }
        let mini = e.size < self.mini_cutoff;
        self.read_chain(e.start, Some(e.size), mini)
    }

    pub fn read(&self, path: &str) -> Option<Vec<u8>> {
        self.find(path).map(|e| self.read_entry(e))
    }

    /// Children of a storage path (direct only).
    pub fn children<'a>(&'a self, storage: &'a str) -> impl Iterator<Item = &'a Entry> + 'a {
        let prefix = if storage == "/" {
            String::from("/")
        } else {
            format!("{storage}/")
        };
        self.entries.iter().filter(move |e| {
            e.path.len() > prefix.len()
                && e.path.starts_with(&prefix)
                && !e.path[prefix.len()..].contains('/')
        })
    }
}

fn walk(
    raw: &[RawDir],
    id: u32,
    parent: &str,
    out: &mut Vec<Entry>,
    visited: &mut HashSet<u32>,
    warnings: &mut Vec<String>,
    depth: usize,
) {
    if id == NOSTREAM || depth > 64 {
        return;
    }
    let Some(d) = raw.get(id as usize) else {
        warnings.push(format!("directory id {id} out of range"));
        return;
    };
    if !visited.insert(id) {
        warnings.push(format!("directory loop at id {id}"));
        return;
    }
    walk(raw, d.left, parent, out, visited, warnings, depth + 1);
    let path = format!("{parent}/{}", d.name);
    let kind = match d.kind {
        1 => Some(EntryKind::Storage),
        2 => Some(EntryKind::Stream),
        _ => None,
    };
    if let Some(kind) = kind {
        out.push(Entry {
            path: path.clone(),
            name: d.name.clone(),
            kind,
            size: d.size,
            start: d.start,
        });
        if kind == EntryKind::Storage {
            walk(raw, d.child, &path, out, visited, warnings, depth + 1);
        }
    }
    walk(raw, d.right, parent, out, visited, warnings, depth + 1);
}
