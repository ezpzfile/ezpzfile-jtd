# Ichitaro document format (`.jtd`): working specification

Status: **draft, reverse-engineered**. Last updated 2026-10-02.

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
| **ichitaro** | Checked with the latest 一太郎 itself (under Wine): either a paired sample it wrote from a macro, or a file we wrote opened in it (see `docs/research/ichitaro-latest.md`) |

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

The page setup itself (paper, margins, 字数 × 行数) is not in these caches
and not in `/PageLayoutStyle` (the latest 一太郎 does not write that stream): it is
in `/DocumentViewStyles` (§10). **ichitaro**

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
| `0020` | 4 | **line feed** (改行幅) after each line of the paragraph: `(kind, value, kind2, value2)`, see below. Older versions repeat the pair; the latest 一太郎 writes `(kind, value, 0, 0)`; `FF` in `kind2` also seen. On a ruled line's header it is the feed of that line (the whole row) | ichitaro |
| `0026` | 5 | **indents** (インデント): `(unit, left, right, first-line left, first-line right)`; unit `0` = half-width columns, `1` = 1/100 mm. The first-line values are measured from the margins, not from `left` (a hanging indent of left 4, first line 2 is stored `0,4,0,2,0`). All zero = none | ichitaro |
| `002E` | 1 | single-column marker | unknown |
| `002A`, `0017` | var | unknown | unknown |

Line feed kinds, from paired samples made with `LineSpacing()` and checked on
screen (the feed is the distance from a line to the next one, so `1/2` makes
the next line overlap; the line before the paragraph keeps the normal feed):

| `kind` | Feed | `kind` | Feed |
|---|---|---|---|
| 1 | 0 | 6 | 3/4 |
| 2 | 1/2 | 7 | ruby (ルビ) |
| 3 | 1/3 | 8 | `value` in 1/100 mm (500 = 5 mm) |
| 4 | 1/4 | 9 | `value` in 0.1 % of the normal feed (1500 = 150 %) |
| 5 | 2/3 | | |

**ichitaro** (all kinds; 1/2, 5 mm, 40 % and 150 % measured on screen)

A line header we write with these tags (indent on one paragraph, 1/2 feed on
the next) is drawn by the latest 一太郎 as one made in it. **ichitaro**

### 4.2 Units

Lengths in style and paragraph data are in **1/100 mm** (see §5, font
size). Table grid coordinates use a different unit (§4.3).

### 4.3 Ruled lines (罫線) **new: item grammar, horizontal rules**

Ichitaro draws tables with rules on a character grid. Every text line inside
a ruled area starts with a class `0010` record whose `008F` item describes
**all** the rules of that line (vertical and horizontal), followed by one
class `0030` record per cell.

`008F` values: `width, 0, x0`, then items of **4 words**
`(style, a, b, dist)`; the last item may be cut to 2 words `(style, dist)`.
**ichitaro** (all 2,369 ruled lines in the corpus add up under this grammar;
so does every line in the paired samples)

Positions are rule centres in grid units: the first item sits at `x0 + 1`,
and each item is `a + dist + 2` units after the one before (a cut last item:
`dist + 1`). The line adds up to `width`. A half-width rule (`0x10` class)
covers `centre ± 1`, a full-width one (`0x20` class) `centre ± 2` and takes
the place of one full-width character. Cells (`0030`) run between rules:
for a half-width rule at centre `c` the next cell starts at `c + 1`.
**ichitaro**

`style` bits:

| Bits | Meaning |
|---|---|
| `0x10` / `0x20` | half-width / full-width vertical rule (0 when the item has no vertical rule) |
| `0x01` | vertical rule over the upper half of the line |
| `0x02` | vertical rule over the lower half of the line (`0x03` = whole line) |
| `0x04` | a horizontal rule **through the middle** of the line starts here |
| `0x08` | a horizontal rule **under** the line (in the gap below it) starts here |

