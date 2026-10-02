# Checks with the latest 一太郎 (2026-10-02)

The latest 一太郎 runs under Wine (64-bit prefix, Japanese locale,
Windows 11 mode). Opening, editing, select-all and copy work. Tools:
`tools/taro2026/`.

## Our saved files open in the latest 一太郎

| Set | Files | Opened |
|---|---|---|
| Public corpus, unchanged | 96 | 95. The other one is an RTF file with a `.jtd` name; 一太郎 refuses it (「ファイル形式が拡張子と一致していないため読み込めません」) and so does our reader |
| Saved by our editor after random edits (text, formatting, page breaks, tables) | 274 | **274** |
| The 88 of them with table edits, made again after the fixes below | 88 | **88**; select all and copy work in all 88 (59 failed before the fixes) |
| Made again after reading the page setup, indents, line feed and line types (finding 4) | 271 | **271**; select all and copy work in all 271; the only message boxes are the font notices of the originals |

The only message box was the notice that Times New Roman is missing on the
test machine; it appears for the same files in the original and the saved
version (274 of 274).

The text 一太郎 shows (copied out of it) matches what our reader reads: 99.9 %
of visible characters for the originals, 99.4 % for the saved files. The rest
is reading order inside narrow table columns; the screens show the inserted
text in the same place.

## What the checks found

1. **New tables were half recognised.** 一太郎 opened them but selecting all
   failed or reported 「空きメモリ不足のため実行できません」. A one-edit
   comparison (text, page break, new row in an existing table, new table,
   new line) narrowed it to new tables. Cause: cells were spaced 4 grid units
   apart and the table width followed the editor's 40 字 line, so the rule
   items did not add up to the line width. Fixed: rules are 2 units wide and
   the table uses the document's own grid width (spec §4.3).
2. **Horizontal rules**, missing from the format notes until now, are in the
   same `008F` items as the vertical ones. Paired samples made by 一太郎
   itself (boxes, single lines, line types, 行間 and 通常 rules) decoded the
   item grammar: 4-word items `(style, a, b, dist)`, style bits for the upper
   and lower half of a vertical rule and for horizontal rules through the
   middle of or under the line. The top edge of a table sits on the line above
   it. Spec §4.3.
3. **Rule records need a plain record style.** When the records of a new table
   took the character style of the text next to them, 一太郎 drew the vertical
   rules faint and no horizontal rules. With the style properties set to 0, as
   in 一太郎's own files, the table draws like one made in 一太郎.

4. **Page setup, indents, line feed, line types.** Further paired samples
   (below) located the page setup in `/DocumentViewStyles` (spec §10), decoded
   the paragraph TLVs `0026` (indents) and `0020` (line feed) (spec §4.1), and
   found the line types as style properties on the units of the rule items
   (spec §4.3). A file in which the engine wrote an indent and a 1/2 line feed
   into line headers shows in 一太郎 exactly as if it had been set there.

## Paired samples made

| Set | What changes | Result |
|---|---|---|
| `rules.py` (11) | box, box with columns or rows, single lines at different lines and lengths, 行間 / 通常 | the item grammar and the bits |
| `types.py` (19) | line types 1-16, half / full width, 行間 / 通常, a box not at the line start | type is in the style properties of the rule items, not in the `008F` values; `x0` |
| `inject.py` (4) | a box drawn by 一太郎 inside public documents | same encoding on wide (landscape) grids, record style |
| `page.py` (16) | paper (A4, B5, A3, landscape), each margin, 字数, 行数, 行間, character size, 縦組み; a probe that writes the defaults into the document | `/DocumentViewStyles`: masks and fields (spec §10) |
| `para.py` (18) | indents (left, right, first line, hanging, mm), every line feed kind | TLV `0026` and `0020` (spec §4.1) |

On screen, 一太郎 spreads the characters of a line over the width between
the margins (the cell is that width divided by 字数), and moves a line with a
1/2 feed by half the normal distance; 5 mm and 150 % feeds measure 18 and
33 pixels at 100 % where the normal feed is 22.
