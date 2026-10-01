// EZPZ File JTD editor UI.
// The engine (Rust → WebAssembly, `JtdEditor`) owns the document, layout and
// editing. This file only turns user input into engine commands and draws the
// per-page display lists the engine returns.
//
// Expects `__wbg_init`, `JtdEditor` and `WASM_B64` in scope (see build.sh).

const PT = 96 / 72; // CSS px per point at 100 %
const FONT_STACK = '"Hiragino Mincho ProN","Hiragino Mincho Pro","Yu Mincho","YuMincho","MS Mincho","Noto Serif CJK JP","Noto Serif JP",serif';
const IS_MAC = /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent);

const $ = (s, r = document) => r.querySelector(s);
const $$ = (s, r = document) => Array.from(r.querySelectorAll(s));
const el = (tag, attrs = {}, ...kids) => {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (k === "class") e.className = v;
    else if (k === "text") e.textContent = v;
    else if (k.startsWith("on")) e.addEventListener(k.slice(2), v);
    else if (v !== false && v != null) e.setAttribute(k, v === true ? "" : v);
  }
  for (const k of kids) if (k != null) e.append(k);
  return e;
};
const store = {
  get(k, d) { try { const v = localStorage.getItem("ezpzjtd." + k); return v == null ? d : JSON.parse(v); } catch { return d; } },
  set(k, v) { try { localStorage.setItem("ezpzjtd." + k, JSON.stringify(v)); } catch {} },
};

// ------------------------------------------------------------------ state
const S = {
  ed: null,
  name: "無題",
  zoom: store.get("zoom", 1),
  marks: store.get("marks", true),
  keymap: store.get("keymap", "windows"), // "windows" | "ichitaro"
  pages: [],          // {div, canvas, dirty, w, h}
  composing: false,
  lastCmd: null,      // for 繰り返し (Ctrl+R)
  saveFmt: null,
  find: "",
  status: null,
};
const scale = () => S.zoom * PT;

// ------------------------------------------------------------------ boot
function b64bytes(s) { const bin = atob(s); const u = new Uint8Array(bin.length); for (let i = 0; i < bin.length; i++) u[i] = bin.charCodeAt(i); return u; }

async function boot() {
  await (WASM_B64.startsWith("/*") ? __wbg_init() : __wbg_init({ module_or_path: b64bytes(WASM_B64) }));
  buildMenus();
  buildToolbar();
  buildPalette();
  bindInput();
  bindChrome();
  setEditor(new JtdEditor(), "無題");
  fitNarrow();
  focusInput();
}

/** On phones, start zoomed to the page width. */
function fitNarrow() {
  const sc = $("#scroller");
  const pageW = 210 * (72 / 25.4) * PT;
  if (sc.clientWidth && sc.clientWidth < pageW + 48) rezoom(Math.max(0.4, (sc.clientWidth - 32) / pageW));
}

function setEditor(ed, name) {
  if (S.ed) S.ed.free();
  S.ed = ed;
  S.name = name;
  S.ed.setShowMarks(S.marks);
  $("#pages").replaceChildren();
  S.pages = [];
  $("#scroller").scrollTop = 0;
  refresh({ all: true });
  renderInfo();
  renderSheets();
}

// ------------------------------------------------------------------ drawing
function ensurePages() {
  const n = S.ed.pageCount();
  const wrap = $("#pages");
  while (S.pages.length < n) {
    const i = S.pages.length;
    const div = el("div", { class: "page", "data-page": i });
    const canvas = el("canvas");
    div.append(canvas, el("span", { class: "no", text: String(i + 1) }));
    wrap.append(div);
    S.pages.push({ div, canvas, dirty: true });
    observer.observe(div);
  }
  while (S.pages.length > n) {
    const p = S.pages.pop();
    observer.unobserve(p.div);
    p.div.remove();
  }
}

const visible = new Set();
const observer = new IntersectionObserver((ents) => {
  for (const e of ents) {
    const i = +e.target.dataset.page;
    if (e.isIntersecting) { visible.add(i); if (S.pages[i]?.dirty) drawPage(i); }
    else visible.delete(i);
  }
}, { root: null, rootMargin: "600px 0px" });

function drawPage(i) {
  const pg = S.pages[i];
  if (!pg) return;
  const data = JSON.parse(S.ed.pageJson(i));
  if (!data) return;
  const s = scale();
  const dpr = window.devicePixelRatio || 1;
  const W = Math.round(data.w * s), H = Math.round(data.h * s);
  pg.div.style.width = W + "px";
  pg.div.style.height = H + "px";
  const c = pg.canvas;
  if (c.width !== Math.round(W * dpr) || c.height !== Math.round(H * dpr)) {
    c.width = Math.round(W * dpr); c.height = Math.round(H * dpr);
    c.style.width = W + "px"; c.style.height = H + "px";
  }
  const g = c.getContext("2d");
  g.setTransform(dpr * s, 0, 0, dpr * s, 0, 0);
  g.clearRect(0, 0, data.w, data.h);
  paintItems(g, data, S.marks);
  pg.dirty = false;
}

/** Paint a display list in page coordinates (points). */
function paintItems(g, data, marks) {
  const setup = S.setup || (S.setup = JSON.parse(S.ed.setupJson()));
  if (marks) {
    // text-area corner marks (like most Japanese word processors)
    const mm = 72 / 25.4;
    const L = (setup.width_mm * mm - setup.chars_per_line * setup.font_pt) / 2;
    const R = setup.width_mm * mm - L, T = setup.margin_top_mm * mm, B = setup.height_mm * mm - setup.margin_bottom_mm * mm;
    g.strokeStyle = "#a8b0c4"; g.lineWidth = 0.5; g.beginPath();
    for (const [x, y, dx, dy] of [[L, T, -1, -1], [R, T, 1, -1], [L, B, -1, 1], [R, B, 1, 1]]) {
      g.moveTo(x + dx * 10, y); g.lineTo(x, y); g.lineTo(x, y + dy * 10);
    }
    g.stroke();
  }
  g.textBaseline = "alphabetic";
  for (const it of data.items) {
    if (it.t === "Text") {
      g.font = `${it.i ? "italic " : ""}${it.b ? "bold " : ""}${it.size}px ${FONT_STACK}`;
      g.fillStyle = it.color || "#111";
      const chars = Array.from(it.s);
      for (let k = 0; k < chars.length; k++) {
        const ch = chars[k];
        const half = ch.charCodeAt(0) < 0x2000 || (ch >= "｡" && ch <= "ﾟ");
        const x = it.xs[k];
        if (half) {
          // Half-width cell: centre narrow glyphs, squeeze wide ones (W, M, …)
          // so text keeps the monospaced grid look of Ichitaro.
          const cell = it.size / 2, w = g.measureText(ch).width;
          if (w > cell) { g.save(); g.translate(x, it.y); g.scale(cell / w, 1); g.fillText(ch, 0, 0); g.restore(); }
          else g.fillText(ch, x + (cell - w) / 2, it.y);
        } else g.fillText(ch, x, it.y);
      }
    } else if (it.t === "Line") {
      g.strokeStyle = it.color || "#111"; g.lineWidth = it.w;
      g.beginPath(); g.moveTo(it.x1, it.y1); g.lineTo(it.x2, it.y2); g.stroke();
    } else if (it.t === "Mark" && marks) {
      g.strokeStyle = "#6fa5ff"; g.fillStyle = "#6fa5ff"; g.lineWidth = 0.6;
      const z = it.size;
      if (it.k === "ret") { // 改行マーク
        const x = it.x + 1.5, y = it.y - z * 0.1;
        g.beginPath(); g.moveTo(x + z * 0.4, y - z * 0.55); g.lineTo(x + z * 0.4, y); g.lineTo(x, y); g.stroke();
        g.beginPath(); g.moveTo(x, y); g.lineTo(x + z * 0.16, y - z * 0.12); g.lineTo(x + z * 0.16, y + z * 0.12); g.closePath(); g.fill();
      } else if (it.k === "sp") { // 全角スペース □ (kept faint: forms are full of them)
        g.save(); g.globalAlpha = 0.55;
        g.strokeRect(it.x + z * 0.15, it.y - z * 0.72, z * 0.7, z * 0.7);
        g.restore();
      } else if (it.k === "tab") {
        g.beginPath(); g.moveTo(it.x + 2, it.y - z * 0.35); g.lineTo(it.x + z * 1.2, it.y - z * 0.35); g.stroke();
      } else if (it.k === "pb") {
        g.setLineDash([3, 2]); g.beginPath(); g.moveTo(it.x, it.y); g.lineTo(it.x + it.size, it.y); g.stroke(); g.setLineDash([]);
        g.font = `7px ${FONT_STACK}`; g.fillText("改ページ", it.x + it.size / 2 - 14, it.y - 2);
      }
    }
  }
}