`a` is the length of a horizontal rule that does not run to the next item
(it is drawn from this item's centre to `centre + a + 1`); `b` describes the
horizontal rule from this item to the next one: `8` under the line, class
plus `4` (`0x14`, `0x24`) through the middle, `0x1C` both. **ichitaro**

Examples from the paired samples (half-width rules, 40 字 page, width `A0`):

```
a box over lines 2-3, 10 half-width columns wide, line rules (行間)
  line 1  A0,0,0, 8,13,0,8A            top edge: under line 1, from 1 to 20
  line 2  A0,0,0, 1B,0,8,12, 13,0,0,89  rules at 1 and 21, a line under the cell
  line 3  A0,0,0, 1B,0,8,12, 13,0,0,89
a box over lines 1-3 with rules through the middle of lines (通常)
  line 1  A0,0,0, 16,0,14,12, 12,0,0,89  ┌ and ┐: lower halves, middle line
  line 2  A0,0,0, 17,0,14,12, 13,0,0,89  ├ and ┤
  line 3  A0,0,0, 15,0,14,12, 11,0,0,89  └ and ┘: upper halves, middle line
```

So **the top edge of a table drawn with line rules is stored on the text line
above it**, as a line under that line. A table at the very top of a sheet
needs a line above it for that. **ichitaro**

**Line types** (線種) are not in the `008F` values: they are character style
properties (§5) on the units of the items. The meaning of the property ids is
different on these units:

| Property | On the item's 1st word (`style`) | On its 3rd word (`b`) |
|---|---|---|
| 1 | type of the vertical rule's upper half | |
| 2 | type of the vertical rule's lower half | |
| 3 | type of the item's own line through the middle | type of the middle line to the next item |
| 8 | type of the item's own line under the line | type of the line under, to the next item |

`FFFF` (1, 3, 8) and `0` (2) mean type 1, the default. **ichitaro** (paired
samples with all 16 types; the dashed 機関番号 box and thick group lines of a
public form show the same way in the latest 一太郎)

The 16 types as the latest 一太郎 draws them: 1 thin, 2 medium, 3 thick, 4-6 dashed
(thin, medium, thick), 7 double, 8 dotted, 9 long dashes, 10-11 dot-dash,
12-13 wavy, 14 double wavy, 15 hatched, 16 hairline. Corpus: types 2, 3, 4 and
7 are common.

A ruled line copied for a new row keeps the style states of its units, and
so its line types. **ichitaro**

The rule records themselves must carry a **plain record style**: the same
style properties as the text around, all set to 0. Records that keep the look
of a neighbouring character make Ichitaro draw their vertical rules faint and
skip their horizontal rules. **ichitaro**

Cell records `0030`: `0000 left right flags 0000`. A cell record also covers
the space before the first rule (`0..x0` when `x0` is not 0) and after the
last one. **observed**

A one-cell line with no text (`0010` header, `0030` cell, `000E`, no
`000A`) is an **empty line of the box**: it still takes a line and keeps the
box's vertical rules. **viewer** A new line can be added to a box by copying
a neighbouring line's header and cell records (Ichitaro Viewer draws the box
one line taller). **viewer**

When `b` carries a class (`0x10` / `0x20`, as in `(8, 3, 0x13, 0)` or
`(0x14, 1, 0x16, 0)`), it is the style of **a vertical rule at the end of the
item's own line**, at `centre + a + 1`; the next item follows at
`centre + a + dist + 2` as usual. 343 corpus lines have such items (for
example the right edge of a box whose last row ends in a horizontal rule);
the line types of that rule are properties 1 and 2 on the `b` word.
**observed** (every one of the 2,369 corpus lines adds up with the cut last
item counted as `dist + 1`; the rules drawn this way close the boxes that
Ichitaro shows closed)

### 4.4 Line headers inside a line **new**

A class `0010` record that comes **after text and before the `000A`** does
not start a new paragraph. 「（２）」+ three empty headers +
「教育プロジェクトの内容…」 is one line in Ichitaro Viewer. Its state is for
the next line. **viewer** (our reader agrees with Ichitaro Viewer on 99.9 % of
the visible characters of 94 public files, 87 of them exactly)

A line header formats **its own line only**. A line with no header (and no
header inside the line before it) has the defaults: left aligned, no indent,
normal line feed. Formatting the middle one of three paragraphs in the latest
一太郎 writes one header, on the middle paragraph; a right-aligned date
followed by a plain 「各位」 line shows 「各位」 left aligned. A ruled line's
header belongs to its row. **ichitaro** (Earlier versions of this document
said the state carried over to the following paragraphs, like character
styles. That was wrong.)

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
| 1 | 2 | `1` next to bold (Ichitaro writes it with the bold bits of id 20), `FFFF` inherit. Alone it does not make bold text | confirmed (62/62 with Word), viewer |
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
| 20 | 4 | **flags**, see below | confirmed (viewer) |

**Id 20** (found 2026-10-07 by writing one line per value and opening the
file in Ichitaro Viewer):

| Bits | Meaning |
|---|---|
| `80000000` | **attributes on**. Without it ids 2-19 are ignored: the text is drawn plain whatever size, colour, underline, font, scale or baseline they hold. Ichitaro sets it on every run it formats and writes `20 = 0` when the text goes back to plain. Every formatted character of the 95 public files has it |
| `0C000000` (bits 26-27) | **bold**: `1` or `2` on, `0` or `3` off |
| `30000000` (bits 28-29) | **italic**: `1` or `2` on, `0` or `3` off |
| `03000000` (bits 24-25) | emphasis dots (傍点): `2` above, `3` below |
| `00001000` | baseline shift of id 19 applied |
| `00000010` | underline of id 13 drawn (id 13 alone draws nothing) |
| `00000008` | reverse (white on black) |

