#!/usr/bin/env bash
# Build the browser viewer.
#   web/pkg/                 ES module + .wasm (for embedding in other sites)
#   web/dist/ezjtd-viewer.html  read-only viewer, one self-contained file (double-click, offline)
#   web/dist/ezjtd-editor.html  the editor, one self-contained file (double-click, offline)
# Needs: rustup target add wasm32-unknown-unknown ; cargo install wasm-bindgen-cli --version 0.2.129
set -euo pipefail
cd "$(dirname "$0")/../engine"
cargo build --release --target wasm32-unknown-unknown -p ezjtd-wasm
wasm-bindgen --target web --no-typescript --out-dir ../web/pkg target/wasm32-unknown-unknown/release/ezjtd_wasm.wasm
if command -v wasm-opt >/dev/null; then wasm-opt -Oz ../web/pkg/ezjtd_wasm_bg.wasm -o ../web/pkg/ezjtd_wasm_bg.wasm; fi
cd ../web
mkdir -p dist
python3 - <<'PY'
import base64, re
glue = open("pkg/ezjtd_wasm.js", encoding="utf-8").read()
glue = re.sub(r"^export \{[^}]*\};?\s*$", "", glue, flags=re.M)   # drop the export list
glue = re.sub(r"^export (class|function|async function|const|let) ", r"\1 ", glue, flags=re.M)
wasm = base64.b64encode(open("pkg/ezjtd_wasm_bg.wasm", "rb").read()).decode()
html = open("viewer.html", encoding="utf-8").read()
html = html.replace("/*__EZJTD_GLUE__*/", glue).replace("/*__EZJTD_WASM__*/", wasm)
open("dist/ezjtd-viewer.html", "w", encoding="utf-8").write(html)
print("dist/ezjtd-viewer.html", len(html) // 1024, "KB")
app = open("editor/app.js", encoding="utf-8").read()
ed = open("editor.html", encoding="utf-8").read()
ed = ed.replace("/*__EZJTD_GLUE__*/", glue).replace("/*__EZJTD_WASM__*/", wasm).replace("/*__EZJTD_APP__*/", app)
open("dist/ezjtd-editor.html", "w", encoding="utf-8").write(ed)
print("dist/ezjtd-editor.html", len(ed) // 1024, "KB")
PY