function drawOverlays() {
  const s = scale();
  $$(".page .sel").forEach((e) => e.remove());
  for (const r of JSON.parse(S.ed.selectionJson())) {
    const pg = S.pages[r.page]; if (!pg) continue;
    pg.div.append(el("div", { class: "sel", style: `left:${r.x * s}px;top:${r.y * s}px;width:${r.w * s}px;height:${r.h * s}px` }));
  }
  const caret = $("#caret");
  const c = JSON.parse(S.ed.caretJson());
  const st = S.status;
  if (!c || !S.pages[c.page] || (st && st.has_selection)) { caret.hidden = true; placeIme(c); return; }
  const pg = S.pages[c.page].div;
  const x = pg.offsetLeft + c.x * s, y = pg.offsetTop + c.y * s;
  caret.hidden = S.composing;
  caret.className = st && st.overwrite ? "ovr" : "ins";
  caret.style.left = (st && st.overwrite ? x : x - 1) + "px";
  caret.style.top = y + "px";
  caret.style.height = c.h * s + "px";
  if (st && st.overwrite) caret.style.width = Math.max(2, 10.5 * s) + "px"; else caret.style.width = "";
  // restart blink
  caret.style.animation = "none"; void caret.offsetWidth; caret.style.animation = "";
  placeIme(c, x, y, c.h * s);
}

function placeIme(c, x, y, h) {
  const ime = $("#ime");
  if (x == null) return;
  ime.style.left = x + "px"; ime.style.top = y + "px"; ime.style.height = h + "px";
  ime.style.fontSize = Math.max(12, h * 0.7) + "px";
}

function scrollCaretIntoView() {
  const caret = $("#caret"), sc = $("#scroller");
  const c = JSON.parse(S.ed.caretJson());
  if (!c || !S.pages[c.page]) return;
  const s = scale(), pg = S.pages[c.page].div;
  const y = pg.offsetTop + c.y * s, h = c.h * s, x = pg.offsetLeft + c.x * s;
  if (y < sc.scrollTop + 8) sc.scrollTop = y - 40;
  else if (y + h > sc.scrollTop + sc.clientHeight - 8) sc.scrollTop = y + h - sc.clientHeight + 40;
  if (x < sc.scrollLeft) sc.scrollLeft = x - 40;
  else if (x > sc.scrollLeft + sc.clientWidth - 20) sc.scrollLeft = x - sc.clientWidth + 60;
  void caret;
}

/** After any change: update pages, overlays, status and palettes. */
function refresh({ all = false, scroll = true } = {}) {
  ensurePages();
  const s = scale();
  const setup = S.setup || (S.setup = JSON.parse(S.ed.setupJson()));
  const mm = 72 / 25.4, W = Math.round(setup.width_mm * mm * s) + "px", H = Math.round(setup.height_mm * mm * s) + "px";
  for (const pg of S.pages) {
    pg.dirty = true; // layout may have changed anywhere; only visible pages are redrawn now
    if (pg.div.style.width !== W) { pg.div.style.width = W; pg.div.style.height = H; }
  }
  void all;
  for (const i of visible) drawPage(i);
  // make sure the caret page is drawn even before the observer fires
  const c = JSON.parse(S.ed.caretJson());
  if (c && S.pages[c.page]?.dirty) drawPage(c.page);
  S.status = JSON.parse(S.ed.statusJson());
  drawOverlays();
  if (scroll) scrollCaretIntoView();
  updateStatus();
  updateStyleUi();
  drawRuler();
  schedulePreviews();
}

function rezoom(z) {
  S.zoom = Math.min(2, Math.max(0.4, Math.round(z * 20) / 20));
  store.set("zoom", S.zoom);
  refresh();
}

// ------------------------------------------------------------------ ruler
function drawRuler() {
  const box = $("#ruler"), c = $("#ruler canvas");
  if (!S.pages[0]) return;
  const dpr = window.devicePixelRatio || 1, W = box.clientWidth, H = box.clientHeight;
  if (!W) return;
  c.width = W * dpr; c.height = H * dpr; c.style.width = W + "px"; c.style.height = H + "px";
  const g = c.getContext("2d"); g.setTransform(dpr, 0, 0, dpr, 0, 0); g.clearRect(0, 0, W, H);
  const setup = S.setup || (S.setup = JSON.parse(S.ed.setupJson()));
  const s = scale(), sc = $("#scroller"), pg = S.pages[0].div;
  const mm = 72 / 25.4;
  const left = (setup.width_mm * mm - setup.chars_per_line * setup.font_pt) / 2;
  const x0 = pg.offsetLeft - sc.scrollLeft + left * s, cw = setup.font_pt * s;
  const css = getComputedStyle(document.documentElement);
  g.fillStyle = css.getPropertyValue("--ink-50"); g.fillRect(x0, 4, cw * setup.chars_per_line, H - 8);
  g.strokeStyle = css.getPropertyValue("--ink-300"); g.fillStyle = css.getPropertyValue("--ink-500");
  g.font = "9px sans-serif"; g.lineWidth = 1; g.beginPath();
  for (let k = 0; k <= setup.chars_per_line; k++) {
    const x = Math.round(x0 + k * cw) + 0.5, len = k % 10 === 0 ? 8 : k % 5 === 0 ? 5 : 3;
    g.moveTo(x, H - 3); g.lineTo(x, H - 3 - len);
    if (k % 10 === 0 && k) g.fillText(String(k), x - 5, 10);
  }
  g.stroke();
  if (S.status) {
    const x = x0 + (S.status.col - 0.5) * cw;
    g.fillStyle = css.getPropertyValue("--brand-500"); g.beginPath(); g.moveTo(x - 4, H - 1); g.lineTo(x + 4, H - 1); g.lineTo(x, H - 7); g.fill();
  }
}

// ------------------------------------------------------------------ status / palettes
function updateStatus() {
  const st = S.status;
  $("#st-page").textContent = st.page; $("#st-line").textContent = st.line; $("#st-col").textContent = st.col;
  $("#st-pages").textContent = st.pages; $("#st-chars").textContent = st.chars.toLocaleString();
  $("#st-mode").textContent = st.overwrite ? "上書" : "挿入";
  $("#st-zv").textContent = Math.round(S.zoom * 100) + "%";
  $("#st-marks").classList.toggle("on", S.marks);
  $("#topbar").classList.toggle("modified", st.modified);
  $("#docname").textContent = S.name;
  document.title = `${st.modified ? "● " : ""}${S.name} — JTD エディタ`;
  for (const [id, on] of [["undo", st.can_undo], ["redo", st.can_redo]]) {
    $$(`button[data-cmd="${id}"]`).forEach((b) => (b.disabled = !on));
  }
  $$('[data-cmd^="row"]').forEach((b) => (b.disabled = !st.in_table));
  $$("#pagelist li").forEach((li, i) => li.classList.toggle("cur", i === st.page - 1));
  $$("#sheettabs button").forEach((b, i) => b.classList.toggle("on", i === st.sheet));
}

function updateStyleUi() {
  const s = JSON.parse(S.ed.styleJson());
  const on = (cmd, v) => $$(`[data-cmd="${cmd}"]`).forEach((b) => b.classList.toggle("on", !!v));
  on("bold", s.bold); on("italic", s.italic); on("underline", s.underline);
  on("alignLeft", s.align === "left" || s.align === "other"); on("alignCenter", s.align === "center"); on("alignRight", s.align === "right");
  on("marks", S.marks);
  const size = String(s.size);
  for (const sel of $$(".size-select")) { if (!Array.from(sel.options).some((o) => o.value === size)) sel.append(el("option", { value: size, text: size })); sel.value = size; }
  $$(".swatchline").forEach((i) => (i.style.background = s.color || "#0b1020"));
  const sel = S.ed.selectedText();
  $("#cnt-sel").textContent = sel ? Array.from(sel.replace(/\s/g, "")).length.toLocaleString() : "0";
  $("#cnt-all").textContent = S.status.chars.toLocaleString();
  $("#cnt-pages").textContent = S.status.pages;
}

let previewTimer = 0;
const thumbObserver = new IntersectionObserver((ents) => {
  for (const e of ents) if (e.isIntersecting && e.target.dataset.dirty === "1") drawThumb(e.target);
}, { root: null, rootMargin: "200px" });
function drawThumb(li) {
  const i = +li.dataset.page;
  const data = JSON.parse(S.ed.pageJson(i));
  if (!data) return;
  const c = $("canvas", li), w = c.clientWidth || 172, dpr = window.devicePixelRatio || 1;
  const k = w / data.w;
  c.width = Math.round(w * dpr); c.height = Math.round(data.h * k * dpr);
  const g = c.getContext("2d");
  g.setTransform(k * dpr, 0, 0, k * dpr, 0, 0);
  paintItems(g, data, false);
  li.dataset.dirty = "0";
}
function schedulePreviews() {
  clearTimeout(previewTimer);
  previewTimer = setTimeout(() => {
    const n = S.ed.pageCount();
    const ul = $("#pagelist");
    while (ul.children.length > n) { thumbObserver.unobserve(ul.lastChild); ul.lastChild.remove(); }
    while (ul.children.length < n) {
      const i = ul.children.length;
      const li = el("li", { "data-page": i, title: `${i + 1} ページ` }, el("span", { class: "thumb" }, el("canvas")), el("span", { class: "n", text: String(i + 1) }));
      li.addEventListener("click", () => { S.ed.gotoPage(i); refresh({ scroll: false }); focusInput(); S.pages[i].div.scrollIntoView({ block: "start" }); });
      ul.append(li); thumbObserver.observe(li);
    }
    for (const li of ul.children) {
      li.dataset.dirty = "1";
      const r = li.getBoundingClientRect();
      if (r.bottom > 0 && r.top < window.innerHeight && !$("#jump").hidden && li.offsetParent) drawThumb(li);
    }
    $$("#pagelist li").forEach((li, i) => li.classList.toggle("cur", i === S.status.page - 1));
  }, 400);
}

