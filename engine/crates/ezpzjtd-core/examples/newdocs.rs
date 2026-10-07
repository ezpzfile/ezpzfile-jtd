//! New documents typed on the blank template and saved as .jtd, for checking
//! in Ichitaro. usage: newdocs <blank.jtd> <out_dir>
use ezpzjtd_core::doc::Align;
use ezpzjtd_core::edit::{Editor, Move};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let blank = std::fs::read(&a[1]).unwrap();
    std::fs::create_dir_all(&a[2]).unwrap();
    let base = ezpzjtd_core::open(blank.clone()).unwrap();
    let save = |e: &Editor, tag: &str| match ezpzjtd_core::save::save(&blank, &e.doc) {
        Ok(s) => {
            std::fs::write(format!("{}/new-{tag}.jtd", a[2]), &s.bytes).unwrap();
            let back = ezpzjtd_core::open(s.bytes.clone()).unwrap().plain_text();
            println!(
                "{tag}: {} bytes, warnings {:?}, text {:?}",
                s.bytes.len(),
                s.warnings,
                back.chars().take(60).collect::<String>()
            );
        }
        Err(err) => println!("{tag}: refused {err:?}"),
    };
    // 1. one line of text
    let mut e = Editor::new(base.clone());
    e.insert_text("新規文書のテストです。");
    save(&e, "text");
    // 2. several paragraphs: title centred and bold, date right, body
    let mut e = Editor::new(base.clone());
    e.set_align(Align::Center);
    e.toggle_bold();
    e.set_size(16.0);
    e.insert_text("社内研修のご案内");
    e.enter();
    e.set_align(Align::Right);
    e.toggle_bold();
    e.set_size(10.5);
    e.insert_text("令和8年10月7日");
    e.enter();
    e.set_align(Align::Left);
    e.insert_text("下記のとおり研修を行います。");
    e.enter();
    e.set_color(Some("#d40000".into()));
    e.insert_text("赤い文字");
    e.set_color(None);
    e.insert_text("と下線");
    e.toggle_underline();
    e.insert_text("の文字");
    e.toggle_underline();
    save(&e, "format");
    // 3. a table
    let mut e = Editor::new(base.clone());
    e.insert_text("表の前の文");
    e.enter();
    let ok = e.insert_table(3, 3);
    let cells = [
        "項目",
        "日時",
        "場所",
        "研修",
        "10月",
        "本社",
        "懇親会",
        "11月",
        "支社",
    ];
    for (k, c) in cells.iter().enumerate() {
        e.insert_text(c);
        if k + 1 < cells.len() {
            e.next_cell(false);
        }
    }
    println!("table inserted: {ok}");
    save(&e, "table");
    // 4. a page break and a second page
    let mut e = Editor::new(base.clone());
    e.insert_text("一ページ目");
    e.page_break();
    e.insert_text("二ページ目");
    save(&e, "pagebreak");
    // 5. long text over several pages
    let mut e = Editor::new(base.clone());
    for k in 1..=120 {
        e.insert_text(&format!(
            "{k}行目：あいうえおかきくけこさしすせそたちつてとなにぬねのはひふへほまみむめも"
        ));
        e.enter();
    }
    save(&e, "long");
    let _ = Move::Down;
}
