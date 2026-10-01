#!/usr/bin/env python3
"""Download the public research corpus listed in corpus/manifest.tsv.

Files go to corpus/local/ (git-ignored). Each Ichitaro file is stored next to
its Word/PDF twin so tools/research/*.py can compare them.

    python3 tools/fetch_corpus.py            # download missing files
    python3 tools/fetch_corpus.py --docx     # also convert .doc twins to .docx (needs LibreOffice)
"""
import os, subprocess, sys, time, urllib.request

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "corpus", "local")


def fetch(url, dest):
    if os.path.exists(dest) and os.path.getsize(dest) > 0:
        return False
    req = urllib.request.Request(url, headers={"User-Agent": "ezpzfile-jtd-research/0.1"})
    with urllib.request.urlopen(req, timeout=30) as r, open(dest, "wb") as f:
        f.write(r.read())
    time.sleep(0.3)  # be polite to government servers
    return True


def main():
    os.makedirs(OUT, exist_ok=True)
    n = 0
    for line in open(os.path.join(ROOT, "corpus", "manifest.tsv"), encoding="utf-8"):
        if line.startswith("#") or not line.strip():
            continue
        parts = line.rstrip("\n").split("\t")
        for url in parts:
            if not url:
                continue
            try:
                if fetch(url, os.path.join(OUT, os.path.basename(url))):
                    n += 1
            except Exception as e:  # keep going; some links rot
                print(f"skip {url}: {e}", file=sys.stderr)
    print(f"downloaded {n} new files into {OUT}")
    if "--docx" in sys.argv:
        docs = [os.path.join(OUT, f) for f in os.listdir(OUT) if f.endswith(".doc")]
        if docs:
            subprocess.run(["soffice", "--headless", "--convert-to", "docx", "--outdir", OUT, *docs],
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            print(f"converted {len(docs)} .doc files to .docx")


if __name__ == "__main__":
    main()