function renderInfo() {
  const s = JSON.parse(S.ed.summaryJson());
  const rows = [["タイトル", s.title], ["作成者", s.author], ["最終保存者", s.last_author], ["作成", s.created], ["保存", s.saved], ["印刷", s.printed], ["作成ソフト", s.application], ["改訂", s.revision]].filter((r) => r[1]);
  const pane = $("#infopane");
  pane.replaceChildren();
  if (!rows.length && !s.template) { pane.append(el("p", { class: "note", text: "文書情報はありません。" })); return; }
  const kv = el("div", { class: "kv" });
  for (const [k, v] of rows) kv.append(el("span", { text: k }), el("span", { text: v }));
  pane.append(kv);
  if (s.template && /[\\:]/.test(s.template)) {
    pane.append(el("div", { class: "warnbox" }, el("b", { text: "元の保存場所がファイルに残っています" }), el("span", { text: s.template }),
      el("span", { text: "Word などで保存し直すと、この情報は含まれません。" })));
  }
}

function renderSheets() {
  const st = JSON.parse(S.ed.statusJson());
  const bar = $("#sheettabs");
  bar.classList.toggle("show", st.sheets.length > 1);
  bar.replaceChildren(...st.sheets.map((n, i) => el("button", { text: n, onclick: () => { S.ed.setSheet(i); refresh({ all: true }); focusInput(); } })));
}

// ------------------------------------------------------------------ commands
const CMDS = {
  new: () => guardUnsaved(() => { setEditor(new JtdEditor(), "無題"); }),
  open: () => guardUnsaved(() => $("#fileinput").click()),
  save: () => save(false),
  saveAs: () => save(true),
  print: () => printDoc(),
  docInfo: () => { switchJump("info"); if ($("#main").classList.contains("no-jump")) toggleView("jump"); },
  undo: () => edit(() => S.ed.undo(), false),
  redo: () => edit(() => S.ed.redo(), false),
  repeat: () => { if (S.lastCmd) CMDS[S.lastCmd](); else edit(() => S.ed.redo(), false); },
  cut: () => clip("cut"),
  copy: () => clip("copy"),
  paste: () => toast(`貼り付けは ${IS_MAC ? "⌘" : "Ctrl+"}V を使ってください`),
  selectAll: () => edit(() => S.ed.selectAll(), false),
  find: () => openFind(false),
  replace: () => openFind(true),
  jump: () => { if ($("#main").classList.contains("no-jump")) toggleView("jump"); switchJump("pages"); $("#pagelist li.cur")?.scrollIntoView({ block: "nearest" }); },
  marks: () => { S.marks = !S.marks; store.set("marks", S.marks); S.ed.setShowMarks(S.marks); refresh({ all: true, scroll: false }); },
  ruler: () => toggleView("ruler"),
  jumpPalette: () => toggleView("jump"),
  toolPalette: () => toggleView("palette"),
  statusBar: () => toggleView("status"),
  zoomIn: () => rezoom(S.zoom + 0.1),
  zoomOut: () => rezoom(S.zoom - 0.1),
  zoom100: () => rezoom(1),
  zoomFit: () => { const sc = $("#scroller"); rezoom((sc.clientWidth - 64) / (210 * (72 / 25.4) * PT)); },
  pageBreak: () => edit(() => S.ed.pageBreak(), true, "pageBreak"),
  table: () => openTableDialog(),
  date: () => edit(() => S.ed.insertText(wareki(new Date())), true),
  bold: () => edit(() => S.ed.bold(), true, "bold"),
  italic: () => edit(() => S.ed.italic(), true, "italic"),
  underline: () => edit(() => S.ed.underline(), true, "underline"),
  sizeUp: () => edit(() => S.ed.sizeStep(true), true, "sizeUp"),
  sizeDown: () => edit(() => S.ed.sizeStep(false), true, "sizeDown"),
  alignLeft: () => edit(() => S.ed.align("left"), true, "alignLeft"),
  alignCenter: () => edit(() => S.ed.align("center"), true, "alignCenter"),
  alignRight: () => edit(() => S.ed.align("right"), true, "alignRight"),
  fontPalette: () => openSection("sec-char"),
  paraPalette: () => openSection("sec-para"),
  rowAbove: () => edit(() => S.ed.insertRow(false), true),
  rowBelow: () => edit(() => S.ed.insertRow(true), true),
  rowDelete: () => edit(() => S.ed.deleteRow(), true),
  keymapWindows: () => setKeymap("windows"),
  keymapIchitaro: () => setKeymap("ichitaro"),
  count: () => showInfo("文字数", [["文字数（空白を除く）", S.status.chars.toLocaleString()], ["ページ数", S.status.pages], ["選択範囲", $("#cnt-sel").textContent]]),
  shortcuts: () => showShortcuts(),
  about: () => showInfo("JTD エディタについて", [["バージョン", "0.3 (開発版)"], ["内容", "一太郎文書（.jtd）を開いて編集できるオープンソースのエディタです。ファイルはこのブラウザの中だけで処理されます。"], ["ライセンス", "MIT / Apache-2.0"], ["注意", "一太郎は株式会社ジャストシステムの商標です。本ソフトは同社と関係ありません。"]]),
};

function edit(fn, record = true, name = null) {
  fn();
  if (name) S.lastCmd = name;
  refresh();
  focusInput();
}

function toggleView(which) {
  if (which === "ruler") $("#main").classList.toggle("no-ruler");
  if (which === "jump") $("#main").classList.toggle("no-jump");
  if (which === "palette") $("#main").classList.toggle("no-palette");
  if (which === "status") $("#app").classList.toggle("no-status");
  updateMenuChecks();
  syncRail();
  setTimeout(() => { drawRuler(); schedulePreviews(); }, 0);
}

function setKeymap(k) { S.keymap = k; store.set("keymap", k); updateMenuChecks(); buildMenus(); toast(k === "windows" ? "キー割付: Windows 標準 (Ctrl+F 検索)" : "キー割付: 一太郎標準 (Ctrl+F 段落)"); }

function wareki(d) {
  const y = d.getFullYear(), m = d.getMonth() + 1, day = d.getDate();
  const reiwa = y - 2018;
  return `令和${reiwa === 1 ? "元" : reiwa}年${m}月${day}日`;
}

// ------------------------------------------------------------------ menus
const SC = (k) => (IS_MAC ? k.replace("Ctrl+", "⌘").replace("Shift+", "⇧") : k);
function menuDefs() {
  const find = S.keymap === "windows" ? "Ctrl+F" : "Ctrl+^";
  const repl = S.keymap === "windows" ? "Ctrl+H" : "Ctrl+Shift+^";
  return [
    ["ファイル", "F", [["new", "新規作成", "Ctrl+N"], ["open", "開く...", "Ctrl+O"], "-", ["save", "上書き保存", "Ctrl+S"], ["saveAs", "名前を付けて保存...", "Ctrl+2"], "-", ["docInfo", "文書情報"], "-", ["print", "印刷...", "Ctrl+P"]]],
    ["編集", "E", [["undo", "元に戻す", "Ctrl+Z"], ["redo", "やり直し", "Ctrl+Shift+Z"], ["repeat", "繰り返し", "Ctrl+R"], "-", ["cut", "切り取り", "Ctrl+X"], ["copy", "コピー", "Ctrl+C"], ["paste", "貼り付け", "Ctrl+V"], "-", ["selectAll", "すべて選択", "Ctrl+A"], "-", ["find", "検索...", find], ["replace", "置換...", repl], ["jump", "ジャンプ", "Ctrl+J"]]],
    ["表示", "V", [["marks", "編集記号", "", "check"], ["ruler", "ルーラー", "", "check"], ["jumpPalette", "ジャンプパレット", "", "check"], ["toolPalette", "ツールパレット", "", "check"], ["statusBar", "ステータスバー", "", "check"], "-", ["zoomIn", "拡大"], ["zoomOut", "縮小"], ["zoom100", "100%"], ["zoomFit", "幅に合わせる"]]],
    ["挿入", "I", [["pageBreak", "改ページ", "Ctrl+Y"], ["table", "表...", "Ctrl+¥"], ["date", "日付（和暦）"]]],
    ["書式", "O", [["bold", "太字", "Ctrl+B"], ["italic", "斜体", "Ctrl+I"], ["underline", "下線", "Ctrl+U"], "-", ["sizeUp", "文字を大きく", "Ctrl+↑"], ["sizeDown", "文字を小さく", "Ctrl+↓"], ["fontPalette", "フォント・飾り...", "F7"], "-", ["alignLeft", "左寄せ", "Ctrl+4"], ["alignCenter", "センタリング", "Ctrl+5"], ["alignRight", "右寄せ", "Ctrl+6"]]],
    ["罫線", "K", [["table", "表作成...", "Ctrl+¥"], "-", ["rowAbove", "行を上に挿入"], ["rowBelow", "行を下に挿入"], ["rowDelete", "行を削除"], "-", ["drawRules", "罫線モード（準備中）", "", "disabled"]]],
    ["ツール", "T", [["keymapWindows", "キー割付: Windows 標準", "", "radio"], ["keymapIchitaro", "キー割付: 一太郎標準", "", "radio"], "-", ["count", "文字数..."]]],
    ["ヘルプ", "H", [["shortcuts", "ショートカットキー一覧"], ["about", "JTD エディタについて"]]],
  ];
}

