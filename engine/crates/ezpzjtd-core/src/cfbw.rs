//! Compound File Binary writer (version 3, 512-byte sectors).
//!
//! Writes a tree of storages and streams. Directory siblings are stored as a
//! valid red-black tree (balanced, deepest incomplete level red), because
//! Windows Structured Storage (which Ichitaro uses) can be strict about it.
//! Class ids, state bits and timestamps can be carried over from the
//! original file so a re-saved document looks the same to its owner.

use crate::cfb::{Cfb, EntryKind};

const SECTOR: usize = 512;
const MINI: usize = 64;
const CUTOFF: usize = 4096;
const FREESECT: u32 = 0xFFFF_FFFF;
const ENDOFCHAIN: u32 = 0xFFFF_FFFE;
const FATSECT: u32 = 0xFFFF_FFFD;
const NOSTREAM: u32 = 0xFFFF_FFFF;

#[derive(Debug, Clone, Default)]
pub struct Meta {
    pub clsid: [u8; 16],
    pub state: u32,
    pub ctime: u64,
    pub mtime: u64,
}

#[derive(Debug, Clone)]
pub enum Node {
    Storage {
        name: String,
        meta: Meta,
        children: Vec<Node>,
    },
    Stream {
        name: String,
        meta: Meta,
        data: Vec<u8>,
    },
}

impl Node {
    pub fn name(&self) -> &str {
        match self {
            Node::Storage { name, .. } | Node::Stream { name, .. } => name,
        }
    }
}

/// The whole file: root metadata + top-level children.
#[derive(Debug, Clone, Default)]
pub struct Tree {
    pub root: Meta,
    pub children: Vec<Node>,
}

impl Tree {
    /// Rebuild the tree of an existing file (all streams read into memory).
    pub fn from_cfb(c: &Cfb) -> Tree {
        let root = c
            .entries()
            .iter()
            .find(|e| e.kind == EntryKind::Root)
            .map(|e| Meta {
                clsid: e.clsid,
                state: e.state,
                ctime: e.ctime,
                mtime: e.mtime,
            })
            .unwrap_or_default();
        fn build(c: &Cfb, parent: &str) -> Vec<Node> {
            let mut out = Vec::new();
            for e in c.children(parent) {
                let meta = Meta {
                    clsid: e.clsid,
                    state: e.state,
                    ctime: e.ctime,
                    mtime: e.mtime,
                };
                match e.kind {
                    EntryKind::Storage => out.push(Node::Storage {
                        name: e.name.clone(),
                        meta,
                        children: build(c, &e.path),
                    }),
                    EntryKind::Stream => out.push(Node::Stream {
                        name: e.name.clone(),
                        meta,
                        data: c.read_entry(e),
                    }),
                    EntryKind::Root => {}
                }
            }
            out
        }
        Tree {
            root,
            children: build(c, "/"),
        }
    }

    fn find_mut<'a>(nodes: &'a mut Vec<Node>, parts: &[&str]) -> Option<&'a mut Node> {
        let (first, rest) = parts.split_first()?;
        let n = nodes.iter_mut().find(|n| n.name() == *first)?;
        if rest.is_empty() {
            return Some(n);
        }
        match n {
            Node::Storage { children, .. } => Self::find_mut(children, rest),
            _ => None,
        }
    }

    /// Replace the bytes of a stream such as `/DocumentText`. Returns false if missing.
    pub fn set_stream(&mut self, path: &str, data: Vec<u8>) -> bool {
        let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
        match Self::find_mut(&mut self.children, &parts) {
            Some(Node::Stream { data: d, .. }) => {
                *d = data;
                true
            }
            _ => false,
        }
    }

    /// Remove a stream or storage. Returns false if missing.
    pub fn remove(&mut self, path: &str) -> bool {
        let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
        let (last, dir) = parts.split_last().unwrap();
        let list = if dir.is_empty() {
            &mut self.children
        } else {
            match Self::find_mut(&mut self.children, dir) {
                Some(Node::Storage { children, .. }) => children,
                _ => return false,
            }
        };
        let before = list.len();
        list.retain(|n| n.name() != *last);
        list.len() != before
    }
}

/// CFB name order: shorter names first, then case-insensitive by UTF-16 unit.
fn cmp_names(a: &str, b: &str) -> std::cmp::Ordering {
    let ua: Vec<u16> = a.encode_utf16().collect();
    let ub: Vec<u16> = b.encode_utf16().collect();
    ua.len().cmp(&ub.len()).then_with(|| {
        let up = |u: &u16| -> u16 {
            if (0x61..=0x7a).contains(u) {
                u - 0x20
            } else {
                *u
            }
        };
        ua.iter().map(up).cmp(ub.iter().map(up))
    })
}