So a run is bold when id 20 is `84000000` (Ichitaro's own bold title in the
corpus: `1 = 1`, `2 = 1A7`, `20 = 84000002`). The public files hold one more
value, `BD004000`, which shows as plain text: bold and italic are `3` there.

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

1. ~~Horizontal rules and cell borders, line types~~ (see §4.3)
2. ~~Paragraph indents and line spacing units (TLV `0020`, `0026`)~~ (see §4.1)
3. ~~Page size and margins~~ (see §10); header and footer positions, 段組
4. Font selector ids 3/8 → face names
5. Properties 6-12, 14, 16-18 and the other bits of 20 (§5)
6. Vertical writing (縦書き): the flag is known (§10), the layout is not done
7. `/Header`, `/Footnote`, frames (`/Frame`, `LayoutBoxText`)
8. `.jttc` (LHA) and pre-Ichitaro 8 files
9. ~~Writing: what must change in the layout caches~~ (see §9)
10. ~~Full Ichitaro (not only the viewer) has not been tested with written files~~ (see `docs/research/ichitaro-latest.md`)

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
7. Formatting must switch the attributes on (id 20, §5): size, colour and
   underline written without the high bit open fine but show as plain text,
   and bold, italic and underline are bits of id 20. Going back to plain
   writes `20 = 0`. (Saves before 2026-10-07 wrote the values alone; on files
   whose text already had attributes on, the new text took the bit from its
   neighbour and showed, elsewhere it did not.)
8. A **new document** is saved the same way, on `src/blank.jtd`: one empty
   paragraph on Ichitaro's default page (A4, margins 30 mm, 40 字 × 40 行,
   10.5 pt), made from a public file by `examples/makeblank.rs` (text, both
   summaries, layout caches and the document id replaced or removed; the
   storages' class ids kept, without them the viewer shows no document).

Every save is checked by reading the result back and comparing it with the
edited document; on any difference the save is refused and nothing is
written. On the corpus: random editing sessions save 96 % (text), 99 %
(text + formatting) and 90 % (with tables and page breaks) of the time; the
rest are refused with a reason (joining lines across a ruled box, a line
break inside ruby, a table inside a box). Saved files are opened in
Ichitaro Viewer by `tools/taroview/`; results are in `experiments/results.md`.

---

## 10. Page setup (文書スタイル): `/DocumentViewStyles` **new**

```
00 01 00 02   10 00   <u32 length>   records…   (then other data, not read)
record:       <u16 tag> <u16 length> <bytes>
```

Inside the first block, records `1001`-`1011` hold the document style. They
store **only the settings that differ from Ichitaro's built-in defaults**:
each group of fields starts with a **mask byte**, and the fields of the set
bits follow in **ascending bit order**. All 95 corpus files have this shape.
**ichitaro** (paired samples: one setting changed per file with
`DocumentStyleMargin`, `DocumentStyleLayout`, `DocumentStylePaper`,
`DocumentStyleFont`; the macro `GetDocumentStyle…` functions report the
defaults)

| Record | Group: bit (size) | Meaning | Default |
|---|---|---|---|
| `1001` | 1: bit 1 (u32), bit 2 (u32) | paper width, height, 1/100 mm (landscape stores the turned size); bits 3, 5, 6 are 1 byte, meaning unknown. The paper name follows in Shift_JIS (`A4 単票・縦方向`) | A4, 21000 × 29700 |
| `1002` | 1: bits 4, 7 (u16) | unknown | |
| | 2: bit 0 (u8); bits 3, 4, 5, 6 (u16) | margins top, bottom, left, right, 1/100 mm; bit 7 (u16, always 3000) unknown | 3000 each |
| | 3: bit 4 (u8) | `1` = 縦組み (vertical writing); bit 6 = ten u16 (700) | |
| `100B` | 1: bit 0 (u8), bit 1 (u16), bits 2 and 4 (u8), bit 6 (u8), bit 7 (u16) | bit 7: **characters per line in half-width columns** (80 = 40 字); bit 1: 行間 in 0.1 % of the character size | 80 |
| `100D` | 1: bit 0 (u8), bit 1 (u16) | bit 1: **lines per page** | 40 |
| `1006` | 1: bit 0 (u32) | **character size**, 1/100 mm (370 = 10.5 pt) | 370 |

Ichitaro spreads the characters over the width between the margins: one
character cell is `(paper width − left − right) / 字数`, and 字間 is the cell
minus the character size (the ruler shows the cells). When 字数 is more than
fits, characters are placed at the narrower cell too (seen with 90 字 on a
landscape A4 form). The line pitch is `(paper height − top − bottom) / 行数`.
**ichitaro**

When the latest 一太郎 changes the paper or the character size it moves the left and
right margins to keep 字数 (B5: 17.01 / 17.00 mm), or lowers 字数 by a half
column (12 pt on A4: 79 half-width columns).