let openMenu = null;
function buildMenus() {
  const bar = $("#menubar");
  bar.replaceChildren();
  for (const [label, key, items] of menuDefs()) {
    const m = el("div", { class: "menu" });
    const btn = el("button", { "data-key": key, "aria-haspopup": "menu" }, label, el("span", { class: "k", text: `(${key})` }));
    const drop = el("div", { class: "drop", role: "menu" });
    for (const it of items) {
      if (it === "-") { drop.append(el("hr")); continue; }
      const [cmd, text, sc, kind] = it;
      const b = el("button", { "data-cmd": cmd, "data-kind": kind || "", role: "menuitem", disabled: kind === "disabled" });
      b.innerHTML = `<span class="chk">${ic("check")}</span><span class="lbl"></span><span class="sc"></span>`;
      $(".lbl", b).textContent = text;
      $(".sc", b).textContent = sc ? SC(sc) : "";
      b.addEventListener("click", () => { closeMenus(); CMDS[cmd]?.(); });
      drop.append(b);
    }
    btn.addEventListener("mousedown", (e) => { e.preventDefault(); openMenu === m ? closeMenus() : showMenu(m); });
    btn.addEventListener("mouseenter", () => { if (openMenu && openMenu !== m) showMenu(m); });
    m.append(btn, drop);
    bar.append(m);
  }
  updateMenuChecks();
}

function showMenu(m, focusFirst = false) {
  closeMenus(false);
  openMenu = m; m.classList.add("open");
  // the menu bar scrolls on narrow screens, so drop-downs are placed in the viewport
  const r = $("button", m).getBoundingClientRect(), d = $(".drop", m);
  d.style.top = r.bottom + 4 + "px";
  d.style.left = Math.max(8, Math.min(r.left, window.innerWidth - d.offsetWidth - 8)) + "px";
  updateMenuChecks();
  if (focusFirst) { const first = $(".drop button:not(:disabled)", m); first?.classList.add("active"); }
}

function closeMenus(refocus = true) {
  $$(".menu.open").forEach((m) => m.classList.remove("open"));
  $$(".drop button.active").forEach((b) => b.classList.remove("active"));
  const had = !!openMenu; openMenu = null;
  if (refocus && had) focusInput();
}
function updateMenuChecks() {
  const checks = { marks: S.marks, ruler: !$("#main").classList.contains("no-ruler"), jumpPalette: !$("#main").classList.contains("no-jump"),
    toolPalette: !$("#main").classList.contains("no-palette"), statusBar: !$("#app").classList.contains("no-status"),
    keymapWindows: S.keymap === "windows", keymapIchitaro: S.keymap === "ichitaro" };
  for (const [cmd, v] of Object.entries(checks)) $$(`.drop [data-cmd="${cmd}"]`).forEach((b) => b.classList.toggle("checked", v));
}
function menuKey(e) {
  if (!openMenu) return false;
  const menus = $$(".menu"), mi = menus.indexOf(openMenu);
  const items = $$(".drop button:not(:disabled)", openMenu);
  let ai = items.findIndex((b) => b.classList.contains("active"));
  const setActive = (i) => { items.forEach((b) => b.classList.remove("active")); items[(i + items.length) % items.length]?.classList.add("active"); };
  switch (e.key) {
    case "Escape": closeMenus(); break;
    case "ArrowDown": setActive(ai + 1); break;
    case "ArrowUp": setActive(ai < 0 ? items.length - 1 : ai - 1); break;
    case "ArrowRight": showMenu(menus[(mi + 1) % menus.length], true); break;
    case "ArrowLeft": showMenu(menus[(mi - 1 + menus.length) % menus.length], true); break;
    case "Enter": if (ai >= 0) items[ai].click(); break;
    default: {
      const k = e.key.toUpperCase();
      const target = menus.find((m) => $("button", m).dataset.key === k);
      if (target) showMenu(target, true);
    }
  }
  e.preventDefault();
  return true;
}

