//! `ezjtd` — inspect and convert Ichitaro documents.

use ezjtd_core::{cfb::Cfb, export, ssmg, style, text};
use std::io::Write;
use std::process::ExitCode;

const HELP: &str = "ezjtd — Ichitaro (.jtd/.jtt) reader

USAGE:
  ezjtd text     <file>            plain text
  ezjtd md       <file>            Markdown
  ezjtd html     <file>            standalone HTML page
  ezjtd json     <file>            document model as JSON
  ezjtd info     <file>            summary information and stats

RESEARCH:
  ezjtd streams  <file>            list CFB entries
  ezjtd dump     <file> <path>     raw bytes of one stream (\\x05 escapes allowed)
  ezjtd tokens   <file>            DocumentText tokens with unit offsets
  ezjtd styles   <file>            style spans with the text they cover
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprint!("{HELP}");
        return ExitCode::from(2);
    }
    match run(&args[1], &args[2], args.get(3).map(|s| s.as_str())) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(cmd: &str, file: &str, arg: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    let bytes = std::fs::read(file)?;
    let mut out = std::io::stdout().lock();
    match cmd {
        "text" => write!(out, "{}", ezjtd_core::open(bytes)?.plain_text())?,
        "md" => write!(out, "{}", export::to_markdown(&ezjtd_core::open(bytes)?))?,
        "html" => write!(out, "{}", export::to_html_page(&ezjtd_core::open(bytes)?))?,
        "json" => writeln!(
            out,
            "{}",
            serde_json::to_string_pretty(&ezjtd_core::open(bytes)?)?
        )?,
        "info" => {
            let d = ezjtd_core::open(bytes)?;
            writeln!(out, "{}", serde_json::to_string_pretty(&d.summary)?)?;
            writeln!(out, "format\t{}", d.format)?;
            writeln!(
                out,
                "fonts\t{}",
                d.fonts
                    .iter()
                    .map(|f| f.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )?;
            for s in &d.sheets {
                let (mut p, mut t) = (0, 0);
                for b in &s.blocks {
                    match b {
                        ezjtd_core::doc::Block::Paragraph(_) => p += 1,
                        ezjtd_core::doc::Block::Table(_) => t += 1,
                        _ => {}
                    }
                }
                writeln!(
                    out,
                    "sheet\t{}\t{}\tparagraphs={p}\ttables={t}",
                    s.name, s.path
                )?;
            }
            for o in &d.objects {
                writeln!(out, "object\t{o}")?;
            }
            for w in &d.warnings {
                writeln!(out, "warning\t{w}")?;
            }
        }
        "streams" => {
            let c = Cfb::open(bytes)?;
            for e in c.entries() {
                writeln!(out, "{:?}\t{}\t{}", e.kind, e.size, e.display_path())?;
            }
            for w in &c.warnings {
                eprintln!("warning: {w}");
            }
        }
        "dump" => {
            let c = Cfb::open(bytes)?;
            let p = arg.ok_or("missing stream path")?;
            let e = c
                .entries()
                .iter()
                .find(|e| e.path == p || e.display_path() == p)
                .ok_or("no such stream")?;
            out.write_all(&c.read_entry(e))?;
        }
        "tokens" | "styles" => {
            let c = Cfb::open(bytes)?;
            let p = arg.unwrap_or("/DocumentText");
            let raw = c.read(p).ok_or("no DocumentText")?;
            let pieces = ssmg::pieces(&raw)?;
            let mut base = 0usize;
            for (pi, piece) in pieces.iter().enumerate() {
                writeln!(
                    out,
                    "# piece {pi}: {} units, {} style bytes",
                    piece.units.len(),
                    piece.style.len()
                )?;
                if cmd == "tokens" {
                    for t in text::tokenize(&piece.units) {
                        match t {
                            text::Token::Text { start, text } => {
                                writeln!(out, "{}\ttext\t{:?}", base + start, text)?
                            }
                            text::Token::Record {
                                start,
                                class,
                                payload,
                            } => {
                                let extra = if class == 0x10 {
                                    text::para_tlv(&payload)
                                        .iter()
                                        .map(|(t, v)| {
                                            format!(
                                                "{t:#x}:{}",
                                                v.iter()
                                                    .map(|x| format!("{x:x}"))
                                                    .collect::<Vec<_>>()
                                                    .join(",")
                                            )
                                        })
                                        .collect::<Vec<_>>()
                                        .join(" ")
                                } else {
                                    payload
                                        .iter()
                                        .map(|x| format!("{x:04x}"))
                                        .collect::<Vec<_>>()
                                        .join(" ")
                                };
                                writeln!(out, "{}\trec{class:#06x}\t{extra}", base + start)?
                            }
                            text::Token::Inline {
                                start,
                                header,
                                text,
                                ..
                            } => writeln!(
                                out,
                                "{}\tinline\t{:?}\t{}",
                                base + start,
                                text,
                                header
                                    .iter()
                                    .map(|x| format!("{x:04x}"))
                                    .collect::<Vec<_>>()
                                    .join(" ")
                            )?,
                            text::Token::Control { start, code } => {
                                writeln!(out, "{}\tctl\t{code:#06x}", base + start)?
                            }
                        }
                    }
                } else {
                    for s in style::spans(&piece.style, piece.units.len()) {
                        let end = (s.start + s.len).min(piece.units.len());
                        let txt: String =
                            String::from_utf16_lossy(&piece.units[s.start.min(end)..end])
                                .chars()
                                .filter(|c| !c.is_control())
                                .take(24)
                                .collect();
                        let props: Vec<String> =
                            s.raw.iter().map(|(k, v)| format!("{k}={v:#x}")).collect();
                        writeln!(
                            out,
                            "{}\t{}\t{}\t{:?}",
                            base + s.start,
                            s.len,
                            props.join(" "),
                            txt
                        )?;
                    }
                }
                base += piece.units.len();
            }
        }
        _ => {
            eprint!("{HELP}");
            return Err("unknown command".into());
        }
    }
    Ok(())
}
