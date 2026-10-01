# Ichitaro document format (`.jtd`): working specification

Status: **draft, reverse-engineered**. Last updated 2026-10-01.

This document describes what EZPZ File JTD knows about JustSystems Ichitaro
(一太郎) documents. Nothing here comes from JustSystems. Everything was
learned by reading public files, and every claim carries a confidence tag:

| Tag | Meaning |
|---|---|
| **confirmed** | Verified on the whole corpus *and* against an independent source (the Word twin of the same document) |
| **strong** | Agrees with the Word twins in the large majority of cases |
| **observed** | Holds on every corpus file, meaning not proven |
| **candidate** | A pattern that fits most data; may be wrong |
| **unknown** | Seen, not understood. Preserve the bytes |
| **viewer** | Checked by opening files we changed in JustSystems' own 一太郎ビューア 2022 (see `tools/taroview/`) |

Corpus: 95 public Ichitaro files published by MEXT, 83 of them with a Word
twin exported by the publisher from the same source (`corpus/manifest.tsv`).
Files were written by Ichitaro 8 through Ichitaro 2018.

Prior art we built on, with thanks: the RFC series of
[OpenJTD](https://github.com/KimEJ/OpenJTD) and
[Tika-JTD](https://github.com/KHiyowa/Tika-JTD) (both Apache-2.0). Where we
reached a different or more specific conclusion, it is marked **new**.

---

## 1. Container: Compound File Binary

A `.jtd` is a CFB (OLE2) file, the same container as legacy `.doc` and HWP 5.0.
Signature `D0 CF 11 E0 A1 B1 1A E1`. **confirmed**

Some files have FAT inconsistencies (duplicate or looping chains). Readers
must follow chains leniently. **observed**

Streams seen in every corpus file:

```
/DocumentText            body text, structure, character styles   (§2-§5)
/Font                    font table                                (§6)
/\x05SummaryInformation  OLE summary properties                    (§7)
/\x04JSRV_SummaryInformation, /\x04JSRV_SegmentInformation
/DocumentViewStyles, /DocumentEditStyles, /DocumentPeripheralThree
/LineMark, /PageMark, /PaperMark       layout caches (unknown)
/Header, /Footnote, /ReferenceInfo
/DocumentMacro/…                       macro storage (usually empty)
```

Optional: `/TextLayoutStyle`, `/DocumentTextPositionTables`, `/PageLayoutStyle`,
`/NumberingLink`, `/RelatedDocuments`, `/Frame`, `/FrameName`, `/FigureData/…`,
`/DocItemInfo` + `/ObjectSheets/DocSheet/DOCS_nnnn/…` (multi-sheet documents),
`/Embedding n/…`, `/OleItem n/…` (embedded objects).

### 1.1 Stream directory `\x04JSRV_SegmentInformation` **new**

Every storage that Ichitaro writes (the root, `DocumentMacro`, …) has a
`\x04JSRV_SegmentInformation` stream listing its children **with their byte
sizes**. Ichitaro Viewer refuses a file whose sizes do not match
(「ファイルを読み込むことができません。」). **viewer**

```
"VDA_DOC\0"  …                         header
u16 LE @14   offset of the first entry (0x90)
u16 LE @16   entry size (0x60)
u16 LE @18   entry count
entry:       child name, UTF-16LE, 64 bytes, NUL padded
             u32 LE 0
             u32 LE byte size of the child stream (0 for a storage)
             u32 LE kind (1, 2 for streams; 0 for storages)
             padding to the entry size
```
**observed** on every corpus file. A writer must rewrite the sizes of the
streams it changes and drop the entries of streams it removes (Ichitaro
Viewer accepts both). **viewer**

### 1.2 Layout caches **new**

`/LineMark` starts with a small header (`09 08 00 00 00 01`, three u32
counts) followed by `(u16 length, u16 flags)` pairs: the length in text units
of each laid-out **line**, in order (paragraph wraps included).
**observed** (checked against the units of two files). `/PageMark`,
`/PaperMark` and `/DocumentTextPositionTables` (only 13 of 95 files) also
index the text by position. Ichitaro Viewer opens files with these caches
stale or removed, and lays the document out again. **viewer**

Compressed variants (`.jttc`) keep the real document inside
`/JSCompDocument` as an LHA `-lh5-` member (see OpenJTD RFC 0005). *Not yet
implemented here.*

---

## 2. `/DocumentText` is a block store: `SsmgV.01` **new**

Earlier work read `/DocumentText` as a header followed by text. That works
for small files only. The stream is a small block store; all integers are
big-endian:

```
offset 0   "SsmgV.01"
       8   u32 sub_count
      12   u32 block_size          always 256 in the corpus
      16   u32 block_count
      20   block_count × block_size bytes
      …    directory, one entry per sub-stream:
             u32 index, u32 0, u32 byte_length, u32 n, u32 n, u32 block_id × n
           u32 0
```

A sub-stream is its blocks concatenated in the listed order, cut to
`byte_length`. **confirmed** (95/95 files; all directories parse exactly to
the end of the stream)

Sub-stream 0 has one of two forms:

### 2.1 `TextV.01`: single piece (94 of 95 files)

```
"TextV.01"  u32 unit_count  unit_count × u16 (UTF-16BE text units)  style events (§5)
```

### 2.2 `QLSTV.01`: piece list (large documents) **new**

```
"QLSTV.01"  u32 piece_count  (u32 unit_count, u32 sub_index) × piece_count
```

Piece *k* is stored in sub-stream `sub_index`: `unit_count` text units, then
that piece's own style event list. The document text is the pieces in list
order. Pieces may split a record in the middle, so concatenate before
tokenizing. **observed** (1 file, 2 pieces of 17 206 units; both pieces' style
lists sum exactly to their unit counts)

---

## 3. Text units and records

Text is UTF-16BE. Structure is carried by control units below `0x20` and by
**records** that open with `0x001C`.

### 3.1 Control units

| Unit | Meaning | Confidence |
|---|---|---|
| `001F` | text starts (also the last word of every record) | confirmed |
| `000A` | paragraph end | confirmed |
| `000E` | end of a ruled line / table row | confirmed |
| `000C` | page break | observed |
| `0009` | tab (displayed) | observed |
| `0000` | end of text | observed |

### 3.2 Ordinary records

```
001C  class  len  payload…  len  0000  class  001F
```

`len` is the total length in words, footer included. The footer repeats
`len` and `class`, which makes records self-checking. **confirmed**

| Class | Role |
|---|---|
| `0010` | line / paragraph header (§4) |
| `0030` | cell of a ruled line: `0000 left right flags 0000` (§4.3) |
| `0000` | context for the next inline record, e.g. width for 均等割付 (unknown) |
| `0020` | ruled area → plain text transition (candidate) |

### 3.3 Inline records (class `0001`) **new: footer**

```
001C 0001 0007 0000 a b 001D   <text>   001E 0005 0000 0001 001F
```

The inline footer is 5 words (`001E`, its own length `0005`, `0000`, the class,
`001F`). Reading it as a record removes stray `0000` units that earlier
parsers mistook for end-of-text. **confirmed**

| `a b` | Kind | Rendering |
|---|---|---|
| `0000 0003` | visible inline text (ruby base, 均等割付 text) | show |
| `0001 0082` | ruby reading for the preceding inline | show as ruby |
| `0000 0001` | template placeholder | show |
| `0001 0000` | template instruction | hide |

---

## 4. Paragraph header (class `0010`)

### 4.1 TLV list **new**

The payload is `0000` followed by a list of `(tag, count, value × count)`
items, closed by `FFFF 0000`. All 3 638 class `0010` records in the corpus
parse exactly this way, ending precisely at the footer. **confirmed**

Earlier RFCs described fixed word offsets (`w4`, `w5`, …); those are the
first TLV items of particular records.

| Tag | Count | Meaning | Confidence |
|---|---|---|---|
| `0024` | 1 | **alignment**: 0 left, 1 center, 2 right | strong. Word twins: value 1 → centered 3 489 chars vs 153 left; value 2 → right 546 vs 21 center. Remaining chars have no explicit alignment in Word |
| `008F` | var | **ruled line**: vertical rules and cell grid (§4.3) | observed |
| `0020` | 4 | line spacing? `(mode, value, mode, value)`, values like 700 = 7.00 mm | candidate |
| `0026` | 5 | indents? `(flag, left, 0, first, 0)`, values in 1/100 mm | candidate |
| `002E` | 1 | single-column marker | unknown |
| `002A`, `0017` | var | unknown | unknown |

### 4.2 Units

Lengths in style and paragraph data are in **1/100 mm** (see §5, font
size). Table grid coordinates use a different unit (§4.3).

### 4.3 Ruled lines (罫線) **new: item grammar**

Ichitaro draws tables with rules on a character grid. Every text line inside
a ruled area starts with a class `0010` record whose `008F` item describes the
vertical rules of that line, followed by one class `0030` record per cell.

`008F` values: `width, 0, x0`, then items:

- style `< 0x10`: 2 words `(style, distance)`: a gap, **no line drawn**
- style `≥ 0x10`: 4 words `(style, a, b, distance)`: a vertical rule
- the last item may be cut to 2 words

Seen rule styles: `13` (most common), `1B`, `23`, `2B`, `14`, `16`, `24`, `26`.
`0x08` set in the style appears with dashed rules in the reference renderings
(candidate). A line whose items are all `< 0x10` has no rules. This is how
ordinary text above a form is stored. **observed**

Cell records `0030`: `0000 left right flags 0000`. Cells are ordered left to
right with a gap of a few grid units between them for the rule itself. Grid
width is the `008F` width (e.g. 160 or 168). **observed**

Geometry of a ruled line, as Ichitaro's own files store it: `x0` is the space
before the first rule (when it is not 0, a cell record `0..x0` holds the text
there); every rule is **2 grid units** wide; each item's distance is the width
of the cell after that rule, so a cell runs from `rule + 2` to the next rule;
and `x0 + Σ(2 + distance) + 2` equals the `008F` width when the last item is
cut to `(style, 0)` (1,119 of 1,125 such lines in the corpus that have no gap
items; lines with gap items count differently and are not understood yet).
A new table must keep these sums and use the document's own grid width (the
most common `008F` width in the file, 160 for a plain A4 page of 40 字).
**strong**

Horizontal rules are **not** in these records. Where they live (candidates:
`LineMark`, `TextLayoutStyle`, the `0020` / `002A` items) is open.

A one-cell line with no text (`0010` header, `0030` cell, `000E`, no
`000A`) is an **empty line of the box**: it still takes a line and keeps the
box's vertical rules. **viewer** A new line can be added to a box by copying
a neighbouring line's header and cell records (Ichitaro Viewer draws the box
one line taller). **viewer**

