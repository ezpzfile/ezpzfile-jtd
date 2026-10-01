"""Shared helpers for the research scripts.

These scripts compare an Ichitaro file with the Word file the publisher
exported from the same source. Word's formatting is known, so lining the two
up character by character tells us what each unknown JTD field means.
"""
import difflib, json, os, subprocess, unicodedata, zipfile
from xml.etree import ElementTree as ET

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
EZPZJTD = os.environ.get("EZPZJTD", os.path.join(ROOT, "engine", "target", "release", "ezpzjtd"))
CORPUS = os.environ.get("EZPZJTD_CORPUS", os.path.join(ROOT, "corpus", "local"))
W = "{http://schemas.openxmlformats.org/wordprocessingml/2006/main}"


def is_space(c):
    return c.isspace() or unicodedata.category(c) in ("Zs", "Cc", "Cf")


def pairs():
    """(jtd_path, docx_path) for every file that has a converted Word twin."""
    for f in sorted(os.listdir(CORPUS)):
        if f.endswith(".jtd"):
            d = os.path.join(CORPUS, f[:-4] + ".docx")
            if os.path.exists(d):
                yield os.path.join(CORPUS, f), d


def jtd_model(path):
    r = subprocess.run([EZPZJTD, "json", path], capture_output=True, text=True)
    return json.loads(r.stdout) if r.returncode == 0 else None


def jtd_chars(model):
    """Visible characters with the raw style properties and paragraph align."""
    out = []

    def para(p):
        for run in p["runs"]:
            raw = (run.get("style") or {}).get("raw", {})
            for ch in run["text"]:
                out.append((ch, raw, p["align"]))

    for sheet in model["sheets"]:
        for b in sheet["blocks"]:
            if b["type"] == "paragraph":
                para(b)
            elif b["type"] == "table":
                for row in b["rows"]:
                    for cell in row["cells"]:
                        for p in cell["paragraphs"]:
                            para(p)
    return out


def docx_chars(path):
    """Characters of a .docx with their direct run/paragraph formatting."""
    z = zipfile.ZipFile(path)
    x = ET.fromstring(z.read("word/document.xml"))
    out = []
    for p in x.iter(W + "p"):
        ppr = p.find(W + "pPr")
        jc = ppr.find(W + "jc").get(W + "val") if ppr is not None and ppr.find(W + "jc") is not None else None
        for r in p.iter(W + "r"):
            rp = r.find(W + "rPr")
            pr = {"jc": jc}
            if rp is not None:
                for tag in ("sz", "b", "i", "u", "color", "w", "spacing", "position", "strike", "vertAlign"):
                    e = rp.find(W + tag)
                    if e is not None:
                        pr[tag] = e.get(W + "val", "1")
                e = rp.find(W + "rFonts")
                if e is not None:
                    pr["font"] = e.get(W + "eastAsia") or e.get(W + "ascii")
            for t in r.iter(W + "t"):
                for ch in t.text or "":
                    out.append((ch, pr))
    return out


def align(a, b):
    """Match two character lists (ignoring spaces). Yields (a_item, b_item)."""
    a = [x for x in a if not is_space(x[0])]
    b = [x for x in b if not is_space(x[0])]
    sm = difflib.SequenceMatcher(None, "".join(x[0] for x in a), "".join(x[0] for x in b), autojunk=False)
    for blk in sm.get_matching_blocks():
        for k in range(blk.size):
            yield a[blk.a + k], b[blk.b + k]