// ------------------------------------------------------------------ toolbar
const ICONS = {"file-plus": "<path d=\"M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z\" /> <path d=\"M14 2v4a2 2 0 0 0 2 2h4\" /> <path d=\"M9 15h6\" /> <path d=\"M12 18v-6\" />", "folder-open": "<path d=\"m6 14 1.5-2.9A2 2 0 0 1 9.24 10H20a2 2 0 0 1 1.94 2.5l-1.54 6a2 2 0 0 1-1.95 1.5H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h3.9a2 2 0 0 1 1.69.9l.81 1.2a2 2 0 0 0 1.67.9H18a2 2 0 0 1 2 2v2\" />", "save": "<path d=\"M15.2 3a2 2 0 0 1 1.4.6l3.8 3.8a2 2 0 0 1 .6 1.4V19a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z\" /> <path d=\"M17 21v-7a1 1 0 0 0-1-1H8a1 1 0 0 0-1 1v7\" /> <path d=\"M7 3v4a1 1 0 0 0 1 1h7\" />", "printer": "<path d=\"M6 18H4a2 2 0 0 1-2-2v-5a2 2 0 0 1 2-2h16a2 2 0 0 1 2 2v5a2 2 0 0 1-2 2h-2\" /> <path d=\"M6 9V3a1 1 0 0 1 1-1h10a1 1 0 0 1 1 1v6\" /> <rect x=\"6\" y=\"14\" width=\"12\" height=\"8\" rx=\"1\" />", "undo-2": "<path d=\"M9 14 4 9l5-5\" /> <path d=\"M4 9h10.5a5.5 5.5 0 0 1 5.5 5.5a5.5 5.5 0 0 1-5.5 5.5H11\" />", "redo-2": "<path d=\"m15 14 5-5-5-5\" /> <path d=\"M20 9H9.5A5.5 5.5 0 0 0 4 14.5A5.5 5.5 0 0 0 9.5 20H13\" />", "scissors": "<circle cx=\"6\" cy=\"6\" r=\"3\" /> <path d=\"M8.12 8.12 12 12\" /> <path d=\"M20 4 8.12 15.88\" /> <circle cx=\"6\" cy=\"18\" r=\"3\" /> <path d=\"M14.8 14.8 20 20\" />", "copy": "<rect width=\"14\" height=\"14\" x=\"8\" y=\"8\" rx=\"2\" ry=\"2\" /> <path d=\"M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2\" />", "clipboard-paste": "<path d=\"M15 2H9a1 1 0 0 0-1 1v2c0 .6.4 1 1 1h6c.6 0 1-.4 1-1V3c0-.6-.4-1-1-1Z\" /> <path d=\"M8 4H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2M16 4h2a2 2 0 0 1 2 2v2M11 14h10\" /> <path d=\"m17 10 4 4-4 4\" />", "table": "<path d=\"M12 3v18\" /> <rect width=\"18\" height=\"18\" x=\"3\" y=\"3\" rx=\"2\" /> <path d=\"M3 9h18\" /> <path d=\"M3 15h18\" />", "separator-horizontal": "<line x1=\"3\" x2=\"21\" y1=\"12\" y2=\"12\" /> <polyline points=\"8 8 12 4 16 8\" /> <polyline points=\"16 16 12 20 8 16\" />", "calendar": "<path d=\"M8 2v4\" /> <path d=\"M16 2v4\" /> <rect width=\"18\" height=\"18\" x=\"3\" y=\"4\" rx=\"2\" /> <path d=\"M3 10h18\" />", "search": "<circle cx=\"11\" cy=\"11\" r=\"8\" /> <path d=\"m21 21-4.3-4.3\" />", "pilcrow": "<path d=\"M13 4v16\" /> <path d=\"M17 4v16\" /> <path d=\"M19 4H9.5a4.5 4.5 0 0 0 0 9H13\" />", "bold": "<path d=\"M6 12h9a4 4 0 0 1 0 8H7a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1h7a4 4 0 0 1 0 8\" />", "italic": "<line x1=\"19\" x2=\"10\" y1=\"4\" y2=\"4\" /> <line x1=\"14\" x2=\"5\" y1=\"20\" y2=\"20\" /> <line x1=\"15\" x2=\"9\" y1=\"4\" y2=\"20\" />", "underline": "<path d=\"M6 4v6a6 6 0 0 0 12 0V4\" /> <line x1=\"4\" x2=\"20\" y1=\"20\" y2=\"20\" />", "baseline": "<path d=\"M4 20h16\" /> <path d=\"m6 16 6-12 6 12\" /> <path d=\"M8 12h8\" />", "align-left": "<path d=\"M15 12H3\" /> <path d=\"M17 18H3\" /> <path d=\"M21 6H3\" />", "align-center": "<path d=\"M17 12H7\" /> <path d=\"M19 18H5\" /> <path d=\"M21 6H3\" />", "align-right": "<path d=\"M21 12H9\" /> <path d=\"M21 18H7\" /> <path d=\"M21 6H3\" />", "minus": "<path d=\"M5 12h14\" />", "plus": "<path d=\"M5 12h14\" /> <path d=\"M12 5v14\" />", "chevron-down": "<path d=\"m6 9 6 6 6-6\" />", "chevron-right": "<path d=\"m9 18 6-6-6-6\" />", "panel-left": "<rect width=\"18\" height=\"18\" x=\"3\" y=\"3\" rx=\"2\" /> <path d=\"M9 3v18\" />", "panel-right": "<rect width=\"18\" height=\"18\" x=\"3\" y=\"3\" rx=\"2\" /> <path d=\"M15 3v18\" />", "move-horizontal": "<path d=\"m18 8 4 4-4 4\" /> <path d=\"M2 12h20\" /> <path d=\"m6 8-4 4 4 4\" />", "type": "<polyline points=\"4 7 4 4 20 4 20 7\" /> <line x1=\"9\" x2=\"15\" y1=\"20\" y2=\"20\" /> <line x1=\"12\" x2=\"12\" y1=\"4\" y2=\"20\" />", "table-2": "<path d=\"M9 3H5a2 2 0 0 0-2 2v4m6-6h10a2 2 0 0 1 2 2v4M9 3v18m0 0h10a2 2 0 0 0 2-2V9M9 21H5a2 2 0 0 1-2-2V9m0 0h18\" />", "hash": "<line x1=\"4\" x2=\"20\" y1=\"9\" y2=\"9\" /> <line x1=\"4\" x2=\"20\" y1=\"15\" y2=\"15\" /> <line x1=\"10\" x2=\"8\" y1=\"3\" y2=\"21\" /> <line x1=\"16\" x2=\"14\" y1=\"3\" y2=\"21\" />", "info": "<circle cx=\"12\" cy=\"12\" r=\"10\" /> <path d=\"M12 16v-4\" /> <path d=\"M12 8h.01\" />", "between-horizontal-start": "<rect width=\"13\" height=\"7\" x=\"8\" y=\"3\" rx=\"1\" /> <path d=\"m2 9 3 3-3 3\" /> <rect width=\"13\" height=\"7\" x=\"8\" y=\"14\" rx=\"1\" />", "between-horizontal-end": "<rect width=\"13\" height=\"7\" x=\"3\" y=\"3\" rx=\"1\" /> <path d=\"m22 15-3-3 3-3\" /> <rect width=\"13\" height=\"7\" x=\"3\" y=\"14\" rx=\"1\" />", "trash-2": "<path d=\"M3 6h18\" /> <path d=\"M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6\" /> <path d=\"M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2\" /> <line x1=\"10\" x2=\"10\" y1=\"11\" y2=\"17\" /> <line x1=\"14\" x2=\"14\" y1=\"11\" y2=\"17\" />", "x": "<path d=\"M18 6 6 18\" /> <path d=\"m6 6 12 12\" />", "keyboard": "<path d=\"M10 8h.01\" /> <path d=\"M12 12h.01\" /> <path d=\"M14 8h.01\" /> <path d=\"M16 12h.01\" /> <path d=\"M18 8h.01\" /> <path d=\"M6 8h.01\" /> <path d=\"M7 16h10\" /> <path d=\"M8 12h.01\" /> <rect width=\"20\" height=\"16\" x=\"2\" y=\"4\" rx=\"2\" />", "check": "<path d=\"M20 6 9 17l-5-5\" />", "file-text": "<path d=\"M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z\" /> <path d=\"M14 2v4a2 2 0 0 0 2 2h4\" /> <path d=\"M10 9H8\" /> <path d=\"M16 13H8\" /> <path d=\"M16 17H8\" />"};
/** Lucide icon (ISC licence), same set as ezpzfile.com. */
const ic = (n, cls = "i") => `<svg class="${cls}" viewBox="0 0 24 24" aria-hidden="true">${ICONS[n] || ""}</svg>`;

function ibtn(cmd, title, icon) {
  const b = el("button", { class: "ib", "data-cmd": cmd, title, "aria-label": title });
  b.innerHTML = ic(icon);
  b.addEventListener("mousedown", (e) => e.preventDefault());
  b.addEventListener("click", () => CMDS[cmd]());
  return b;
}
const vsep = () => el("span", { class: "vsep" });
const SIZE_LIST = [6, 7, 8, 9, 10, 10.5, 11, 12, 14, 16, 18, 20, 22, 24, 28, 32, 36, 40, 48, 56, 64, 72];
function sizeSelect() {
  const s = el("select", { class: "size-select", title: "文字サイズ", "aria-label": "文字サイズ" });
  for (const v of SIZE_LIST) s.append(el("option", { value: String(v), text: String(v) }));
  s.addEventListener("change", () => edit(() => S.ed.setSize(parseFloat(s.value)), true));
  return s;
}
function sizeControl() {
  const minus = el("button", { class: "mini", title: "文字を小さく (Ctrl+↓)", "aria-label": "文字を小さく" });
  const plus = el("button", { class: "mini", title: "文字を大きく (Ctrl+↑)", "aria-label": "文字を大きく" });
  minus.innerHTML = ic("minus"); plus.innerHTML = ic("plus");
  for (const [b, c] of [[minus, "sizeDown"], [plus, "sizeUp"]]) { b.addEventListener("mousedown", (e) => e.preventDefault()); b.addEventListener("click", () => CMDS[c]()); }
  return el("span", { class: "fill" }, sizeSelect(), el("span", { class: "unit", text: "pt" }), minus, plus);
}
function colorButton() {
  const b = el("label", { class: "ib colorbtn", title: "文字色" });
  b.innerHTML = ic("baseline") + '<span class="swatchline"></span>';
  const inp = el("input", { type: "color", value: "#ef4444", "aria-label": "文字色" });
  inp.addEventListener("change", () => edit(() => S.ed.setColor(inp.value), true));
  b.append(inp);
  return b;
}
function buildToolbar() {
  // top bar actions (same place as the other EZPZ editors)
  const save = el("span", { class: "split" });
  const main = el("button", { class: "main", title: "上書き保存 (Ctrl+S)" }); main.innerHTML = ic("save") + "<span>保存</span>";
  const more = el("button", { class: "more", title: "名前を付けて保存 (Ctrl+2)", "aria-label": "名前を付けて保存" }); more.innerHTML = ic("chevron-down");
  main.addEventListener("click", () => CMDS.save()); more.addEventListener("click", () => CMDS.saveAs());
  save.append(main, more);
  const open = el("button", { class: "tbtn hide-narrow", title: "開く (Ctrl+O)" }); open.innerHTML = ic("folder-open") + "<span>開く</span>";
  open.addEventListener("click", () => CMDS.open());
  $("#actions").replaceChildren(el("span", { style: "display:inline-flex;align-items:center" }, ibtn("undo", "元に戻す (Ctrl+Z)", "undo-2"), (() => { const b = ibtn("redo", "やり直し (Ctrl+Shift+Z)", "redo-2"); b.classList.add("hide-narrow"); return b; })(), vsep(), open, (() => { const b = ibtn("print", "印刷 (Ctrl+P)", "printer"); b.classList.add("hide-narrow"); return b; })(), save));

  $("#toolbar").replaceChildren(
    ibtn("new", "新規作成 (Ctrl+N)", "file-plus"), ibtn("open", "開く (Ctrl+O)", "folder-open"), vsep(),
    ibtn("cut", "切り取り (Ctrl+X)", "scissors"), ibtn("copy", "コピー (Ctrl+C)", "copy"), ibtn("paste", "貼り付け (Ctrl+V)", "clipboard-paste"), vsep(),
    ibtn("table", "表作成 (Ctrl+¥)", "table"), ibtn("rowBelow", "行を下に挿入", "between-horizontal-start"), ibtn("pageBreak", "改ページ (Ctrl+Y)", "separator-horizontal"), ibtn("date", "日付（和暦）", "calendar"), vsep(),
    ibtn("find", "検索", "search"), ibtn("marks", "編集記号", "pilcrow"), vsep(),
    ibtn("jumpPalette", "ジャンプパレット", "panel-left"), ibtn("toolPalette", "ツールパレット", "panel-right"));

  const font = el("span", { class: "fill", title: "フォント（いまは明朝のみ）" }, el("select", { disabled: true, "aria-label": "フォント" }, el("option", { text: "明朝" })));
  $("#formatbar").replaceChildren(
    font, sizeControl(), vsep(),
    ibtn("bold", "太字 (Ctrl+B)", "bold"), ibtn("italic", "斜体 (Ctrl+I)", "italic"), ibtn("underline", "下線 (Ctrl+U)", "underline"), colorButton(), vsep(),
    ibtn("alignLeft", "左寄せ (Ctrl+4)", "align-left"), ibtn("alignCenter", "センタリング (Ctrl+5)", "align-center"), ibtn("alignRight", "右寄せ (Ctrl+6)", "align-right"));
}