struct DirEntry {
    name: String,
    kind: u8, // 1 storage, 2 stream, 5 root
    color: u8,
    left: u32,
    right: u32,
    child: u32,
    meta: Meta,
    start: u32,
    size: u64,
}

/// Build a balanced red-black tree over `ids` (already sorted). Returns the root id.
fn rb_tree(dir: &mut [DirEntry], ids: &[u32]) -> u32 {
    if ids.is_empty() {
        return NOSTREAM;
    }
    // depth of a perfect tree that fits inside n nodes
    let n = ids.len();
    let mut full = 0usize;
    while (1usize << (full + 1)) - 1 <= n {
        full += 1;
    }
    fn go(dir: &mut [DirEntry], ids: &[u32], depth: usize, full: usize) -> u32 {
        if ids.is_empty() {
            return NOSTREAM;
        }
        let mid = ids.len() / 2;
        let id = ids[mid];
        let l = go(dir, &ids[..mid], depth + 1, full);
        let r = go(dir, &ids[mid + 1..], depth + 1, full);
        let e = &mut dir[id as usize];
        e.left = l;
        e.right = r;
        e.color = if depth >= full { 0 } else { 1 }; // 0 red, 1 black
        id
    }
    go(dir, ids, 0, full)
}

/// Name of JustSystems' per-storage directory stream.
pub const SEGMENT_INFO: &str = "\u{4}JSRV_SegmentInformation";

impl Tree {
    /// Bring every `\x04JSRV_SegmentInformation` up to date: Ichitaro keeps,
    /// per storage, a table of its children with their byte sizes and
    /// refuses a file whose sizes do not match. Entries of removed children
    /// are dropped; sizes are rewritten.
    ///
    /// Layout: `"VDA_DOC\0"`, …, u16 LE first-entry offset at 14, u16 LE
    /// entry size at 16, u16 LE entry count at 18; each entry is the child
    /// name (UTF-16LE, 64 bytes), u32 LE 0, u32 LE size, u32 LE kind, padding.
    pub fn fix_segment_info(&mut self) {
        fn fix(children: &mut Vec<Node>) {
            for c in children.iter_mut() {
                if let Node::Storage { children, .. } = c {
                    fix(children);
                }
            }
            let sizes: Vec<(String, Option<u32>)> = children
                .iter()
                .map(|c| match c {
                    Node::Stream { name, data, .. } => (name.clone(), Some(data.len() as u32)),
                    Node::Storage { name, .. } => (name.clone(), None),
                })
                .collect();
            let Some(Node::Stream { data, .. }) =
                children.iter_mut().find(|c| c.name() == SEGMENT_INFO)
            else {
                return;
            };
            if let Some(new) = rewrite_segment_info(data, &sizes) {
                *data = new;
            }
        }
        fix(&mut self.children);
    }
}

fn rewrite_segment_info(b: &[u8], children: &[(String, Option<u32>)]) -> Option<Vec<u8>> {
    if b.len() < 20 || &b[..8] != b"VDA_DOC\0" {
        return None;
    }
    let u16le = |o: usize| u16::from_le_bytes([b[o], b[o + 1]]) as usize;
    let (first, size, count) = (u16le(14), u16le(16), u16le(18));
    if size < 76 || first + size * count > b.len() {
        return None;
    }
    let mut out = b[..first].to_vec();
    let mut kept = 0u16;
    for k in 0..count {
        let e = &b[first + k * size..first + (k + 1) * size];
        let units: Vec<u16> = e[..64]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        let n = units.iter().position(|&u| u == 0).unwrap_or(units.len());
        let name = String::from_utf16_lossy(&units[..n]);
        let Some((_, sz)) = children.iter().find(|(c, _)| *c == name) else {
            continue; // the child is gone
        };
        let mut e = e.to_vec();
        if let Some(sz) = sz {
            e[68..72].copy_from_slice(&sz.to_le_bytes());
        }
        out.extend_from_slice(&e);
        kept += 1;
    }
    out[18..20].copy_from_slice(&kept.to_le_bytes());
    out.extend_from_slice(&b[(first + size * count).min(b.len())..]);
    Some(out)
}

