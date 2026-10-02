#!/usr/bin/env bash
# Build the browser viewer and editor.
#   web/pkg/                         ES module + .wasm (for embedding in other sites)
#   web/dist/ezpzjtd-viewer.html     read-only viewer, one self-contained file (double-click, offline)
#   web/dist/ezpzjtd-editor.html     the editor in Japanese, one self-contained file (double-click, offline)
#   web/dist/ezpzjtd-editor.en.html  the same in English
#   web/dist/ezpzjtd-editor.embed.html
#                                    the editor for a site that serves pkg/ itself: fill in
#                                    /*__EZPZJTD_WASM_URL__*/ with the .wasm URL and set <html lang>
# Needs: rustup target add wasm32-unknown-unknown ; cargo install wasm-bindgen-cli --version 0.2.129
set -euo pipefail
cd "$(dirname "$0")/../engine"
cargo build --release --target wasm32-unknown-unknown -p ezpzjtd-wasm
wasm-bindgen --target web --no-typescript --out-dir ../web/pkg target/wasm32-unknown-unknown/release/ezpzjtd_wasm.wasm
if command -v wasm-opt >/dev/null; then wasm-opt -Oz ../web/pkg/ezpzjtd_wasm_bg.wasm -o ../web/pkg/ezpzjtd_wasm_bg.wasm; fi
cd ../web
mkdir -p dist
python3 - <<'PY'
import base64, re
glue = open("pkg/ezpzjtd_wasm.js", encoding="utf-8").read()
glue = re.sub(r"^export \{[^}]*\};?\s*$", "", glue, flags=re.M)   # drop the export list
glue = re.sub(r"^export (class|function|async function|const|let) ", r"\1 ", glue, flags=re.M)
wasm = base64.b64encode(open("pkg/ezpzjtd_wasm_bg.wasm", "rb").read()).decode()
html = open("viewer.html", encoding="utf-8").read()
html = html.replace("/*__EZPZJTD_GLUE__*/", glue).replace("/*__EZPZJTD_WASM__*/", wasm)
open("dist/ezpzjtd-viewer.html", "w", encoding="utf-8").write(html)
print("dist/ezpzjtd-viewer.html", len(html) // 1024, "KB")
# the editor script: words (i18n.js), the optional link to a framing page (host.js), the app
app = "\n".join(open(f, encoding="utf-8").read() for f in ("editor/i18n.js", "editor/host.js", "editor/app.js"))
ed = open("editor.html", encoding="utf-8").read()
ed = ed.replace("/*__EZPZJTD_GLUE__*/", glue).replace("/*__EZPZJTD_APP__*/", app)
for lang, name in (("ja", "ezpzjtd-editor.html"), ("en", "ezpzjtd-editor.en.html")):
    page = ed.replace("/*__EZPZJTD_WASM__*/", wasm).replace('<html lang="ja">', f'<html lang="{lang}">')
    open(f"dist/{name}", "w", encoding="utf-8").write(page)
    print(f"dist/{name}", len(page) // 1024, "KB")
open("dist/ezpzjtd-editor.embed.html", "w", encoding="utf-8").write(ed)
print("dist/ezpzjtd-editor.embed.html", len(ed) // 1024, "KB")
PY
