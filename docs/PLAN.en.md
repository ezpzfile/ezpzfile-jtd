# EZPZ File JTD: project plan

[日本語](PLAN.ja.md) · **English** · [한국어](PLAN.ko.md)

> An open-source project that lets anyone open, edit and convert `.jtd` files from the Japanese
> word processor 一太郎 (Ichitaro), on any device. It aims to do for jtd what rhwp did for
> Korea's hwp.

Written 2026-10-01 · Status: v0.3. Reader, Ichitaro-style editor, saving back to `.jtd`

---

## 1. Why

- jtd files are still common in Japanese government offices and schools, but **Ichitaro runs only
  on Windows**. On Mac, Linux, phones and in the browser there is practically no way to open them.
- The format is not published, so other programs cannot read it, and documents stay locked in jtd.
- The main goal is to **work the format out and publish it**, so that anyone can build tools that
  read jtd and documents move sooner to open formats (docx, PDF, HTML).

## 2. What exists, and where this fits

| | What it does | Limits |
|---|---|---|
| Ichitaro Viewer (official) | View | Windows only |
| OpenOffice Ichitaro filter (2009) | Import | Windows DLL, abandoned |
| OpenJTD / Tika-JTD (2026, Apache-2.0) | Text extraction, structure RFC | No formatting, table layout or editing |
| ezpzfile.com JTD viewer (TypeScript) | View in the browser, export to Word | Tied to the site |
| **EZPZ File JTD (this project)** | **Engine + format spec + viewer → editor** | - |

Our own tool: **the "paired file" method.** Offices often publish the same document as jtd and as
Word. The Word side's formatting (size, bold, alignment) is known, so lining the two files up
character by character tells us mechanically what the unknown numbers in jtd mean. It works
without Ichitaro.

## 3. Results of the first day (2026-10-01)

- **New engine `ezpzjtd`** (Rust; external libraries only for character encodings and JSON): all
  **95 public jtd files open**, about 2 ms per file.
- **Body text: 97 % matches the paired Word files** (the rest is reading order in some table
  cells; practically no characters are missing).
- Shown: paragraphs, centre/right alignment, tables (merged cells, column width ratios), vertical
  rules on/off, font size, bold, underline, colour, ruby (furigana), page breaks, document
  properties (including the original save path).
- Export: plain text, Markdown, HTML, JSON. Also builds for WebAssembly (browser).
- **New findings not in earlier analyses** (details in `docs/spec/JTD-FORMAT.md`):
  1. `DocumentText` is not one continuous text but a **256-byte block store (SsmgV.01)**; large
     documents split their text into several pieces (`QLSTV.01`). Earlier analyses do not handle
     this and can miss text in large documents.
  2. The paragraph head record is a **(tag, count, value) list**. All 3,638 records parse exactly.
     Tag `0x24` is **paragraph alignment**, confirmed with the Word pairs.
  3. Character attribute numbers: **2 = font size (1/100 mm)**, 1 = bold, 13 = underline,
     15 = colour, confirmed on 22,722 characters of Word pairs.
  4. The item grammar of rule information `0x8F` (4 cells with a line / 2 without), which tells
     ruled rows from unruled ones.

### The same afternoon: saving back to jtd (v0.3)

- **The real Ichitaro Viewer is the referee.** The free viewer runs in the cloud under Wine with a
  Japanese locale; opening files, screenshots and copying text are automated (`tools/taroview/`).
  This route was chosen because the viewer crashes on every file under Korean Windows (code page
  949).
- Our text compared with what the viewer shows: **99.9 % of visible characters match, 87 of 94
  files are identical.** The comparison found and fixed "a paragraph head record in the middle of
  a line is not a new paragraph".
- **How saving works: patch the original.** Only changed characters, paragraphs, formatting,
  alignment, page breaks and table rows are rewritten; unknown parts stay as they are. The saved
  file is read back and compared with the editor; if anything differs, nothing is saved.
- **Main finding: `\x04JSRV_SegmentInformation`.** Every storage has a "stream list + sizes" table.
  If one size is wrong the viewer refuses the file ("cannot read the file"). Keeping it right made
  edits that change sizes open as well.
- Random edits saved successfully (95 public jtd × 3 runs): text 96 %, with formatting 99 %,
  with tables and page breaks 90 %. The rest is refused with the reason shown (joining a line
  across a ruled cell, a line break inside ruby, a new table inside a ruled area).
- Editor: Ctrl+S saves as jtd. PDF looks like the screen and has a searchable, copyable text layer.

### 2026-10-02: checked in the latest 一太郎, horizontal rules decoded

- **The latest 一太郎 runs under Wine** (`tools/taro2026/`). All 274 files saved by the editor open in it.
- New tables opened but could not be selected. Rule widths and table widths now follow Ichitaro's own files.
- **一太郎 made the paired samples.** Its macro feature (statement run and `RunFileMacro`) runs "write five
  lines, draw rules from here to there, save" by itself: 34 samples so far.
