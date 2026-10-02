# EZPZ File JTD editor design

[日本語](EDITOR.ja.md) · **English** · [한국어](EDITOR.ko.md)

Goal: like rhwp-studio, **an editor of our own where the engine draws the page itself**. The screen
and controls follow 一太郎 (Ichitaro) closely enough that its users can start right away.
No trademarks, logos, icons or signature colours are used. Only the layout and the way things work
(where each control sits, which key does what) are matched.

## 1. Structure

```
┌──────────── Browser / Mac app (WebView) ──────────┐
│ UI (JavaScript, no build tools)                   │
│  menu bar · toolbar · jump palette · tool palette │
│  ruler · status bar · sheet tabs · dialogs        │
│  hidden textarea ← keys and IME (Japanese input)  │
│  one <canvas> per page ← only draws display lists │
└───────────────▲───────────────┬───────────────────┘
                │ JSON: display │ commands:
                │ list per page,│ insert text,
                │ cursor, format│ bold, …
┌───────────────┴───────────────▼───────────────────┐
│ ezpzjtd-core (Rust → WebAssembly)                 │
│  read  : jtd → document model                     │
│  edit  : cursor, selection, typing, delete,       │
│          formatting, tables, undo                 │
│  layout: 字×行 grid, line breaks, pages, tables   │
│  export: docx · html · md · txt                   │
└───────────────────────────────────────────────────┘
```

- **The document model is the source of truth.** The screen is just a drawing of the model, so
  none of the instability of `contenteditable` applies.
- Layout uses a **character grid (字×行)** like Ichitaro. A document opened from a file keeps its own page setup (paper, margins, 字数, 行数, character size); a new one is A4, margins 30 mm, 40 字 × 40 行, 10.5 pt, as in the latest 一太郎.
  A full-width character takes one cell, a half-width one half a cell.
- Japanese input: when the hidden textarea receives text still being converted (preedit), the
  engine lays it out at the cursor with an underline. The textarea is moved to the cursor so the
  candidate window opens next to it.
- Every keystroke lays out the whole document again (documents are tens of KB, about 1 ms), and
  the JavaScript redraws **only the visible pages**.