// ------------------------------------------------------------------ tool palette
function section(id, title, open, ...body) {
  const d = el("details", { class: "sec", id, open });
  const sum = el("summary", {}, el("span", { text: title }));
  sum.insertAdjacentHTML("beforeend", ic("chevron-right", "i chev"));
  d.append(sum, el("div", { class: "secbody" }, ...body));
  return d;
}

function pbtn(cmd, text, title, icon) {
  const b = el("button", { class: "pbtn", "data-cmd": cmd, title: title || text });
  b.innerHTML = (icon ? ic(icon) : "") + "<span></span>";
  b.lastChild.textContent = text;
  b.addEventListener("mousedown", (e) => e.preventDefault());
  b.addEventListener("click", () => CMDS[cmd]());
  return b;
}
function actBtn(text, fn, primary = false) {
  const b = el("button", { class: primary ? "pbtn primary" : "pbtn", text });
  b.addEventListener("click", fn);
  return b;
}

const RAIL = [["sec-char", "文字", "type"], ["sec-para", "段落", "align-left"], ["sec-rule", "罫線", "table-2"], ["sec-find", "検索", "search"], ["sec-count", "文字数", "hash"]];
function buildPalette() {
  const p = $("#palette");
  const swatches = el("div", { class: "row", style: "gap:4px" }, el("label", { text: "色", style: "margin-right:2px" }),
    ...["", "#0b1020", "#ef4444", "#0155ff", "#00b894", "#7c3aed", "#f59e0b"].map((c) => {
      const b = el("button", { class: "swatch", title: c || "自動", "aria-label": c || "自動", style: c ? `background:${c}` : "background:linear-gradient(135deg,#fff 44%,#ef4444 44%,#ef4444 56%,#fff 56%)" });
      b.addEventListener("mousedown", (e) => e.preventDefault());
      b.addEventListener("click", () => edit(() => S.ed.setColor(c), true));
      return b;
    }));
  const findIn = el("input", { class: "field", id: "find-q", placeholder: "検索する文字" });
  const replIn = el("input", { class: "field", id: "find-r", placeholder: "置換後の文字" });
  findIn.addEventListener("keydown", (e) => { if (e.key === "Enter") { e.preventDefault(); doFind(e.shiftKey); } if (e.key === "Escape") focusInput(); });
  replIn.addEventListener("keydown", (e) => { if (e.key === "Enter") { e.preventDefault(); doReplace(); } if (e.key === "Escape") focusInput(); });
  const rows = el("input", { class: "num", type: "number", min: 1, max: 100, value: 3, id: "pal-rows", "aria-label": "行数" });
  const cols = el("input", { class: "num", type: "number", min: 1, max: 20, value: 3, id: "pal-cols", "aria-label": "列数" });
  const hide = el("button", { class: "ib", title: "ツールパレットを隠す", "aria-label": "ツールパレットを隠す" });
  hide.innerHTML = ic("chevron-right");
  hide.addEventListener("click", () => toggleView("palette"));
  const sizeRow = el("div", { class: "row" }, el("label", { text: "サイズ" }), sizeControl());
  p.append(
    el("div", { class: "panelhead" }, el("span", { text: "ツールパレット" }), hide),
    section("sec-char", "文字", true, sizeRow,
      el("div", { class: "row" }, el("label", { text: "飾り" }), pbtn("bold", "太字", "太字 (Ctrl+B)"), pbtn("italic", "斜体", "斜体 (Ctrl+I)"), pbtn("underline", "下線", "下線 (Ctrl+U)")),
      swatches),
    section("sec-para", "段落", false,
      el("div", { class: "row" }, el("label", { text: "揃え" }), pbtn("alignLeft", "左", "左寄せ (Ctrl+4)", "align-left"), pbtn("alignCenter", "中央", "センタリング (Ctrl+5)", "align-center"), pbtn("alignRight", "右", "右寄せ (Ctrl+6)", "align-right")),
      el("div", { class: "row" }, el("label", { text: "ページ" }), pbtn("pageBreak", "改ページ", "改ページ (Ctrl+Y)", "separator-horizontal"))),
    section("sec-rule", "罫線", false,
      el("div", { class: "row" }, el("label", { text: "表作成" }), rows, el("span", { class: "lab", text: "行" }), cols, el("span", { class: "lab", text: "列" })),
      el("div", { class: "row" }, el("label", { text: "" }), actBtn("表を作成", () => edit(() => { if (!S.ed.insertTable(+rows.value, +cols.value)) toast("表は本文の段落にだけ作成できます"); }, true), true)),
      el("div", { class: "row" }, el("label", { text: "行" }), pbtn("rowAbove", "上に挿入", "行を上に挿入", "between-horizontal-end"), pbtn("rowBelow", "下に挿入", "行を下に挿入", "between-horizontal-start"), pbtn("rowDelete", "削除", "行を削除", "trash-2")),
      el("p", { class: "note", text: "表の中では Tab で次のセルへ移動します。" })),
    section("sec-find", "検索・置換", false,
      findIn, el("div", { class: "row" }, actBtn("前を検索", () => doFind(true)), actBtn("次を検索", () => doFind(false), true)),
      replIn, el("div", { class: "row" }, actBtn("置換", doReplace), actBtn("すべて置換", doReplaceAll))),
    section("sec-count", "文字数", false,
      el("div", { class: "kv" }, el("span", { text: "全体" }), el("b", { id: "cnt-all", text: "0" }), el("span", { text: "選択範囲" }), el("b", { id: "cnt-sel", text: "0" }), el("span", { text: "ページ" }), el("b", { id: "cnt-pages", text: "1" }))),
  );
  const rail = $("#rail");
  for (const [id, label, icon] of RAIL) {
    const b = el("button", { "data-sec": id, title: label });
    b.innerHTML = ic(icon) + "<span></span>"; b.lastChild.textContent = label;
    b.addEventListener("mousedown", (e) => e.preventDefault());
    b.addEventListener("click", () => { openSection(id, false); focusInput(); });
    rail.append(b);
  }
  for (const d of $$("details.sec", p)) d.addEventListener("toggle", syncRail);
  syncRail();
}
function syncRail() {
  // highlight the first open section, like the tool rail of the PDF editor
  const hidden = $("#main").classList.contains("no-palette");
  const cur = hidden ? null : $$("#palette details.sec").find((d) => d.open)?.id;
  for (const b of $$("#rail button")) b.classList.toggle("on", b.dataset.sec === cur);
}

function openSection(id, focus = true) {
  if ($("#main").classList.contains("no-palette")) toggleView("palette");
  for (const d of $$("#palette details.sec")) d.open = d.id === id;
  const d = $("#" + id); d.scrollIntoView({ block: "nearest" });
  syncRail();
  if (focus) $("select, input, button", d)?.focus();
}

function openFind(replace) {
  openSection("sec-find");
  const q = $("#find-q");
  const sel = S.ed.selectedText();
  if (sel && !sel.includes("\n")) q.value = sel;
  (replace ? $("#find-r") : q).focus();
  q.select();
}
function doFind(back) {
  const q = $("#find-q").value;
  if (!q) return;
  if (!S.ed.find(q, back)) toast(`「${q}」は見つかりません`);
  refresh();
}
function doReplace() {
  const q = $("#find-q").value; if (!q) return;
  S.ed.replace(q, $("#find-r").value); refresh();
}
function doReplaceAll() {
  const q = $("#find-q").value; if (!q) return;
  const n = S.ed.replaceAll(q, $("#find-r").value); refresh(); toast(`${n} 件置換しました`);
}
function switchJump(tab) {
  $$("#jump .seg button").forEach((b) => b.classList.toggle("on", b.dataset.tab === tab));
  $("#pagelist").hidden = tab !== "pages";
  $("#infopane").hidden = tab !== "info";
}

// ------------------------------------------------------------------ input
function focusInput() { const i = $("#ime"); if (document.activeElement !== i) i.focus({ preventScroll: true }); }