- **Horizontal rules found**: they are in the same `008F` items as the vertical rules. Items are always
  4 words `(style, length, horizontal, distance)`, and style bits mean "upper half of a vertical rule,
  lower half, a line through the middle of the line, a line under the line". A table's top edge belongs
  to the line above it. 2,340 of 2,369 ruled lines in the public files fit. Details in
  `docs/spec/JTD-FORMAT.md` §4.3.
- When rule records carry the look of the character next to them, 一太郎 draws the rules faint and skips the
  horizontal ones. With the values set to 0 they draw correctly.
- The editor now draws horizontal rules as in the original, and a new table is saved with its top,
  bottom and row lines.

### 2026-10-02 (later): page setup, indents, line feed, rule line types

- More paired samples from 一太郎's macros, one setting changed each (paper, margins, characters
  and lines: 16; indents and line feed: 18). `tools/taro2026/samples/page.py`, `para.py`.
- **The page setup is in `/DocumentViewStyles`.** Only settings that differ from the defaults are
  stored, as a mask byte followed by values. Paper width and height, the four margins, characters per
  line, lines per page and the base character size are read now, for all 95 public files (9 are
  landscape A4, one form has 90 字 per line). The editor's pages, ruler and Word export follow them.
- **Indents (TLV `0026`) and line feed (TLV `0020`) decoded** and drawn. Saving keeps them, and
  the latest 一太郎 shows a line header the engine wrote the same way as one it made itself.
- **Rule line types** are style properties on the units of the rule items (1 and 2: upper and lower
  half of a vertical rule, 3: the line through the middle, 8: the line under the line). All 16 types
  were made in 一太郎 and compared; thick, dotted, double and the rest are drawn. A row added to a
  table keeps its line types.
- The ruled lines that did not fit are explained: when the third word `b` of an item carries a rule
  class (0x10 / 0x20), it is a vertical rule at the end of the item's own line. Counted again, all
  2,369 ruled lines in the public files add up.
- Details in `docs/spec/JTD-FORMAT.md` §4.1, §4.3 and §10.

## 4. Stages

| Stage | Goal | Main work | Done when |
|---|---|---|---|
| **0. Engine v0.1** ✅ | Read | Container, block store, text, tables, some formatting | All public samples open |
| **1-a. Editor v0.2** ✅ | Ichitaro-style editor of our own | Editing engine, 字×行 layout, canvas screen, IME, docx export (`docs/EDITOR.en.md`) | Typing, tables, formatting and saving work |
| **1-b. Display v0.3** | Close to the original look | ~~Horizontal rules~~, ~~rule line types~~, ~~indents and line feed~~, ~~paper, margins, characters and lines~~ (done), fonts, headers and footnotes, vertical writing | No big differences next to the sample PDFs |
| **2. Convert v0.3** | Into open formats | docx and PDF export, `.jttc` decompression, multiple sheets | Tables and formatting survive in Word |
| **3. Mac app and web** | Real users | Browser viewer (WASM), Mac app (including Finder Quick Look) | Double-clicking a jtd on a Mac opens it |
| **4. Editing v0.5** | Fix documents | Edit text → save as docx/HTML; unknown bytes kept as they were | Open → edit → save as Word |
| **5-a. Save back to jtd v0.3** ✅ | Round trip | Patch the original, drop position caches, update the stream size table, verify after saving | Opens in Ichitaro Viewer (checked automatically) |
| **5-b. jtd saving v1.0** | Full round trip | New documents as jtd, edits across ruled cells, checked in the real Ichitaro | Opens unbroken in Ichitaro itself |

## 5. Principles

- **Separate from ezpzfile.com**: developed in its own repository. The site will only take the
  engine once it is mature, as a single WASM package.
- **Samples are not in the repository**: only a list of addresses (`corpus/manifest.tsv`); everyone
  downloads them themselves.
- **Show how sure we are**: every item in the format spec is marked confirmed / strong / observed /
  candidate / unknown.
- **Never drop unknown bytes**: they are needed to save back to jtd.
- **Credit earlier work**: OpenJTD's and Tika-JTD's RFCs are cited, and our new findings are written
  up so they can be shared back.
- Licence: **MIT or Apache-2.0** (your choice), compatible with both rhwp (MIT) and OpenJTD
  (Apache-2.0).

## 6. Help wanted

1. ~~Checking in the real Ichitaro~~: done with the latest 一太郎 (`docs/research/ichitaro-latest.md`).
2. **Paired samples**: horizontal rules, page setup, indents, line feed and line types were solved
   this way. Headers and footers, columns and vertical writing can be made the same way. The list is in `docs/research/paired-samples.en.md`.
3. ~~Public repository~~: done, `github.com/ezpzfile/ezpzfile-jtd`.
4. **Spreading the word**: introduce it to Japanese developer communities (Qiita, Zenn, X) as
   "open jtd files on a Mac".

## 7. Risks

- **Legal**: analysing a file format for compatibility is generally allowed, but it differs by
  country and licence terms. We do not take Ichitaro apart; we **only look at the files**. Not
  legal advice.
- **Market**: jtd is a shrinking format, so the tool matters more for opening and moving documents
  than for editing them.
- **Fidelity**: reproducing the exact original look takes time. Each stage shows honestly on screen
  how far it goes.
