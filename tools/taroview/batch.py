#!/usr/bin/env python3
"""Open every .jtd of a folder in Ichitaro Viewer and keep its copied text.

usage: batch.py CORPUS_DIR CACHE_DIR
Writes CACHE_DIR/<name>.txt (viewer text, UTF-8), <name>.png and status.tsv.
Files already in the cache are skipped.
"""
import os, subprocess, sys

here = os.path.dirname(os.path.abspath(__file__))
src, cache = sys.argv[1], sys.argv[2]
os.makedirs(cache, exist_ok=True)
names = sorted(f for f in os.listdir(src) if f.endswith(".jtd"))
with open(os.path.join(cache, "status.tsv"), "a") as log:
    for n in names:
        base = os.path.join(cache, n[:-4])
        if os.path.exists(base + ".txt"):
            continue
        try:
            r = subprocess.run([sys.executable, os.path.join(here, "view.py"), os.path.join(src, n),
                                base + ".png", "--text", base + ".txt"], capture_output=True, text=True, timeout=120)
            out = r.stdout.strip()
        except subprocess.TimeoutExpired as e:
            out = (e.stdout.decode() if isinstance(e.stdout, bytes) else (e.stdout or "")).strip() or "TIMEOUT\tharness"
        line = f"{n}\t{out}"
        print(line, flush=True)
        log.write(line + "\n")
        log.flush()
