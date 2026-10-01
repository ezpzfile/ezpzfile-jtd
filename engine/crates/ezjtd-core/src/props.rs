//! OLE property sets (`\x05SummaryInformation`): title, author, dates, the
//! program that made the file, and the *template* field, where Ichitaro
//! often leaves the author's original file path.

use serde::Serialize;

#[derive(Debug, Clone, Default, Serialize)]
pub struct Summary {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keywords: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comments: Option<String>,
    /// Template / original path. May reveal the author's folder structure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub saved: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub printed: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pages: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chars: Option<i32>,
    /// The program that wrote the file, e.g. `一太郎 2018-8 文書`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub application: Option<String>,
}

fn le16(b: &[u8], o: usize) -> Option<u16> {
    b.get(o..o + 2).map(|s| u16::from_le_bytes([s[0], s[1]]))
}
fn le32(b: &[u8], o: usize) -> Option<u32> {
    b.get(o..o + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}
fn le64(b: &[u8], o: usize) -> Option<u64> {
    b.get(o..o + 8).map(|s| {
        let mut a = [0u8; 8];
        a.copy_from_slice(s);
        u64::from_le_bytes(a)
    })
}

enum Val {
    Int(i32),
    Str(String),
    Time(u64),
}

fn decode_str(bytes: &[u8], codepage: u16) -> String {
    let bytes = match bytes.iter().position(|&c| c == 0) {
        Some(p) => &bytes[..p],
        None => bytes,
    };
    let enc = match codepage {
        932 => encoding_rs::SHIFT_JIS,
        65001 => encoding_rs::UTF_8,
        1252 => encoding_rs::WINDOWS_1252,
        949 => encoding_rs::EUC_KR,
        _ => encoding_rs::SHIFT_JIS,
    };
    enc.decode(bytes).0.trim().to_string()
}

/// FILETIME (100 ns since 1601) → `YYYY-MM-DD HH:MM` (UTC).
pub fn filetime(ft: u64) -> Option<String> {
    if ft == 0 {
        return None;
    }
    let secs = (ft / 10_000_000) as i64 - 11_644_473_600;
    if secs <= 0 {
        return None;
    }
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // civil-from-days (Howard Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    Some(format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}",
        rem / 3600,
        rem % 3600 / 60
    ))
}

pub fn parse_summary(b: &[u8]) -> Summary {
    let mut s = Summary::default();
    let Some(sec) = le32(b, 44).map(|v| v as usize) else {
        return s;
    };
    let Some(count) = le32(b, sec + 4) else {
        return s;
    };
    let mut raw: Vec<(u32, usize)> = Vec::new();
    for k in 0..count.min(256) as usize {
        let (Some(pid), Some(off)) = (le32(b, sec + 8 + k * 8), le32(b, sec + 12 + k * 8)) else {
            break;
        };
        raw.push((pid, sec + off as usize));
    }
    // codepage first
    let mut cp = 932u16;
    for &(pid, o) in &raw {
        if pid == 1 {
            cp = le16(b, o + 4).unwrap_or(932);
        }
    }
    for (pid, o) in raw {
        let Some(t) = le32(b, o).map(|t| t & 0xffff) else {
            continue;
        };
        let v = match t {
            2 => le16(b, o + 4).map(|x| Val::Int(x as i16 as i32)),
            3 => le32(b, o + 4).map(|x| Val::Int(x as i32)),
            30 => le32(b, o + 4)
                .and_then(|n| b.get(o + 8..o + 8 + n as usize))
                .map(|x| Val::Str(decode_str(x, cp))),
            31 => le32(b, o + 4)
                .and_then(|n| b.get(o + 8..o + 8 + n as usize * 2))
                .map(|x| {
                    let u: Vec<u16> = x
                        .chunks_exact(2)
                        .map(|c| u16::from_le_bytes([c[0], c[1]]))
                        .collect();
                    Val::Str(
                        String::from_utf16_lossy(&u)
                            .trim_end_matches('\0')
                            .trim()
                            .to_string(),
                    )
                }),
            64 => le64(b, o + 4).map(Val::Time),
            _ => None,
        };
        let Some(v) = v else { continue };
        let st = |v: &Val| match v {
            Val::Str(x) if !x.is_empty() => Some(x.clone()),
            _ => None,
        };
        match (pid, &v) {
            (2, _) => s.title = st(&v),
            (3, _) => s.subject = st(&v),
            (4, _) => s.author = st(&v),
            (5, _) => s.keywords = st(&v),
            (6, _) => s.comments = st(&v),
            (7, _) => s.template = st(&v),
            (8, _) => s.last_author = st(&v),
            (9, _) => s.revision = st(&v),
            (11, Val::Time(t)) => s.printed = filetime(*t),
            (12, Val::Time(t)) => s.created = filetime(*t),
            (13, Val::Time(t)) => s.saved = filetime(*t),
            (14, Val::Int(n)) => s.pages = Some(*n),
            (16, Val::Int(n)) => s.chars = Some(*n),
            (18, _) => s.application = st(&v),
            _ => {}
        }
    }
    // Ichitaro writes its own name and version into the comments field
    // (e.g. "一太郎 13/12/11/10/9/8 文書").
    if s.application.is_none() {
        if let Some(c) = &s.comments {
            if c.starts_with("一太郎") || c.starts_with("JUST") {
                s.application = s.comments.take();
            }
        }
    }
    s
}
