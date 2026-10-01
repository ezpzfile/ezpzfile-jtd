#!/usr/bin/env python3
"""Text recall against the Word twins: how much of Word's visible text do we output?"""
import difflib, subprocess
from common import pairs, is_space, docx_chars, EZPZJTD

tot = hit = 0
rows = []
for jtd, docx in pairs():
    ours = subprocess.run([EZPZJTD, "text", jtd], capture_output=True, text=True).stdout
    a = "".join(c for c in ours if not is_space(c))
    b = "".join(c for c, _ in docx_chars(docx) if not is_space(c))
    sm = difflib.SequenceMatcher(None, a, b, autojunk=False)
    h = sum(m.size for m in sm.get_matching_blocks())
    tot += len(b)
    hit += h
    rows.append((h / max(1, len(b)), jtd.rsplit("/", 1)[-1], len(b)))
rows.sort()
print(f"files {len(rows)}  recall {hit / max(1, tot):.4f}  ({hit}/{tot} characters, order-sensitive)")
for r in rows[:10]:
    print(f"  {r[0]:.3f}  {r[1]}  ({r[2]} chars)")
