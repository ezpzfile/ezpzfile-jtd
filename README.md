# EZPZ File JTD

**Open Ichitaro (一太郎) `.jtd` documents anywhere — Mac, Linux, phone, browser.**
An open-source reader and format specification for JustSystems Ichitaro files,
in the spirit of [rhwp](https://github.com/edwardkim/rhwp) for HWP.

> 一太郎の `.jtd` ファイルを、Mac・Linux・スマホ・ブラウザで開くためのオープンソースです。
> ファイル形式の解析結果（仕様書）も公開しています。ファイルは端末の外に送信されません。

Status: **v0.1 — reading works, layout is approximate.** Not affiliated with JustSystems.

## What works

Tested on 95 public Ichitaro files (Ichitaro 8 – 2018) published by Japanese ministries:

- all 95 open; about 2 ms per file
- body text: 97 % character match against the publisher's own Word export of the same document
- paragraphs and alignment, ruled tables (merged cells, column widths, ruled/unruled lines)
- font size, bold, underline, colour, ruby (furigana), page breaks
- document properties, including the **original file path** that Ichitaro leaves inside files
- export: plain text, Markdown, HTML, JSON

Not yet: horizontal rules, indents and line spacing, page size and margins, pictures,
vertical writing, `.jttc` (compressed), editing, saving back to `.jtd`. See the
[roadmap](docs/PLAN.ko.md).

## Try it

**Browser (no install):** build once, then double-click `web/dist/ezjtd-viewer.html`
and drop a `.jtd` file on it. It works offline.

```sh
./web/build.sh
```

**Command line:**

```sh
cd engine
cargo run --release -p ezjtd-cli -- text  sample.jtd      # plain text
cargo run --release -p ezjtd-cli -- html  sample.jtd > sample.html
cargo run --release -p ezjtd-cli -- md    sample.jtd      # Markdown
cargo run --release -p ezjtd-cli -- json  sample.jtd      # document model
cargo run --release -p ezjtd-cli -- info  sample.jtd      # properties, fonts, sheets
```

Research commands: `streams`, `dump <path>`, `tokens`, `styles`.

**As a library (Rust):**

```rust
let doc = ezjtd_core::open(std::fs::read("sample.jtd")?)?;
println!("{}", doc.plain_text());
let html = ezjtd_core::export::to_html(&doc);
```

**As a library (JavaScript / WebAssembly):** `web/pkg/` after `./web/build.sh`.

```js
import init, { JtdDocument } from "./pkg/ezjtd_wasm.js";
await init();
const doc = new JtdDocument(new Uint8Array(await file.arrayBuffer()));
element.innerHTML = doc.html();
```

## The format

[`docs/spec/JTD-FORMAT.md`](docs/spec/JTD-FORMAT.md) is the working specification.
Every statement is tagged *confirmed / strong / observed / candidate / unknown*.

How we decode it without Ichitaro: Japanese public bodies often publish the same
form as `.jtd` **and** `.doc`. Word's formatting is known, so lining the two files
up character by character tells us what each unknown JTD field means.
`tools/research/` holds those scripts.

## Repository

```
engine/                Rust workspace
  crates/ezjtd-core    the reader (CFB → block store → text/records → styles → model)
  crates/ezjtd-cli     `ezjtd` command
  crates/ezjtd-wasm    WebAssembly bindings
web/                   single-file browser viewer
docs/spec/             format specification
docs/research/         research notes and sample-making guides
tools/                 corpus download and research scripts
corpus/manifest.tsv    URLs of public sample files (files themselves are not committed)
```

## Contributing

The most valuable contribution is **paired samples**: the same document saved twice
from Ichitaro with one setting changed. See
[`docs/research/paired-samples.ko.md`](docs/research/paired-samples.ko.md).
Please do not commit documents you do not have the right to share.

```sh
python3 tools/fetch_corpus.py --docx   # public corpus → corpus/local/
cd engine && EZJTD_CORPUS=../corpus/local cargo test
```

## Thanks

Built on the published research of [OpenJTD](https://github.com/KimEJ/OpenJTD) and
[Tika-JTD](https://github.com/KHiyowa/Tika-JTD). See [NOTICE](NOTICE).

## License

MIT or Apache-2.0, at your option. "一太郎" / "Ichitaro" are trademarks of JustSystems Corporation.
