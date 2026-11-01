// Runs ezpzjtd in a Web Worker so a hostile or huge .jtd cannot freeze the page.
// Used by ezpzjtd-safe.js; load it as a module worker next to ../pkg/.
//
// in:  { id, bytes: Uint8Array, limits: {maxPages, …}, want: "html" | "text" | "markdown" | "json" | "pages" }
// out: { id, ok: true, result, warnings } or { id, ok: false, error }
import init, { JtdDocument, JtdEditor } from "../pkg/ezpzjtd_wasm.js";

const ready = init();

self.onmessage = async (e) => {
  const { id, bytes, limits = {}, want = "html" } = e.data || {};
  try {
    await ready;
    const lim = JSON.stringify(limits);
    if (want === "pages") {
      const ed = JtdEditor.openWith(bytes, lim);
      const n = ed.pageCount();
      const pages = [];
      for (let i = 0; i < n; i++) pages.push(JSON.parse(ed.pageJson(i)));
      const status = JSON.parse(ed.statusJson());
      self.postMessage({ id, ok: true, result: { pages, truncated: status.truncated }, warnings: [] });
      ed.free();
      return;
    }
    const doc = JtdDocument.withLimits(bytes, lim);
    const result =
      want === "text" ? doc.text() : want === "markdown" ? doc.markdown() : want === "json" ? doc.json() : doc.html();
    const warnings = JSON.parse(doc.warningsJson());
    doc.free();
    self.postMessage({ id, ok: true, result, warnings });
  } catch (err) {
    self.postMessage({ id, ok: false, error: String((err && err.message) || err) });
  }
};