- Undo: a snapshot before each edit, up to 100 steps (Ichitaro's default). A run of typing counts
  as one step.

## 2. Ichitaro's screen and ours

| Ichitaro | Our editor | Stage |
|---|---|---|
| Menu bar ファイル/編集/表示/挿入/書式/罫線/ツール/ヘルプ | Same names and order, shortcuts shown | v0.2 |
| ツールバー | New, open, save, print, undo, formatting, alignment, table | v0.2 |
| ジャンプパレット (left) | Page list (first line as preview), sheets, document info | v0.2 |
| ツールパレット (right, collapsible) | 文字 (size, bold, underline, colour), 段落 (alignment), 罫線 (make a table), 文字数 | v0.2 |
| 水平ルーラ | Ruler in 字 units, cursor position shown | v0.2 |
| ステータスバー | `nページ n行 n字`, 挿入/上書, character count, zoom | v0.2 |
| 編集記号 (改行マーク, 全角スペース) | Show or hide (表示 menu) | v0.2 |
| シートタブ | Tabs at the bottom for documents with several sheets | v0.2 |
| 作業フェーズ (基本編集/エディタ/印刷…) | 基本編集 only; エディタ later | v0.3 |
| 罫線モード (Ctrl+¥, drawing lines with the mouse) | v0.2 has a make-a-table dialog; drawing lines comes later | v0.3 |
| 縦書き | Later | v0.4 |

## 3. Shortcuts (ones with a known source first)

| Key | Action | Source |
|---|---|---|
| Ctrl+5 / Ctrl+6 | センタリング / 右寄せ | Published shortcut list |
| Ctrl+4 | 左寄せ (back to the default) | **Guess**, needs checking |
| Ctrl+↑ / Ctrl+↓ | Larger / smaller text | Published list |
| Ctrl+B / Ctrl+U / Ctrl+I | 太字 / 下線 / 斜体 | Ichitaro 10 list |
| Ctrl+Y | 改ページ | Published list |
| Ctrl+^ / Ctrl+Shift+^ | 検索 / 置換 | Published list |
| Ctrl+F / Ctrl+H | 検索 / 置換 (Windows style) or 段落 (Ichitaro style). Chosen in **ツール→キー割付** | Wikipedia, an article on Ichitaro 2015 |
| Ctrl+¥ | 罫線 (v0.2: make a table) | Published list |
| F7 | フォント・飾り (opens 文字 in the tool palette) | Published list |
| Ctrl+2 | 名前を付けて保存 | Published list |
| Ctrl+S / Ctrl+O / Ctrl+N / Ctrl+P | Save / open / new / print | Ichitaro 10 list |
| Ctrl+Z / Ctrl+R | 取り消し / 繰り返し (repeat the last command) | Ichitaro 10 list |
| Ctrl+J | ジャンプ | Ichitaro 10 list |
| Esc | Open the menu | Ichitaro 10 list |
| Insert | 挿入 ↔ 上書 | Common |

## 4. Saving

| Format | How | Notes |
|---|---|---|
| **Ichitaro (.jtd)** | Patches the original jtd (`save.rs`) | Only for documents opened from a jtd. The default for Ctrl+S (上書き保存) |
| Word (.docx) | Built fresh from the model (`docx.rs`) | Keeps tables, character decoration and ruby |
| PDF | The pages as drawn on screen plus an invisible text layer (`pdf.rs`) | Searchable and copyable; one button, downloads straight away |
| HTML, text, Markdown | From the model | |

**How a jtd is saved** (full rules in `docs/spec/JTD-FORMAT.md` §9)

1. Read the original again, this time with positions (which unit of the original each character
   came from).
2. Flatten the original and the edited document into sequences of "character, paragraph end, table
   row, cell" and diff them.
3. Characters that stayed keep their original bytes, deleted ones are dropped, and new ones go next
   to their neighbours. A character whose formatting changed gets only that attribute changed.
4. Alignment is changed in the paragraph head record (0x10). When joining or splitting paragraphs
   changes the state that the next paragraph inherits, the original state is written back in.
5. The result is read back and compared with the edited document character by character. If
   anything differs, **nothing is saved** and the reason is shown.
6. The position caches (LineMark, PageMark, DocumentTextPositionTables) are removed and the stream
   size table (`\x04JSRV_SegmentInformation`) is updated.

**Edits not yet saved to jtd** (refused with the reason; Word or PDF is suggested instead)
- Joining a line inside a 罫線 cell to a line outside it, or joining lines from different cells
- A line break in the middle of ruby or evenly spaced (均等割付) text
- A new table directly inside or next to a 罫線 cell
- Saving a new document (one started blank) as jtd: there is no original to patch yet, so this is
  still in progress
- Italics: the jtd attribute number is unknown, so the text is saved as plain and the user is told

## 5. Differences caused by what we do not know yet

- Paper size, margins and the 字×行 setting are not read yet, so every document is laid out as
  A4 40×36. Lines may break in different places than in the original.
- Where horizontal rules go is unknown, so horizontal lines in tables are drawn faint.
- Font numbers are not decoded yet, so everything is shown in one 明朝 (Mincho) font.

## 6. Current state (v0.3, 2026-10-01)

**Works**
- New document, open a jtd (drag and drop or Ctrl+O), sheet tabs
- Typing: Japanese IME (text being converted is underlined), paste, overwrite (Insert), Tab (next
  cell inside a table)
- Cursor: arrows, Home/End, PageUp/Down, word jumps, click, drag and double-click to select; up
  and down stay in the same column inside tables
- Editing: line breaks, deleting (joins paragraphs), replacing a selection, undo/redo (100 steps),
  繰り返し (Ctrl+R)
- Formatting: 太字, 斜体, 下線, text size (in list steps), text colour, left/centre/right alignment,
  改ページ
- 罫線: make a table, insert rows above or below, delete rows
- Find and replace (including replace all), character count, insert a date in the Japanese era
  calendar (和暦)
- Screen: menus (Esc, Alt+letter, arrow keys), toolbar, jump palette (page list, document info, a
  warning about the original save path), tool palette, ruler, status bar, editing marks, zoom
  (Ctrl+wheel), fit to phone width
- Saving: **Ichitaro (.jtd)** (patches the original, checked with 一太郎ビューア), Word (.docx),
  **PDF (direct download, searchable text)**, HTML, text, Markdown, printing
- Testing: 14 engine tests, plus 4,000 random edits × 5 runs on 40 public jtd files without a
  crash. Automated browser tests cover typing, IME, tables, saving and menus.

**Not yet**
- Saving a new document as jtd, 罫線モード (drawing lines with the mouse), inserting or deleting
  columns, merging cells
- Vertical writing, pictures, headers and footers, indent and line spacing settings, choosing a
  font
- Auto-scroll while dragging, spell check, packaging as a Mac app (WebView)

## 7. Design (v0.2.1)

Layout and controls come from Ichitaro; **the look is EZPZ File** (the same design language as the
PDF and HWP editors on ezpzfile.com).

- Colours: the same values as ezpzfile.com. Brand blue `#0155ff`, ink scale (`#0b1020` to
  `#f4f6fb`), paper background `#e7eaf1`
- Fonts: Pretendard (when online), then Hiragino Sans / Noto Sans JP
- Icons: Lucide (the same set as the site, ISC licence)
- Top bar: logo, `JTD エディタ`, file name, menus, and on the right undo / 開く / print / a blue
  `保存` button (▾ = 名前を付けて保存), in the same places as in the HWP editor
- Second row toolbar, third row formatting bar (font, size and − + in a grey box), shaped like the
  HWP editor's
- The jump palette on the left shows **page preview cards** like the PDF editor (current page has a
  blue border)
- Tool palette on the right, plus a **vertical tool strip** at the far right (文字, 段落, 罫線, 検索,
  文字数, shaped like the PDF editor's tool strip). Clicking one opens only that section
- Status bar: `n ページ n 行 n 字 | 全 n ページ | 挿入 | 文字数`, with editing marks, fit width and
  zoom on the right
- Dialogs and notices use the site's cards (16 px corners) and dark toast style
