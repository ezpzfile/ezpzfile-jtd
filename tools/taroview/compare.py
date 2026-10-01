#!/usr/bin/env python3
"""Compare our text with the text Ichitaro Viewer shows (from batch.py).

usage: compare.py CORPUS_DIR CACHE_DIR [EZJTD_BIN]
Both texts are reduced to their visible characters (no spaces, no box
drawing) and aligned. Prints per-file similarity and the worst differences.
"""
import difflib, os, subprocess, sys, unicodedata

src, cache = sys.argv[1], sys.argv[2]
exe = sys.argv[3] if len(sys.argv) > 3 else os.path.join(os.path.dirname(__file__), "../../engine/target/release/ezjtd")


def norm(s):
    out = []
    for c in s:
        if c.isspace() or c == "　" or 0x2500 <= ord(c) <= 0x257F:
            continue
        out.append(unicodedata.normalize("NFKC", c))
    return "".join(out)


rows = []
for f in sorted(os.listdir(cache)):
    if not f.endswith(".txt"):
        continue
    name = f[:-4]
    viewer = open(os.path.join(cache, f), encoding="utf-8", errors="replace").read()
    if not viewer.strip():
        continue
    r = subprocess.run([exe, "text", os.path.join(src, name + ".jtd")], capture_output=True, text=True)
    if r.returncode != 0:
        rows.append((name, 0.0, len(norm(viewer)), "our reader failed: " + r.stderr.strip()[:80]))
        continue
    a, b = norm(viewer), norm(r.stdout)
    sm = difflib.SequenceMatcher(None, a, b, autojunk=False)
    ratio = sm.ratio()
    diffs = [(op, a[i1:i2][:30], b[j1:j2][:30]) for op, i1, i2, j1, j2 in sm.get_opcodes() if op != "equal"]
    rows.append((name, ratio, len(a), diffs[:3]))

rows.sort(key=lambda r: r[1])
tot = sum(r[2] for r in rows)
w = sum(r[1] * r[2] for r in rows) / max(tot, 1)
print(f"files {len(rows)}  weighted similarity {w:.4f}  exact {sum(1 for r in rows if r[1] == 1.0)}")
for name, ratio, n, d in rows[:15]:
    print(f"{ratio:.3f} {n:6d} {name}  {d}")
