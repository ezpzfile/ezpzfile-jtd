//! New documents.
//!
//! Saving as .jtd patches an existing Ichitaro file (see `save`), so a new
//! document needs one to start from: `blank.jtd`, one empty paragraph on
//! Ichitaro's default page (A4, margins 30 mm, 40 字 × 40 行, 10.5 pt), with
//! no author, dates, file paths or text in it. `examples/makeblank.rs` makes it
//! from a public file and says what it removes and why the rest stays.

use crate::doc::Document;

/// The empty Ichitaro document new documents are saved on.
pub const BLANK_JTD: &[u8] = include_bytes!("blank.jtd");

/// A new document: one empty paragraph on Ichitaro's default A4 page. Save
/// it with `save::save(BLANK_JTD, &doc)`.
pub fn new_document() -> Document {
    crate::open(BLANK_JTD.to_vec()).expect("the blank document reads")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::Align;
    use crate::edit::Editor;

    #[test]
    fn the_blank_is_one_empty_a4_paragraph() {
        let d = new_document();
        assert_eq!(d.plain_text().trim(), "");
        assert_eq!(d.sheets.len(), 1);
        let p = d.page.clone().unwrap();
        assert_eq!(
            (p.width_mm, p.height_mm, p.margin_top_mm, p.margin_left_mm),
            (210.0, 297.0, 30.0, 30.0)
        );
        assert_eq!(
            (p.chars_per_line, p.lines_per_page, p.font_pt),
            (40, 40, 10.5)
        );
        // nothing about who wrote the file it was made from
        assert!(
            d.summary.author.is_none()
                && d.summary.template.is_none()
                && d.summary.created.is_none()
        );
    }

    #[test]
    fn new_documents_save_as_jtd() {
        let mut e = Editor::new(new_document());
        e.set_align(Align::Center);
        e.toggle_bold();
        e.insert_text("新規文書");
        e.enter();
        e.set_align(Align::Left);
        e.toggle_bold();
        e.insert_text("本文です。");
        e.enter();
        assert!(e.insert_table(2, 2));
        for (k, c) in ["A", "B", "C", "D"].iter().enumerate() {
            e.insert_text(c);
            if k < 3 {
                e.next_cell(false);
            }
        }
        let s = crate::save::save(BLANK_JTD, &e.doc).expect("saves");
        assert!(s.warnings.is_empty(), "{:?}", s.warnings);
        let back = crate::open(s.bytes).unwrap();
        let t = back.plain_text();
        assert!(t.starts_with("新規文書\n本文です。\n"), "{t:?}");
        assert!(t.contains("A\tB\nC\tD"), "{t:?}");
        let first = |k: usize| match &back.sheets[0].blocks[k] {
            crate::doc::Block::Paragraph(p) => p.runs[0].style.clone(),
            b => panic!("{b:?}"),
        };
        assert_eq!((first(0).bold, first(1).bold), (Some(true), None));
    }

    #[test]
    fn formatting_is_switched_on_for_ichitaro() {
        // every look the editor can give, saved and read back; the style
        // state carries id 20 with its high bit (without it Ichitaro shows
        // the text plain)
        let mut e = Editor::new(new_document());
        e.toggle_bold();
        e.insert_text("太字");
        e.toggle_bold();
        e.toggle_italic();
        e.insert_text("斜体");
        e.toggle_italic();
        e.toggle_underline();
        e.insert_text("下線");
        e.toggle_underline();
        e.set_color(Some("#d40000".into()));
        e.insert_text("赤");
        e.set_color(None);
        e.set_size(16.0);
        e.insert_text("大");
        e.set_size(10.5);
        e.insert_text("標準");
        let s = crate::save::save(BLANK_JTD, &e.doc).expect("saves");
        let back = crate::open(s.bytes).unwrap();
        let crate::doc::Block::Paragraph(p) = &back.sheets[0].blocks[0] else {
            panic!()
        };
        let looks: Vec<(
            String,
            Option<bool>,
            Option<bool>,
            Option<u16>,
            Option<String>,
            Option<f32>,
        )> = p
            .runs
            .iter()
            .map(|r| {
                (
                    r.text.clone(),
                    r.style.bold,
                    r.style.italic,
                    r.style.underline,
                    r.style.color.clone(),
                    r.style.size_pt,
                )
            })
            .collect();
        assert_eq!(
            looks,
            vec![
                ("太字".into(), Some(true), None, None, None, None),
                ("斜体".into(), None, Some(true), None, None, None),
                ("下線".into(), None, None, Some(1), None, None),
                ("赤".into(), None, None, None, Some("#d40000".into()), None),
                ("大".into(), None, None, None, None, Some(16.0)),
                ("標準".into(), None, None, None, None, None),
            ]
        );
        for r in &p.runs[..5] {
            assert_ne!(
                r.style.raw.get(&20).copied().unwrap_or(0) & crate::style::ATTR_ON,
                0,
                "{}",
                r.text
            );
        }
    }
}
