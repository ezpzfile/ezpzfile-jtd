//! Character style events that follow the text units of each piece.
//!
//! Grammar (byte oriented):
//!
//! ```text
//! 00 <u32-be n>                       the next n text units keep the current style
//! fe (<id> <len> <value[len]>)* ff 00 change properties; covers exactly 1 unit
//! ff                                  end
//! ```
//!
//! Property changes are *persistent*: they stay active until replaced.
//! The sum of run lengths plus the number of change events equals the
//! piece's unit count (verified on every sample in the corpus).
//!
//! Property meanings were decoded by aligning Ichitaro files with the Word
//! files that publishers exported from the same source (see
//! `docs/spec/JTD-FORMAT.md`). Each field is marked with how sure we are.

use std::collections::BTreeMap;

/// Raw property id → value (big-endian, up to 4 bytes).
pub type RawProps = BTreeMap<u8, u32>;

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct CharStyle {
    /// Bold. id 1 (`1` on, `0xffff` inherit). confirmed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bold: Option<bool>,
    /// Italic. Editor-side only for now (the JTD property id is not known yet).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub italic: Option<bool>,
    /// Font size in points. id 2, stored in 1/100 mm (`0` = document default). confirmed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_pt: Option<f32>,
    /// Index into `/Font` for Japanese text. id 3. likely
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font: Option<u16>,
    /// Index into `/Font` for Latin text. id 8. likely
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_latin: Option<u16>,
    /// Horizontal scale in percent (倍角/半角). id 4. likely
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale_x: Option<u8>,
    /// Vertical scale in percent. id 5. likely
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale_y: Option<u8>,
    /// Underline kind: 1 single, 2 thick. id 13. confirmed (small sample)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub underline: Option<u16>,
    /// Text colour as `#RRGGBB`. id 15, stored `0x00BBGGRR`, `0xffffffff` = auto. confirmed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Baseline shift, raw signed value. id 19. candidate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline: Option<i16>,
    /// Every property as stored, including the ones not yet understood.
    pub raw: RawProps,
}

impl CharStyle {
    pub fn from_raw(raw: &RawProps) -> Self {
        let g = |k: u8| raw.get(&k).copied();
        let flag16 = |v: u32| v != 0xffff && v != 0xffff_ffff;
        CharStyle {
            bold: g(1).and_then(|v| if v == 1 { Some(true) } else { None }),
            size_pt: g(2)
                .filter(|&v| v > 0)
                .map(|v| ((v as f32) / 35.2778 * 2.0).round() / 2.0),
            font: g(3).filter(|&v| flag16(v)).map(|v| v as u16),
            font_latin: g(8).filter(|&v| flag16(v)).map(|v| v as u16),
            scale_x: g(4).filter(|&v| v > 0 && v != 100).map(|v| v as u8),
            scale_y: g(5).filter(|&v| v > 0 && v != 100).map(|v| v as u8),
            underline: g(13).filter(|&v| v > 0 && v != 0xffff).map(|v| v as u16),
            color: g(15).filter(|&v| v != 0xffff_ffff && v != 0).map(|v| {
                let (r, gg, b) = (v & 0xff, (v >> 8) & 0xff, (v >> 16) & 0xff);
                format!("#{r:02x}{gg:02x}{b:02x}")
            }),
            baseline: g(19).map(|v| v as u16 as i16).filter(|&v| v != 0),
            italic: None,
            raw: raw.clone(),
        }
    }

    pub fn is_plain(&self) -> bool {
        self.bold.is_none()
            && self.italic.is_none()
            && self.size_pt.is_none()
            && self.underline.is_none()
            && self.color.is_none()
            && self.scale_x.is_none()
            && self.scale_y.is_none()
    }

    /// Same visible formatting (ignores raw-only differences).
    pub fn same_look(&self, o: &CharStyle) -> bool {
        self.bold == o.bold
            && self.italic == o.italic
            && self.size_pt == o.size_pt
            && self.font == o.font
            && self.font_latin == o.font_latin
            && self.scale_x == o.scale_x
            && self.scale_y == o.scale_y
            && self.underline == o.underline
            && self.color == o.color
            && self.baseline == o.baseline
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub enum StyleEvent {
    Run(u32),
    Set(Vec<(u8, Vec<u8>)>),
    End,
}

/// Parse the raw event list. Returns events and the number of bytes consumed.
pub fn parse_events(b: &[u8]) -> (Vec<StyleEvent>, usize) {
    let mut ev = Vec::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            0x00 => {
                let Some(s) = b.get(i + 1..i + 5) else { break };
                ev.push(StyleEvent::Run(u32::from_be_bytes([
                    s[0], s[1], s[2], s[3],
                ])));
                i += 5;
            }
            0xfe => {
                i += 1;
                let mut props = Vec::new();
                while i + 1 < b.len() && b[i] != 0xff {
                    let id = b[i];
                    let len = b[i + 1] as usize;
                    let Some(v) = b.get(i + 2..i + 2 + len) else {
                        return (ev, b.len());
                    };
                    props.push((id, v.to_vec()));
                    i += 2 + len;
                }
                if b.get(i..i + 2) != Some(&[0xff, 0x00]) {
                    break;
                }
                i += 2;
                ev.push(StyleEvent::Set(props));
            }
            0xff => {
                ev.push(StyleEvent::End);
                i += 1;
                break;
            }
            _ => break,
        }
    }
    (ev, i)
}

/// A span of units sharing one style state.
#[derive(Debug, Clone)]
pub struct StyleSpan {
    pub start: usize,
    pub len: usize,
    pub raw: RawProps,
}

/// Expand events into spans covering `unit_count` units.
pub fn spans(b: &[u8], unit_count: usize) -> Vec<StyleSpan> {
    let (ev, _) = parse_events(b);
    let mut out: Vec<StyleSpan> = Vec::new();
    let mut state = RawProps::new();
    let mut pos = 0usize;
    let mut push = |pos: &mut usize, n: usize, st: &RawProps| {
        if n == 0 {
            return;
        }
        if let Some(last) = out.last_mut() {
            if last.raw == *st && last.start + last.len == *pos {
                last.len += n;
                *pos += n;
                return;
            }
        }
        out.push(StyleSpan {
            start: *pos,
            len: n,
            raw: st.clone(),
        });
        *pos += n;
    };
    for e in ev {
        match e {
            StyleEvent::Run(n) => push(&mut pos, n as usize, &state),
            StyleEvent::Set(props) => {
                for (id, v) in props {
                    let val = v.iter().fold(0u32, |a, &x| (a << 8) | x as u32);
                    state.insert(id, val);
                }
                push(&mut pos, 1, &state);
            }
            StyleEvent::End => break,
        }
    }
    if pos < unit_count {
        let rest = unit_count - pos;
        push(&mut pos, rest, &state);
    }
    out
}

/// Per-unit lookup helper.
pub struct StyleMap {
    spans: Vec<StyleSpan>,
}

impl StyleMap {
    pub fn new(spans: Vec<StyleSpan>) -> Self {
        StyleMap { spans }
    }
    pub fn empty() -> Self {
        StyleMap { spans: Vec::new() }
    }
    pub fn at(&self, unit: usize) -> Option<&RawProps> {
        let i = self.spans.partition_point(|s| s.start + s.len <= unit);
        self.spans
            .get(i)
            .filter(|s| s.start <= unit)
            .map(|s| &s.raw)
    }
}