function keyCommand(e) {
  const mod = e.ctrlKey || e.metaKey;
  const k = e.key;
  // Ctrl-only shortcuts that differ from macOS Cmd conventions
  if (mod) {
    const code = e.code;
    const map = {
      z: e.shiftKey ? "redo" : "undo", r: "repeat", a: "selectAll", b: "bold", i: "italic", u: "underline",
      s: "save", o: "open", n: "new", p: "print", y: "pageBreak", j: "jump",
      "2": "saveAs", "4": "alignLeft", "5": "alignCenter", "6": "alignRight",
      f: S.keymap === "windows" ? "find" : "paraPalette", h: S.keymap === "windows" ? "replace" : null,
    };
    if (k === "^" || (code === "Equal" && k !== "=" && k !== "+")) return e.shiftKey ? "replace" : "find";
    if (k === "¥" || k === "\\" || code === "IntlYen" || code === "Backslash") return "table";
    if (e.ctrlKey && !e.metaKey && k === "ArrowUp") return "sizeUp";
    if (e.ctrlKey && !e.metaKey && k === "ArrowDown") return "sizeDown";
    const c = map[k.toLowerCase()];
    if (c) return c;
  }
  if (k === "F7") return "fontPalette";
  return null;
}

function onKeyDown(e) {
  if (menuKey(e)) return;
  if (S.composing || e.isComposing || e.keyCode === 229) return;
  const ed = S.ed;
  const mod = e.ctrlKey || e.metaKey;
  if (k_is(e, "Escape")) { e.preventDefault(); showMenu($$(".menu")[0], true); return; }
  if (e.altKey && !mod && /^[a-z]$/i.test(e.key)) {
    const m = $$(".menu").find((x) => $("button", x).dataset.key === e.key.toUpperCase());
    if (m) { e.preventDefault(); showMenu(m, true); return; }
  }
  const cmd = keyCommand(e);
  if (cmd) { e.preventDefault(); CMDS[cmd]?.(); return; }
  if (mod && ["c", "x", "v"].includes(e.key.toLowerCase())) {
    if (e.key.toLowerCase() !== "v") primeClipboard(); // let the native copy/cut happen on the textarea
    return;
  }
  const ext = e.shiftKey;
  const word = (IS_MAC && e.altKey) || (!IS_MAC && e.ctrlKey);
  const line = IS_MAC && e.metaKey;
  let handled = true;
  switch (e.key) {
    case "ArrowLeft": ed.moveCaret(line ? "home" : word ? "wordLeft" : "left", ext); break;
    case "ArrowRight": ed.moveCaret(line ? "end" : word ? "wordRight" : "right", ext); break;
    case "ArrowUp": ed.moveCaret(line ? "docStart" : "up", ext); break;
    case "ArrowDown": ed.moveCaret(line ? "docEnd" : "down", ext); break;
    case "Home": ed.moveCaret(e.ctrlKey ? "docStart" : "home", ext); break;
    case "End": ed.moveCaret(e.ctrlKey ? "docEnd" : "end", ext); break;
    case "PageUp": ed.moveCaret("pageUp", ext); break;
    case "PageDown": ed.moveCaret("pageDown", ext); break;
    case "Backspace": ed.backspace(); break;
    case "Delete": ed.deleteForward(); break;
    case "Enter": ed.enter(); break;
    case "Tab": if (!ed.nextCell(e.shiftKey)) ed.insertText("\t"); break;
    case "Insert": ed.setOverwrite(!S.status.overwrite); break;
    default: handled = false;
  }
  if (handled) { e.preventDefault(); refresh(); }
}
const k_is = (e, k) => e.key === k && !e.ctrlKey && !e.metaKey && !e.altKey;

function primeClipboard() {
  const ime = $("#ime");
  const t = S.ed.selectedText();
  if (!t) return;
  ime.value = t;
  ime.select();
}
function clip(kind) {
  const t = S.ed.selectedText();
  if (!t) return;
  navigator.clipboard?.writeText(t).then(() => toast(kind === "cut" ? "切り取りました" : "コピーしました"), () => toast(`${IS_MAC ? "⌘" : "Ctrl+"}${kind === "cut" ? "X" : "C"} を使ってください`));
  if (kind === "cut") edit(() => S.ed.cut(), true);
}

function bindInput() {
  const ime = $("#ime");
  ime.addEventListener("keydown", onKeyDown);
  ime.addEventListener("compositionstart", () => { S.composing = true; $("#caret").hidden = true; });
  ime.addEventListener("compositionupdate", (e) => { S.ed.setPreedit(e.data || ""); refresh({ scroll: true }); });
  ime.addEventListener("compositionend", (e) => {
    S.composing = false;
    S.ed.setPreedit("");
    if (e.data) S.ed.insertText(e.data);
    ime.value = "";
    setTimeout(() => { ime.value = ""; }, 0);
    refresh();
  });
  ime.addEventListener("input", (e) => {
    if (S.composing || e.isComposing || e.inputType === "insertCompositionText" || e.inputType === "insertFromComposition") return;
    if (e.inputType === "insertFromPaste") return; // handled in paste
    const v = ime.value;
    ime.value = "";
    if (v) { S.ed.insertText(v); S.lastCmd = null; refresh(); }
  });
  ime.addEventListener("paste", (e) => {
    e.preventDefault();
    const t = e.clipboardData?.getData("text/plain");
    if (t) { S.ed.insertText(t); refresh(); }
  });
  ime.addEventListener("copy", (e) => { const t = S.ed.selectedText(); if (t) { e.clipboardData.setData("text/plain", t); e.preventDefault(); } setTimeout(() => (ime.value = ""), 0); });
  ime.addEventListener("cut", (e) => { const t = S.ed.selectedText(); if (t) { e.clipboardData.setData("text/plain", t); e.preventDefault(); S.ed.cut(); refresh(); } setTimeout(() => (ime.value = ""), 0); });
  ime.addEventListener("blur", () => { if (!S.composing) $("#caret").style.opacity = ".35"; });
  ime.addEventListener("focus", () => { $("#caret").style.opacity = ""; });

  // mouse on pages
  const sc = $("#scroller");
  let dragging = false;
  const hit = (e) => {
    const page = e.target.closest?.(".page") || pageAtY(e.clientY);
    if (!page) return null;
    const r = page.getBoundingClientRect(), s = scale();
    return { page: +page.dataset.page, x: (e.clientX - r.left) / s, y: (e.clientY - r.top) / s };
  };
  sc.addEventListener("mousedown", (e) => {
    if (e.button !== 0) return;
    const h = hit(e); if (!h) return;
    e.preventDefault();
    closeMenus(false);
    if (e.detail === 2) S.ed.selectWord(h.page, h.x, h.y);
    else S.ed.click(h.page, h.x, h.y, e.shiftKey);
    dragging = e.detail === 1;
    refresh({ scroll: false });
    focusInput();
  });
  window.addEventListener("mousemove", (e) => {
    if (!dragging || !(e.buttons & 1)) { dragging = false; return; }
    const h = hit(e); if (!h) return;
    S.ed.click(h.page, h.x, h.y, true);
    refresh({ scroll: false });
  });
  window.addEventListener("mouseup", () => { dragging = false; });
  sc.addEventListener("scroll", () => drawRuler());
  sc.addEventListener("wheel", (e) => { if (e.ctrlKey) { e.preventDefault(); rezoom(S.zoom + (e.deltaY < 0 ? 0.1 : -0.1)); } }, { passive: false });
}
function pageAtY(y) {
  return S.pages.map((p) => p.div).find((d) => { const r = d.getBoundingClientRect(); return y >= r.top && y <= r.bottom; }) || null;
}

// ------------------------------------------------------------------ files
async function openFile(file) {
  try {
    const bytes = new Uint8Array(await file.arrayBuffer());
    const ed = JtdEditor.open(bytes);
    S.setup = null;
    setEditor(ed, file.name.replace(/\.[^.]+$/, ""));
    // 上書き保存 writes the same kind of file it came from
    S.saveFmt = ed.canSaveJtd() ? "jtd" : null;
    const s = JSON.parse(ed.summaryJson());
    if (s.template && /[\\:]/.test(s.template)) toast("この文書には元の保存場所が残っています（文書情報）");
    focusInput();
  } catch (err) {
    showInfo("開けませんでした", [["ファイル", file.name], ["理由", String(err.message || err)]]);
  }
}

function download(name, data, type) {
  const a = el("a", { href: URL.createObjectURL(new Blob([data], { type })), download: name });
  document.body.append(a); a.click(); a.remove();
  setTimeout(() => URL.revokeObjectURL(a.href), 2000);
}