### 4.4 Line headers inside a line **new**

A class `0010` record that comes **after text and before the `000A`** does
not start a new paragraph. 「（２）」+ three empty headers +
「教育プロジェクトの内容…」 is one line in Ichitaro Viewer. The header's state
applies from that point on and is inherited by the following paragraphs.
**viewer** (our reader now agrees with Ichitaro Viewer on 99.9 % of the
visible characters of 94 public files, 87 of them exactly)

Header state is persistent like character styles: a paragraph without its
own header uses the last header seen. **strong**

---

## 5. Character style events

After the text units of each piece:

```
00 <u32 n>                          next n units keep the current style
FE (<id u8> <len u8> <value>)* FF 00   change properties; covers exactly 1 unit
FF                                  end
```

State is persistent. Sum of runs + number of change events = unit count, on
every piece in the corpus. **confirmed**

Property ids, decoded by aligning 22 722 characters with the Word twins:

| Id | Size | Meaning | Confidence |
|---|---|---|---|
| 1 | 2 | **bold**: `1` on, `FFFF` inherit | confirmed (62/62) |
| 2 | 2 | **font size in 1/100 mm**, `0` = document default. 282 → 8 pt, 318 → 9 pt, 388 → 11 pt, 423 → 12 pt (1 pt = 35.28) | confirmed |
| 3 | 2 | font selector (`2`, `10` → Gothic; `1` → Times New Roman). Not a direct `/Font` index | candidate |
| 4 | 1 | horizontal scale % (50, 100, 200) | candidate |
| 5 | 1 | vertical scale % | candidate |
| 6, 7, 9-12, 14 | 1-2 | appear together; co-occur with condensed spacing in Word | unknown |
| 8 | 2 | second font selector (`FFFF` = default) | candidate |
| 13 | 2 | **underline**: 1 single, 2 thick | confirmed (54/54) |
| 15 | 4 | **text colour** `0x00BBGGRR`, `FFFFFFFF` = auto | confirmed (also OpenJTD) |
| 16 | 4 | second colour (background / marker?) | candidate |
| 17 | 4 | unknown | unknown |
| 18 | 2 | unknown | unknown |
| 19 | 2 | signed; tracks baseline shift in Word, sign inverted, scale unclear | candidate |
| 20 | 4 | bit flags, high bit usually set; `0x10` often with underline | unknown |

