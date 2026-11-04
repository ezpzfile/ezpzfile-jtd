//! Hostile-input check (robustness): build crafted files from a seed .jtd and
//! report time, peak memory, text units and pages for each.
//! `cargo run --release --example hostile -- seed.jtd [out_dir]`
use ezpzjtd_core::cfb::Cfb;
use ezpzjtd_core::cfbw::{write, Tree};
use std::alloc::{GlobalAlloc, Layout as AL, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

struct Count;
static NOW: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for Count {
    unsafe fn alloc(&self, l: AL) -> *mut u8 {
        let n = NOW.fetch_add(l.size(), Ordering::Relaxed) + l.size();
        PEAK.fetch_max(n, Ordering::Relaxed);
        System.alloc(l)
    }
    unsafe fn dealloc(&self, p: *mut u8, l: AL) {
        NOW.fetch_sub(l.size(), Ordering::Relaxed);
        System.dealloc(p, l)
    }
}
#[global_allocator]
static A: Count = Count;

fn be(v: u32) -> [u8; 4] {
    v.to_be_bytes()
}

/// DocumentText whose one sub-stream lists the same block `n` times.
fn reuse_blocks(block: usize, n: u32) -> Vec<u8> {
    let mut b = b"SsmgV.01".to_vec();
    b.extend(be(1));
    b.extend(be(block as u32));
    b.extend(be(1));
    let mut blk = b"TextV.01".to_vec();
    blk.extend(be(u32::MAX / 4));
    while blk.len() < block {
        blk.extend("あいうえお\u{d}".encode_utf16().flat_map(|u| u.to_be_bytes()));
    }
    blk.truncate(block);
    b.extend(&blk);
    b.extend(be(0));
    b.extend(be(0));
    b.extend(be(u32::MAX));
    b.extend(be(n));
    b.extend(be(n));
    for _ in 0..n {
        b.extend(be(0));
    }
    b.extend(be(0));
    b
}

/// A sub-stream entry that claims a huge block count but has none listed.
fn huge_count() -> Vec<u8> {
    let mut b = b"SsmgV.01".to_vec();
    b.extend(be(1));
    b.extend(be(256));
    b.extend(be(1));
    let mut blk = b"TextV.01".to_vec();
    blk.extend(be(4));
    blk.resize(256, 0);
    b.extend(&blk);
    b.extend(be(0));
    b.extend(be(0));
    b.extend(be(u32::MAX));
    b.extend(be(u32::MAX));
    b.extend(be(u32::MAX));
    b
}

/// DocumentText holding one text sub-stream: `units` then the raw style bytes.
fn text_stream(units: &[u16], style: &[u8]) -> Vec<u8> {
    let mut payload = b"TextV.01".to_vec();
    payload.extend(be(units.len() as u32));
    for u in units {
        payload.extend(u.to_be_bytes());
    }
    payload.extend(style);
    let len = payload.len();
    let nb = len.div_ceil(256);
    payload.resize(nb * 256, 0);
    let mut b = b"SsmgV.01".to_vec();
    b.extend(be(1));
    b.extend(be(256));
    b.extend(be(nb as u32));
    b.extend(&payload);
    b.extend(be(0));
    b.extend(be(0));
    b.extend(be(len as u32));
    b.extend(be(nb as u32));
    b.extend(be(nb as u32));
    for i in 0..nb as u32 {
        b.extend(be(i));
    }
    b.extend(be(0));
    b
}

/// A style "set" event giving every property id 0..=255 a 4-byte value.
fn set_all(v: u32) -> Vec<u8> {
    let mut e = vec![0xfe];
    for id in 0..=254u8 {
        e.extend([id, 4]);
        e.extend(be(v));
    }
    e.extend([0xff, 0x00]);
    e
}

/// Text with paragraphs of `per` characters, styled by `style`.
fn text(chars: usize, per: usize) -> Vec<u16> {
    let mut u = Vec::with_capacity(chars + chars / per + 1);
    for i in 0..chars {
        u.push(0x3042);
        if (i + 1) % per == 0 {
            u.push(0x0a);
        }
    }
    u
}

fn run(name: &str, bytes: Vec<u8>) {
    let len = bytes.len();
    let name = name.to_string();
    let base = NOW.load(Ordering::Relaxed);
    PEAK.store(base, Ordering::Relaxed);
    let t = Instant::now();
    let res = std::panic::catch_unwind(|| match ezpzjtd_core::open(bytes) {
        Ok(doc) => {
            let units: usize = doc.sheets.iter().map(|s| s.blocks.len()).sum();
            let mut ed = ezpzjtd_core::edit::Editor::new(doc);
            let pages = ed.layout().pages.len();
            format!("ok blocks={units} pages={pages}")
        }
        Err(e) => format!("err {e}"),
    });
    let msg = res.unwrap_or_else(|_| "PANIC".into());
    println!(
        "{name:<14} file={:>7}KB  {:>6}ms  peak={:>8}MB  {msg}",
        len / 1024,
        t.elapsed().as_millis(),
        PEAK.load(Ordering::Relaxed).saturating_sub(base) >> 20
    );
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seed = std::fs::read(&args[1]).expect("seed");
    let cfb = Cfb::open(seed.clone()).expect("cfb");
    let only = args.get(3).cloned().unwrap_or_default();
    let run = |n: &str, b: Vec<u8>| if only.is_empty() || only == n { run(n, b) };
    run("seed", seed.clone());
    let swap = |dt: Vec<u8>| {
        let mut t = Tree::from_cfb(&cfb);
        t.set_stream("/DocumentText", dt);
        write(&t)
    };
    run("reuse-64k-x2k", swap(reuse_blocks(65536, 2000)));
    run("reuse-1m-x200", swap(reuse_blocks(1 << 20, 200)));
    run("huge-count", swap(huge_count()));
    // 2nd report (2026-11-02): each style event and each glyph copied the whole property map
    let style_events = {
        let n = 200_000;
        let mut st = set_all(0x8000_0000);
        for i in 0..n as u32 {
            st.extend([0xfe, 0x00, 0x01, (i & 0xff) as u8 | 1, 0xff, 0x00]);
        }
        swap(text_stream(&text(n + 10, 100), &st))
    };
    let style_glyphs = {
        let n = 400_000;
        let mut st = set_all(0x8000_0000);
        st.extend([0x00]);
        st.extend(be(n as u32));
        swap(text_stream(&text(n, n + 1), &st))
    };
    if let Some(dir) = args.get(2).filter(|d| !d.is_empty()) {
        std::fs::write(format!("{dir}/style-events.jtd"), &style_events).ok();
        std::fs::write(format!("{dir}/style-glyphs.jtd"), &style_glyphs).ok();
    }
    run("style-events", {
        // every property set once, then 6-byte events that each change one value: each makes a
        // new style state over one character
        let n = 200_000;
        let mut st = set_all(0x8000_0000);
        for i in 0..n as u32 {
            st.extend([0xfe, 0x00, 0x01, (i & 0xff) as u8 | 1, 0xff, 0x00]);
        }
        swap(text_stream(&text(n + 10, 100), &st))
    });
    run("style-glyphs", {
        // one big style state over many characters (one paragraph)
        let n = 400_000;
        let mut st = set_all(0x8000_0000);
        st.extend([0x00]);
        st.extend(be(n as u32));
        swap(text_stream(&text(n, n + 1), &st))
    });
    // a document made in the editor, then saved as .jtd
    let made = |f: &dyn Fn(&mut ezpzjtd_core::edit::Editor)| {
        let mut ed = ezpzjtd_core::edit::Editor::new(ezpzjtd_core::blank::new_document());
        f(&mut ed);
        ezpzjtd_core::save::save(ezpzjtd_core::blank::BLANK_JTD, &ed.doc).expect("save").bytes
    };
    run("huge-size", made(&|ed| {
        ed.insert_text(&"あ".repeat(5_000));
        ed.select_all();
        ed.set_size(999.0);
    }));
    // many sheets whose DocumentText all point at the same stored data
    run("shared-sheets", {
        let mut dt = b"SsmgV.01".to_vec();
        dt.extend(be(1));
        dt.extend(be(256));
        let n_blocks = 3000u32;
        dt.extend(be(n_blocks));
        for i in 0..n_blocks {
            let mut blk = if i == 0 { b"TextV.01".to_vec() } else { Vec::new() };
            if i == 0 {
                blk.extend(be(n_blocks * 128 - 6));
            }
            while blk.len() < 256 {
                blk.extend("漢字かな\u{d}".encode_utf16().flat_map(|u| u.to_be_bytes()));
            }
            blk.truncate(256);
            dt.extend(&blk);
        }
        dt.extend(be(0));
        dt.extend(be(0));
        dt.extend(be(n_blocks * 256));
        dt.extend(be(n_blocks));
        dt.extend(be(n_blocks));
        for i in 0..n_blocks {
            dt.extend(be(i));
        }
        dt.extend(be(0));
        let mut t = Tree::from_cfb(&cfb);
        t.set_stream("/DocumentText", dt.clone());
        for k in 0..60 {
            t.set_stream(&format!("/ObjectSheets/DocSheet/DOCS_{k:04}/DocumentText"), b"x".repeat(5000));
        }
        let mut f = write(&t);
        // point every DocumentText entry at the big one
        let name: Vec<u8> = "DocumentText".encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        let mut recs = Vec::new();
        let mut i = 512;
        while i + 128 <= f.len() {
            if f[i..].starts_with(&name) && f[i + 66] == 2 {
                let size = u64::from_le_bytes(f[i + 120..i + 128].try_into().unwrap());
                recs.push((i, size));
            }
            i += 128;
        }
        if let Some(&(main, _)) = recs.iter().max_by_key(|r| r.1) {
            let (start, size) = (f[main + 116..main + 120].to_vec(), f[main + 120..main + 128].to_vec());
            for &(r, _) in &recs {
                f[r + 116..r + 120].copy_from_slice(&start);
                f[r + 120..r + 128].copy_from_slice(&size);
            }
        }
        f
    });
    if let Some(dir) = args.get(2).filter(|d| !d.is_empty()) {
        std::fs::write(format!("{dir}/reuse.jtd"), swap(reuse_blocks(65536, 2000))).ok();
        std::fs::write(format!("{dir}/count.jtd"), swap(huge_count())).ok();
        std::fs::write(
            format!("{dir}/manypages.jtd"),
            made(&|ed| {
                ed.insert_text(&"あ".repeat(5_000));
                ed.select_all();
                ed.set_size(999.0);
            }),
        )
        .ok();
    }
}