function save(ask) {
  if (!ask && S.saveFmt && S.saveFmt !== "pdf") return writeFile(S.saveFmt, S.name);
  const d = $("#dlg-save");
  $("#save-name").value = S.name;
  const canJtd = S.ed.canSaveJtd();
  const jtd = $('input[name="fmt"][value="jtd"]', d);
  jtd.disabled = !canJtd;
  $("#fmt-jtd-note").textContent = canJtd ? "— 元の文書を書きかえて保存（罫線・書式をそのまま保持）" : "— 新規文書は準備中（一太郎文書を開いた場合に使えます）";
  const def = S.saveFmt || (canJtd ? "jtd" : "docx");
  $$('input[name="fmt"]', d).forEach((r) => (r.checked = r.value === def));
  d.returnValue = "";
  d.showModal();
  d.addEventListener("close", function h() {
    d.removeEventListener("close", h);
    if (d.returnValue !== "ok") return focusInput();
    const fmt = $('input[name="fmt"]:checked', d).value;
    const name = $("#save-name").value.trim() || "無題";
    S.name = name;
    if (fmt !== "pdf") S.saveFmt = fmt;
    writeFile(fmt, name);
  });
}
function writeFile(fmt, name) {
  const ed = S.ed;
  if (fmt === "jtd") {
    let bytes;
    try {
      bytes = ed.toJtd();
    } catch (err) {
      S.saveFmt = null;
      showInfo("一太郎形式で保存できませんでした", [
        ["理由", String(err.message || err)],
        ["文書", "何も書き出していません。編集内容はこの画面に残っています。"],
        ["ほかの方法", "Word (.docx) や PDF なら保存できます（名前を付けて保存）。"],
      ]);
      return;
    }
    download(name + ".jtd", bytes, "application/octet-stream");
    for (const w of JSON.parse(ed.saveWarnings())) toast(w);
  } else if (fmt === "pdf") {
    savePdf(name);
    return;
  } else if (fmt === "docx") download(name + ".docx", ed.toDocx(), "application/vnd.openxmlformats-officedocument.wordprocessingml.document");
  else if (fmt === "html") download(name + ".html", ed.toHtml(), "text/html");
  else if (fmt === "txt") download(name + ".txt", "﻿" + ed.toText(), "text/plain");
  else if (fmt === "md") download(name + ".md", ed.toMarkdown(), "text/markdown");
  ed.markSaved();
  refresh({ scroll: false });
  toast(`${name}.${fmt} を保存しました`);
  focusInput();
}

/** Render every page to JPEG and let the engine wrap them (plus an invisible
 *  text layer for search and copy) into a PDF. */
async function savePdf(name) {
  const n = S.ed.pageCount();
  const k = 2.2; // ~200 dpi
  const parts = [], lens = [], dims = [];
  toast("PDF を作成しています…");
  for (let i = 0; i < n; i++) {
    const data = JSON.parse(S.ed.pageJson(i));
    const c = el("canvas"); c.width = Math.round(data.w * k * PT); c.height = Math.round(data.h * k * PT);
    const g = c.getContext("2d"); g.fillStyle = "#fff"; g.fillRect(0, 0, c.width, c.height);
    g.setTransform(k * PT, 0, 0, k * PT, 0, 0);
    paintItems(g, data, false);
    const blob = await new Promise((res) => c.toBlob(res, "image/jpeg", 0.9));
    const buf = new Uint8Array(await blob.arrayBuffer());
    parts.push(buf); lens.push(buf.length); dims.push(c.width, c.height);
  }
  const all = new Uint8Array(lens.reduce((a, b) => a + b, 0));
  let o = 0; for (const p of parts) { all.set(p, o); o += p.length; }
  const pdf = S.ed.toPdf(all, Uint32Array.from(lens), Uint32Array.from(dims), name);
  download(name + ".pdf", pdf, "application/pdf");
  toast(`${name}.pdf を保存しました`);
  focusInput();
}

function printDoc() {
  const area = $("#printarea");
  area.replaceChildren();
  const n = S.ed.pageCount();
  const k = 2.2; // render at ~200 dpi
  for (let i = 0; i < n; i++) {
    const data = JSON.parse(S.ed.pageJson(i));
    const c = el("canvas"); c.width = Math.round(data.w * k * PT); c.height = Math.round(data.h * k * PT);
    const g = c.getContext("2d"); g.fillStyle = "#fff"; g.fillRect(0, 0, c.width, c.height);
    g.setTransform(k * PT, 0, 0, k * PT, 0, 0);
    paintItems(g, data, false);
    area.append(el("img", { src: c.toDataURL("image/png"), alt: `${i + 1}ページ` }));
  }
  setTimeout(() => { window.print(); setTimeout(() => area.replaceChildren(), 1000); }, 50);
}

function guardUnsaved(next) {
  if (!S.status?.modified) return next();
  const d = $("#dlg-confirm");
  $("#cf-title").textContent = "変更を保存しますか？";
  $("#cf-text").textContent = `「${S.name}」は変更されています。保存しないと変更は失われます。`;
  $("#cf-ok").textContent = "保存する";
  $("#cf-discard").hidden = false;
  d.returnValue = "";
  d.showModal();
  d.addEventListener("close", function h() {
    d.removeEventListener("close", h);
    if (d.returnValue === "ok") { save(true); }
    else if (d.returnValue === "discard") next();
    else focusInput();
  });
}

function openTableDialog() {
  const d = $("#dlg-table");
  d.returnValue = "";
  d.showModal();
  d.addEventListener("close", function h() {
    d.removeEventListener("close", h);
    if (d.returnValue === "ok") edit(() => { if (!S.ed.insertTable(+$("#tbl-rows").value, +$("#tbl-cols").value)) toast("表は本文の段落にだけ作成できます"); }, true);
    else focusInput();
  });
}

function showInfo(title, rows) {
  const d = $("#dlg-info");
  $("#info-title").textContent = title;
  const t = el("table");
  for (const [k, v] of rows) t.append(el("tr", {}, el("td", { text: k }), el("td", { text: String(v) })));
  $("#info-body").replaceChildren(t);
  d.showModal();
  d.addEventListener("close", function h() { d.removeEventListener("close", h); focusInput(); });
}
function showShortcuts() {
  const k = (s) => SC(s);
  const findKey = S.keymap === "windows" ? "Ctrl+F" : "Ctrl+^";
  showInfo("ショートカットキー", [
    [k("Ctrl+5") + " / " + k("Ctrl+6"), "センタリング / 右寄せ（" + k("Ctrl+4") + " 左寄せ）"],
    [k("Ctrl+↑") + " / " + k("Ctrl+↓"), "文字を大きく / 小さく"],
    [k("Ctrl+B") + " " + k("Ctrl+I") + " " + k("Ctrl+U"), "太字 / 斜体 / 下線"],
    [k("Ctrl+Y"), "改ページ"], [k("Ctrl+¥"), "表作成"], ["F7", "フォント・飾り"],
    [k(findKey) + " / " + k("Ctrl+^"), "検索"], [k("Ctrl+2"), "名前を付けて保存"],
    [k("Ctrl+Z") + " / " + k("Ctrl+R"), "元に戻す / 繰り返し"], [k("Ctrl+J"), "ジャンプ"],
    ["Esc", "メニュー"], ["Insert", "挿入 / 上書 切り替え"], ["Tab", "表の次のセルへ"],
  ]);
}

let toastTimer = 0;
function toast(msg) {
  const t = $("#toast"); t.textContent = msg; t.classList.add("show");
  clearTimeout(toastTimer); toastTimer = setTimeout(() => t.classList.remove("show"), 2600);
}

// ------------------------------------------------------------------ chrome
function bindChrome() {
  $("#fileinput").addEventListener("change", (e) => { const f = e.target.files[0]; e.target.value = ""; if (f) openFile(f); });
  const dz = $("#dropzone");
  let depth = 0;
  window.addEventListener("dragenter", (e) => { if (e.dataTransfer?.types?.includes("Files")) { depth++; dz.classList.add("show"); e.preventDefault(); } });
  window.addEventListener("dragleave", () => { depth = Math.max(0, depth - 1); if (!depth) dz.classList.remove("show"); });
  window.addEventListener("dragover", (e) => e.preventDefault());
  window.addEventListener("drop", (e) => { e.preventDefault(); depth = 0; dz.classList.remove("show"); const f = e.dataTransfer.files[0]; if (f) guardUnsaved(() => openFile(f)); });
  window.addEventListener("beforeunload", (e) => { if (S.status?.modified) { e.preventDefault(); e.returnValue = ""; } });
  window.addEventListener("resize", () => drawRuler());
  document.addEventListener("mousedown", (e) => { if (openMenu && !e.target.closest(".menu")) closeMenus(false); });
  document.addEventListener("keydown", (e) => {
    if (e.target === $("#ime") || e.target.closest("dialog")) return;
    if (openMenu) { menuKey(e); return; }
    const cmd = keyCommand(e);
    if (cmd && !["selectAll", "undo", "redo", "bold", "italic", "underline"].includes(cmd)) { e.preventDefault(); CMDS[cmd](); }
  });
  $("#st-mode").addEventListener("click", () => { S.ed.setOverwrite(!S.status.overwrite); refresh({ scroll: false }); focusInput(); });
  for (const [id, icon, cmd] of [["#st-marks", "pilcrow", "marks"], ["#st-fit", "move-horizontal", "zoomFit"], ["#st-zin", "plus", "zoomIn"], ["#st-zout", "minus", "zoomOut"]]) {
    const b = $(id); b.innerHTML = ic(icon);
    b.addEventListener("mousedown", (e) => e.preventDefault());
    b.addEventListener("click", () => CMDS[cmd]());
  }
  $$("#jump .seg button").forEach((b) => b.addEventListener("click", () => switchJump(b.dataset.tab)));
  $("#st-note").textContent = "用紙: A4 40字×36行";
}

boot().catch((e) => { document.body.textContent = "起動できませんでした: " + (e.message || e); });