---

## 6. `/Font`

```
"FontV.01"  u16 count
per font:   u16 index
            LOGFONT-like fixed part, 28 bytes (weight at +16, charset at +23)
            face name, UTF-16BE, NUL-terminated
            12 bytes (unknown)
            style name, UTF-16BE, NUL-terminated (usually 標準)
```
**observed**

## 7. `/\x05SummaryInformation`

Standard OLE property set (code page 932). Ichitaro writes its own name and
version into the *comments* field (`一太郎 13/12/11/10/9/8 文書`) and often the
author's **original file path** into the *template* field, e.g.
`N:\改革支援チーム\…\◎４条（様式１）.jtd`. Viewers should show this to the
user before they share a file. **confirmed**

---

## 8. Open questions (research backlog)

1. Horizontal rules and cell borders (§4.3)
2. Paragraph indents and line spacing units (TLV `0020`, `0026`)
3. Page size and margins (`PaperMark`, `PageLayoutStyle`)
4. Font selector ids 3/8 → face names
5. Properties 6-12, 14, 16-20
6. Vertical writing (縦書き)
7. `/Header`, `/Footnote`, frames (`/Frame`, `LayoutBoxText`)
8. `.jttc` (LHA) and pre-Ichitaro 8 files
9. ~~Writing: what must change in the layout caches~~ (see §9)
10. Full Ichitaro (not only the viewer) has not been tested with written files

