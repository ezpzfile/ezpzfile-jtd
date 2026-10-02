// Words on screen: Japanese (the source) or English.
//
// The language comes from ?lang=en|ja, else from <html lang> (build.sh writes
// one page per language). The Japanese text in app.js and editor.html is the
// key; EN below holds its English. A missing key shows the Japanese as is.
//
//   tr("「{0}」は見つかりません", q)   placeholders {0} {1} … may move in the English
//   translateDom()                     the page's own text, titles and labels

const LANG = (() => {
  let q = null;
  try { q = new URLSearchParams(location.search).get("lang"); } catch {}
  const l = (q || document.documentElement.lang || "ja").toLowerCase();
  return l.startsWith("en") ? "en" : "ja";
})();
document.documentElement.lang = LANG;

const EN = {
  // ---- page
  "JTD エディタ | EZPZ File": "JTD Editor | EZPZ File",
  "JTD エディタ": "JTD Editor",
  "サイトのメニュー": "Site Menu",
  "メニュー": "Menu",
  "未保存の変更": "Unsaved changes",
  "ツールバー": "Toolbar",
  "書式": "Format",
  "ジャンプパレット": "Jump Palette",
  "ツールパレット": "Tool Palette",
  "ツールパレットの項目": "Tool palette sections",
  "本文入力": "Document text",
  "ページ": "Pages",
  "文書情報": "Document Info",
  "Insert キーで切り替え": "Switch with the Insert key",
  "編集記号": "Edit Marks",
  "幅に合わせる": "Fit Width",
  "縮小": "Zoom Out",
  "拡大": "Zoom In",
  "ここにドロップして開く（.jtd）": "Drop here to open (.jtd)",
  "一太郎の文書をここにドロップ": "Drag and drop an Ichitaro file here",
  "一太郎がなくても .jtd を開いて、そのまま直して保存できます。Word や PDF にも書き出せます。": "Open a .jtd file without Ichitaro, edit it and save it. You can also save it as Word or PDF.",
  "ファイルを開く": "Open File",
  "新規文書": "New Document",
  "ファイルはこのブラウザの中だけで処理されます": "Your file never leaves this browser",

  // ---- dialogs
  "名前を付けて保存": "Save As",
  "ファイル名": "File name",
  "一太郎文書 (.jtd)": "Ichitaro document (.jtd)",
  "Word 文書 (.docx)": "Word document (.docx)",
  "表・文字飾り・ルビを保持": "Keeps tables, text styles and ruby",
  "画面と同じ見た目。文字の検索・コピーもできます": "Looks the same as on screen. Text can be searched and copied",
  "Web ページ (.html)": "Web page (.html)",
  "テキスト (.txt)": "Text (.txt)",
  "キャンセル": "Cancel",
  "保存": "Save",
  "表作成": "New table",
  "行数": "Rows",
  "列数": "Columns",
  "罫線で囲んだ表を、カーソルのある段落の下に作ります。": "Adds a table with ruled lines below the paragraph with the cursor.",
  "作成": "Create",
  "確認": "Confirm",
  "保存しない": "Don't Save",
  "情報": "Info",
  "閉じる": "Close",
  "元の文書を書きかえて保存（罫線・書式をそのまま保持）": "Writes over the original document and keeps its ruled lines and formatting",
  "新規文書は準備中（一太郎文書を開いた場合に使えます）": "Not ready for new documents yet (works when you opened an Ichitaro file)",

  // ---- menus
  "ファイル": "File",
  "新規作成": "New",
  "開く...": "Open...",
  "上書き保存": "Save",
  "名前を付けて保存...": "Save As...",
  "印刷...": "Print...",
  "編集": "Edit",
  "元に戻す": "Undo",
  "やり直し": "Redo",
  "繰り返し": "Repeat",
  "切り取り": "Cut",
  "コピー": "Copy",
  "貼り付け": "Paste",
  "すべて選択": "Select All",
  "検索...": "Find...",
  "置換...": "Replace...",
  "ジャンプ": "Jump",
  "表示": "View",
  "ルーラー": "Ruler",
  "ステータスバー": "Status Bar",
  "挿入": "Insert",
  "改ページ": "Page Break",
  "表...": "Table...",
  "日付（和暦）": "Date (Japanese era)",
  "太字": "Bold",
  "斜体": "Italic",
  "下線": "Underline",
  "文字を大きく": "Larger Text",
  "文字を小さく": "Smaller Text",
  "フォント・飾り...": "Font and Style...",
  "左寄せ": "Align Left",
  "センタリング": "Center",
  "右寄せ": "Align Right",
  "罫線": "Table",
  "表作成...": "Create Table...",
  "行を上に挿入": "Insert Row Above",
  "行を下に挿入": "Insert Row Below",
  "行を削除": "Delete Row",
  "罫線モード（準備中）": "Draw Lines (coming later)",
  "ツール": "Tools",
  "キー割付: Windows 標準": "Keys: Windows Standard",
  "キー割付: 一太郎標準": "Keys: Ichitaro Standard",
  "文字数...": "Character Count...",
  "ヘルプ": "Help",
  "ショートカットキー一覧": "Keyboard Shortcuts",
  "JTD エディタについて": "About JTD Editor",

  // ---- top bar and toolbars
  "開く": "Open",
  "開く (Ctrl+O)": "Open (Ctrl+O)",
  "印刷 (Ctrl+P)": "Print (Ctrl+P)",
  "共有": "Share",
  "上書き保存 (Ctrl+S)": "Save (Ctrl+S)",
  "形式を選んで保存": "Save in Another Format",
  "一太郎文書で保存": "Save as Ichitaro document",
  "Word 文書で保存": "Save as Word document",
  "PDF で保存": "Save as PDF",
  "元に戻す (Ctrl+Z)": "Undo (Ctrl+Z)",
  "やり直し (Ctrl+Shift+Z)": "Redo (Ctrl+Shift+Z)",
  "新規作成 (Ctrl+N)": "New (Ctrl+N)",
  "切り取り (Ctrl+X)": "Cut (Ctrl+X)",
  "コピー (Ctrl+C)": "Copy (Ctrl+C)",
  "貼り付け (Ctrl+V)": "Paste (Ctrl+V)",
  "表作成 (Ctrl+¥)": "Create table (Ctrl+¥)",
  "改ページ (Ctrl+Y)": "Page break (Ctrl+Y)",
  "検索": "Find",
  "フォント（いまは明朝のみ）": "Font (Mincho only for now)",
  "フォント": "Font",
  "明朝": "Mincho",
  "文字サイズ": "Text size",
  "文字を小さく (Ctrl+↓)": "Smaller text (Ctrl+↓)",
  "文字を大きく (Ctrl+↑)": "Larger text (Ctrl+↑)",
  "文字色": "Text color",
  "太字 (Ctrl+B)": "Bold (Ctrl+B)",
  "斜体 (Ctrl+I)": "Italic (Ctrl+I)",
  "下線 (Ctrl+U)": "Underline (Ctrl+U)",
  "左寄せ (Ctrl+4)": "Align left (Ctrl+4)",
  "センタリング (Ctrl+5)": "Center (Ctrl+5)",
  "右寄せ (Ctrl+6)": "Align right (Ctrl+6)",

  // ---- tool palette
  "文字": "Text",
  "段落": "Paragraph",
  "サイズ": "Size",
  "飾り": "Style",
  "色": "Color",
  "自動": "Auto",
  "揃え": "Align",
  "左": "Left",
  "中央": "Center",
  "右": "Right",
  "行": "Rows",
  "列": "Columns",
  "表を作成": "Create Table",
  "上に挿入": "Insert above",
  "下に挿入": "Insert below",
  "削除": "Delete",
  "表の中では Tab で次のセルへ移動します。": "In a table, Tab moves to the next cell.",
  "検索・置換": "Find and Replace",
  "検索する文字": "Find what",
  "置換後の文字": "Replace with",
  "前を検索": "Previous",
  "次を検索": "Next",
  "置換": "Replace",
  "すべて置換": "Replace All",
  "文字数": "Characters",
  "全体": "Whole document",
  "選択範囲": "Selection",
  "ツールパレットを隠す": "Hide tool palette",

  // ---- status bar
  "<b>{0}</b> ページ <b>{1}</b> 行 <b>{2}</b> 字": "Page <b>{0}</b> · Line <b>{1}</b> · Col <b>{2}</b>",
  "全 <b>{0}</b> ページ": ["<b>{0}</b> page", "<b>{0}</b> pages"],
  "上書": "Overwrite",
  "用紙: {0} {1}字×{2}行": "Paper: {0}, {1} chars × {2} lines",
  " 横": " landscape",

  // ---- document info, about, counts
  "タイトル": "Title",
  "作成者": "Author",
  "最終保存者": "Last saved by",
  "作成日時": "Created",
  "保存日時": "Saved",
  "印刷日時": "Printed",
  "作成ソフト": "Created with",
  "改訂": "Revision",
  "文書情報はありません。": "No document information.",
  "元の保存場所がファイルに残っています": "The file still records where it was saved",
  "Word などで保存し直すと、この情報は含まれません。": "Saving it again as Word or another format leaves this out.",
  "文字数（空白を除く）": "Characters (without spaces)",
  "ページ数": "Pages",
  "バージョン": "Version",
  "{0} (開発版)": "{0} (preview)",
  "内容": "What it is",
  "一太郎文書（.jtd）を開いて編集できるオープンソースのエディタです。ファイルはこのブラウザの中だけで処理されます。": "An open-source editor that opens and edits Ichitaro documents (.jtd). Your file is processed only inside this browser.",
  "ライセンス": "License",
  "注意": "Note",
  "一太郎は株式会社ジャストシステムの商標です。本ソフトは同社と関係ありません。": "Ichitaro is a trademark of JustSystems Corporation. This software is not affiliated with JustSystems.",
  "ショートカットキー": "Keyboard Shortcuts",
  "センタリング / 右寄せ（{0} 左寄せ）": "Center / align right ({0} aligns left)",
  "文字を大きく / 小さく": "Larger / smaller text",
  "太字 / 斜体 / 下線": "Bold / italic / underline",
  "フォント・飾り": "Font and style",
  "元に戻す / 繰り返し": "Undo / repeat",
  "挿入 / 上書 切り替え": "Switch insert / overwrite",
  "表の次のセルへ": "Next cell in a table",
  "{0} ページ": "Page {0}",

  // ---- messages
  "無題": "Untitled",
  "貼り付けは {0}V を使ってください": "To paste, press {0}V",
  "キー割付: Windows 標準 (Ctrl+F 検索)": "Keys: Windows standard (Ctrl+F finds)",
  "キー割付: 一太郎標準 (Ctrl+F 段落)": "Keys: Ichitaro standard (Ctrl+F opens Paragraph)",
  "表は本文の段落にだけ作成できます": "A table can only go in a body paragraph",
  "「{0}」は見つかりません": "\"{0}\" was not found",
  "{0} 件置換しました": "{0} replaced",
  "切り取りました": "Cut",
  "コピーしました": "Copied",
  "{0}{1} を使ってください": "Press {0}{1}",
  "この文書には元の保存場所が残っています（文書情報）": "This document still records where it was saved (see Document Info)",
  "開けませんでした": "Couldn't open the file",
  "理由": "Reason",
  "一太郎形式で保存できませんでした": "Couldn't save in Ichitaro format",
  "文書": "Document",
  "何も書き出していません。編集内容はこの画面に残っています。": "Nothing was written. Your edits are still on screen.",
  "ほかの方法": "Instead",
  "Word (.docx) や PDF なら保存できます（名前を付けて保存）。": "You can save it as Word (.docx) or PDF (Save As).",
  "{0} を保存しました": "Saved {0}",
  "PDF を作成しています…": "Making the PDF…",
  "PDF を作成できませんでした": "Couldn't make the PDF",
  "変更を保存しますか？": "Save your changes?",
  "「{0}」は変更されています。保存しないと変更は失われます。": "\"{0}\" has changes. If you don't save, they will be lost.",
  "保存する": "Save",
  "起動できませんでした: {0}": "Couldn't start: {0}",
};