pub fn write(tree: &Tree) -> Vec<u8> {
    // ---- flatten directory (root first) and collect stream data
    let mut dir: Vec<DirEntry> = vec![DirEntry {
        name: "Root Entry".into(),
        kind: 5,
        color: 1,
        left: NOSTREAM,
        right: NOSTREAM,
        child: NOSTREAM,
        meta: tree.root.clone(),
        start: ENDOFCHAIN,
        size: 0,
    }];
    let mut datas: Vec<Option<Vec<u8>>> = vec![None];
    fn add(nodes: &[Node], dir: &mut Vec<DirEntry>, datas: &mut Vec<Option<Vec<u8>>>) -> Vec<u32> {
        let mut sorted: Vec<&Node> = nodes.iter().collect();
        sorted.sort_by(|a, b| cmp_names(a.name(), b.name()));
        let mut ids = Vec::new();
        for n in sorted {
            let id = dir.len() as u32;
            ids.push(id);
            match n {
                Node::Stream { name, meta, data } => {
                    dir.push(DirEntry {
                        name: name.clone(),
                        kind: 2,
                        color: 1,
                        left: NOSTREAM,
                        right: NOSTREAM,
                        child: NOSTREAM,
                        meta: meta.clone(),
                        start: ENDOFCHAIN,
                        size: data.len() as u64,
                    });
                    datas.push(Some(data.clone()));
                }
                Node::Storage {
                    name,
                    meta,
                    children,
                } => {
                    dir.push(DirEntry {
                        name: name.clone(),
                        kind: 1,
                        color: 1,
                        left: NOSTREAM,
                        right: NOSTREAM,
                        child: NOSTREAM,
                        meta: meta.clone(),
                        start: 0,
                        size: 0,
                    });
                    datas.push(None);
                    let kids = add(children, dir, datas);
                    let root = rb_tree(dir, &kids);
                    dir[id as usize].child = root;
                }
            }
        }
        ids
    }
    let top = add(&tree.children, &mut dir, &mut datas);
    let root_child = rb_tree(&mut dir, &top);
    dir[0].child = root_child;
    dir[0].color = 1;

    // ---- mini stream for small streams
    let mut mini_stream: Vec<u8> = Vec::new();
    let mut minifat: Vec<u32> = Vec::new();
    for (i, d) in datas.iter().enumerate() {
        let Some(data) = d else { continue };
        if data.len() >= CUTOFF {
            continue;
        }
        if data.is_empty() {
            dir[i].start = ENDOFCHAIN;
            continue;
        }
        let first = (mini_stream.len() / MINI) as u32;
        let n = data.len().div_ceil(MINI);
        for k in 0..n {
            minifat.push(if k + 1 == n {
                ENDOFCHAIN
            } else {
                first + k as u32 + 1
            });
        }
        dir[i].start = first;
        mini_stream.extend_from_slice(data);
        mini_stream.resize(mini_stream.len().div_ceil(MINI) * MINI, 0);
    }

    // ---- lay out sectors: [big streams][mini stream][minifat][directory][fat]
    let mut sectors: Vec<Vec<u8>> = Vec::new(); // data of each regular sector
    let mut fat: Vec<u32> = Vec::new();
    let place = |bytes: &[u8], sectors: &mut Vec<Vec<u8>>, fat: &mut Vec<u32>| -> u32 {
        if bytes.is_empty() {
            return ENDOFCHAIN;
        }
        let first = sectors.len() as u32;
        let n = bytes.len().div_ceil(SECTOR);
        for k in 0..n {
            let mut s = bytes[k * SECTOR..((k + 1) * SECTOR).min(bytes.len())].to_vec();
            s.resize(SECTOR, 0);
            sectors.push(s);
            fat.push(if k + 1 == n {
                ENDOFCHAIN
            } else {
                first + k as u32 + 1
            });
        }
        first
    };
    for (i, d) in datas.iter().enumerate() {
        if let Some(data) = d {
            if data.len() >= CUTOFF {
                dir[i].start = place(data, &mut sectors, &mut fat);
            }
        }
    }
    dir[0].start = place(&mini_stream, &mut sectors, &mut fat);
    dir[0].size = mini_stream.len() as u64;
    let mut minifat_bytes: Vec<u8> = minifat.iter().flat_map(|v| v.to_le_bytes()).collect();
    minifat_bytes.resize(minifat_bytes.len().div_ceil(SECTOR) * SECTOR, 0xff);
    let minifat_start = place(&minifat_bytes, &mut sectors, &mut fat);
    let minifat_count = minifat_bytes.len() / SECTOR;

    let mut dir_bytes = Vec::new();
    for e in &dir {
        let mut b = [0u8; 128];
        let units: Vec<u16> = e.name.encode_utf16().take(31).collect();
        for (k, u) in units.iter().enumerate() {
            b[k * 2..k * 2 + 2].copy_from_slice(&u.to_le_bytes());
        }
        b[64..66].copy_from_slice(&(((units.len() + 1) * 2) as u16).to_le_bytes());
        b[66] = e.kind;
        b[67] = e.color;
        b[68..72].copy_from_slice(&e.left.to_le_bytes());
        b[72..76].copy_from_slice(&e.right.to_le_bytes());
        b[76..80].copy_from_slice(&e.child.to_le_bytes());
        b[80..96].copy_from_slice(&e.meta.clsid);
        b[96..100].copy_from_slice(&e.meta.state.to_le_bytes());
        b[100..108].copy_from_slice(&e.meta.ctime.to_le_bytes());
        b[108..116].copy_from_slice(&e.meta.mtime.to_le_bytes());
        let start = if e.kind == 1 { 0 } else { e.start };
        b[116..120].copy_from_slice(&start.to_le_bytes());
        b[120..124].copy_from_slice(&(e.size as u32).to_le_bytes());
        dir_bytes.extend_from_slice(&b);
    }
    // pad the directory with empty entries
    while dir_bytes.len() % SECTOR != 0 {
        let mut b = [0u8; 128];
        b[68..72].copy_from_slice(&NOSTREAM.to_le_bytes());
        b[72..76].copy_from_slice(&NOSTREAM.to_le_bytes());
        b[76..80].copy_from_slice(&NOSTREAM.to_le_bytes());
        dir_bytes.extend_from_slice(&b);
    }
    let dir_start = place(&dir_bytes, &mut sectors, &mut fat);

    // FAT sectors: each holds 128 entries and must also describe itself
    let mut nfat = 1usize;
    while (sectors.len() + nfat) > nfat * (SECTOR / 4) {
        nfat += 1;
    }
    assert!(
        nfat <= 109,
        "file too large for the simple writer (needs DIFAT)"
    );
    let fat_start = sectors.len() as u32;
    for _ in 0..nfat {
        fat.push(FATSECT);
    }
    fat.resize(nfat * SECTOR / 4, FREESECT);
    for k in 0..nfat {
        let s: Vec<u8> = fat[k * 128..(k + 1) * 128]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        sectors.push(s);
    }

    // ---- header
    let mut h = vec![0u8; SECTOR];
    h[..8].copy_from_slice(&crate::cfb::SIGNATURE);
    h[24..26].copy_from_slice(&0x003Eu16.to_le_bytes()); // minor
    h[26..28].copy_from_slice(&3u16.to_le_bytes()); // major
    h[28..30].copy_from_slice(&0xFFFEu16.to_le_bytes()); // byte order
    h[30..32].copy_from_slice(&9u16.to_le_bytes()); // 512
    h[32..34].copy_from_slice(&6u16.to_le_bytes()); // 64
    h[44..48].copy_from_slice(&(nfat as u32).to_le_bytes());
    h[48..52].copy_from_slice(&dir_start.to_le_bytes());
    h[56..60].copy_from_slice(&(CUTOFF as u32).to_le_bytes());
    h[60..64].copy_from_slice(
        &(if minifat_count == 0 {
            ENDOFCHAIN
        } else {
            minifat_start
        })
        .to_le_bytes(),
    );
    h[64..68].copy_from_slice(&(minifat_count as u32).to_le_bytes());
    h[68..72].copy_from_slice(&ENDOFCHAIN.to_le_bytes());
    h[72..76].copy_from_slice(&0u32.to_le_bytes());
    for k in 0..109 {
        let v = if k < nfat {
            fat_start + k as u32
        } else {
            FREESECT
        };
        h[76 + k * 4..80 + k * 4].copy_from_slice(&v.to_le_bytes());
    }
    let mut out = h;
    for s in sectors {
        out.extend_from_slice(&s);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_small_and_big_streams() {
        let tree = Tree {
            root: Meta::default(),
            children: vec![
                Node::Stream {
                    name: "Small".into(),
                    meta: Meta::default(),
                    data: b"hello".to_vec(),
                },
                Node::Stream {
                    name: "Big".into(),
                    meta: Meta::default(),
                    data: vec![7u8; 10_000],
                },
                Node::Storage {
                    name: "Dir".into(),
                    meta: Meta::default(),
                    children: (0..9)
                        .map(|i| Node::Stream {
                            name: format!("S{i}"),
                            meta: Meta::default(),
                            data: vec![i as u8; 100 * i],
                        })
                        .collect(),
                },
            ],
        };
        let bytes = write(&tree);
        let c = Cfb::open(bytes).unwrap();
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        assert_eq!(c.read("/Small").unwrap(), b"hello");
        assert_eq!(c.read("/Big").unwrap(), vec![7u8; 10_000]);
        for i in 0..9 {
            assert_eq!(
                c.read(&format!("/Dir/S{i}")).unwrap(),
                vec![i as u8; 100 * i]
            );
        }
    }
}
