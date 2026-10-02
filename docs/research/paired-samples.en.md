# Paired samples: what to do if you have Ichitaro

[日本語](paired-samples.ja.md) · **English** · [한국어](paired-samples.ko.md)

On a Windows PC with Ichitaro, the files below would let us work out the rest of
the format quickly. The method is simple: **type some text into a new document and save it, then
change one setting and save again under another name.** The difference between the two files is
where that setting is stored.

File names: `number-what-value.jtd` (for example `03-margin-left-30mm.jtd`). If you can, also save
the same document **as PDF**.

| No. | What to change | Files to make | What it tells us |
|---|---|---|---|
| 01 | Baseline | A document with one line, `あいう` | The baseline for comparison |
| 02 | Paper | A4 portrait / A4 landscape / B5 | PaperMark, PageLayoutStyle |
| 03 | Margins | Left margin 20 / 30 mm | Margin units |
| 04 | Characters and lines | 40 字 × 36 行 / 30 字 × 30 行 | The character grid (units of rule coordinates) |
| 05 | Indent | Left indent 2 characters / first line 1 character | TLV 0x26 |
| 06 | Line spacing | 1.0 / 1.5 / 2.0 | TLV 0x20 |
| 07 | Font | MS明朝 / MSゴシック / 游明朝 | Attributes 3 and 8 |
| 08 | Character decoration | Italic / strikethrough / superscript / subscript / highlighter | Attributes 6 to 12 and 16 to 20 |
| 09 | Ruled table | 2×2 table (solid) / the same table (dotted) / the same table (thick) | Position and style of horizontal rules |
| 10 | Merging ruled cells | Merge the middle cells of a 3×3 table horizontally | How merged cells are stored |
| 11 | Vertical writing | The same text, written vertically | The vertical writing flag |
| 12 | Header and footer | Header `ヘッダ` / page numbers | The Header piece |
| 13 | Footnote | One footnote | The Footnote piece |
| 14 | Picture | Insert one PNG | The picture object |
| 15 | Compressed save | The same document as `.jttc` | LHA compression |
| 16 | Several sheets | Two sheets | DocItemInfo |
| 17 | Text change | No. 01 with `あいう` → `あいうえ` | **Numbers that change when a file is saved again** |

No. 17 matters most. Knowing which numbers change along with one added character (LineMark,
PageMark, the position tables) makes saving back to jtd more reliable.

Put the files in `corpus/local/paired/` (they are not committed to the repository).