The fastest way to close these is **paired samples**: the same document saved
twice from Ichitaro with one setting changed. See `docs/research/`.

---

## 9. Writing (saving) **new**

EZPZ File JTD saves by **patching the original file**, not by generating a
new one (`engine/crates/ezpzjtd-core/src/save.rs`). What we know is enough to
change text, paragraphs, character formatting, alignment, page breaks and
table lines; what we do not understand is copied byte for byte.

Rules that Ichitaro Viewer accepts (all **viewer**):

1. CFB may be rewritten from scratch (`cfbw.rs`): any sector layout works.
2. `/DocumentText` may be re-packed (`SsmgV.01`, 256-byte blocks, in order)
   and grow by any number of blocks.
3. Character styles may be re-encoded from the per-unit state: one change
   event wherever the state changes, `00 n` runs elsewhere. A property that
   must go back to "not set" is written with its neutral value (bold
   `FFFF`, size `0`, underline `0`, colour `FFFFFFFF`). Keep the original's
   choice of whether the list ends with `FF`.
4. When the text changes, remove `/LineMark`, `/PageMark` and
   `/DocumentTextPositionTables`; the viewer rebuilds the layout.
5. Update `\x04JSRV_SegmentInformation` (§1.1). **Without this step any size
   change makes the file unreadable.**
6. Splitting a paragraph needs only a `000A`; the new paragraph inherits the
   header state. A paragraph that must look different gets its own header.

Every save is checked by reading the result back and comparing it with the
edited document; on any difference the save is refused and nothing is
written. On the corpus: random editing sessions save 96 % (text), 99 %
(text + formatting) and 90 % (with tables and page breaks) of the time; the
rest are refused with a reason (joining lines across a ruled box, a line
break inside ruby, a table inside a box). Saved files are opened in
Ichitaro Viewer by `tools/taroview/`; results are in `experiments/results.md`.
