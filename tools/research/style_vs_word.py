#!/usr/bin/env python3
"""Which Word attribute does each JTD style property id predict?

    python3 tools/research/style_vs_word.py            # all ids
    python3 tools/research/style_vs_word.py 2 sz       # id 2 against font size

For each property id, prints how much better we predict each Word attribute
when we know the id's value (the "gain"), then the value → attribute table.
"""
import collections, sys
from common import pairs, jtd_model, jtd_chars, docx_chars, align

KEYS = ("sz", "b", "i", "u", "color", "w", "spacing", "position", "font", "strike", "vertAlign", "jc")


def main():
    co = collections.defaultdict(lambda: collections.defaultdict(collections.Counter))
    n = 0
    for jtd, docx in pairs():
        m = jtd_model(jtd)
        if not m:
            continue
        for (ch, raw, _), (_, pr) in align(jtd_chars(m), docx_chars(docx)):
            n += 1
            for pid in range(1, 32):
                v = raw.get(str(pid), "-")
                for k in KEYS:
                    co[pid][v][(k, pr.get(k, "-"))] += 1
    print(f"aligned characters: {n}")
    only = [int(sys.argv[1])] if len(sys.argv) > 1 else range(1, 32)
    for pid in only:
        d = co.get(pid)
        if not d or set(d) == {"-"}:
            continue
        gains = []
        for k in (sys.argv[2:3] or KEYS):
            pur = base = 0
            allc = collections.Counter()
            for v, c in d.items():
                cs = collections.Counter({kv[1]: x for kv, x in c.items() if kv[0] == k})
                pur += cs.most_common(1)[0][1] if cs else 0
                allc.update(cs)
            base = allc.most_common(1)[0][1] if allc else 0
            gains.append(((pur - base) / max(n, 1), k))
        gains.sort(reverse=True)
        g, k = gains[0]
        print(f"\nproperty {pid}: best predicts '{k}' (gain {g:.3f})")
        for v, c in sorted(d.items(), key=lambda kv: -sum(x for kk, x in kv[1].items() if kk[0] == k))[:10]:
            cs = collections.Counter({kv[1]: x for kv, x in c.items() if kv[0] == k})
            vs = hex(v) if isinstance(v, int) else v
            print(f"   {vs:>12}  {cs.most_common(4)}")


if __name__ == "__main__":
    main()