const DICT = LANG === "en" ? EN : null;

/** The words for `ja` in the page's language; {0} {1} … are filled from args.
 * An English entry may be [one, many]: the first number among args picks it. */
function tr(ja, ...args) {
  let s = (DICT && DICT[ja]) || ja;
  if (Array.isArray(s)) s = s[args.find((a) => typeof a === "number") === 1 ? 0 : 1];
  return args.length ? s.replace(/\{(\d+)\}/g, (_, i) => String(args[+i] ?? "")) : s;
}

/** Put the page's static text, titles and labels in the page's language. */
function translateDom(root = document.documentElement) {
  if (DICT) {
    const walk = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
      acceptNode: (n) => (n.parentElement && /^(SCRIPT|STYLE)$/.test(n.parentElement.tagName) ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT),
    });
    for (let n = walk.nextNode(); n; n = walk.nextNode()) {
      const key = n.data.trim();
      if (key && typeof DICT[key] === "string") n.data = n.data.replace(key, DICT[key]);
    }
    for (const el of root.querySelectorAll("[title], [placeholder], [aria-label], [alt]")) {
      for (const a of ["title", "placeholder", "aria-label", "alt"]) {
        const v = el.getAttribute(a);
        if (v && typeof DICT[v.trim()] === "string") el.setAttribute(a, DICT[v.trim()]);
      }
    }
  }
  document.documentElement.classList.add("i18n");
}
