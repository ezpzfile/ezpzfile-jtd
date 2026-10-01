"""Minimal SsmgV.01 reader for research scripts (mirrors engine ssmg.rs)."""
import struct


def substreams(b):
    assert b[:8] == b"SsmgV.01"
    sub_count, bs, nblocks = struct.unpack(">III", b[8:20])
    blocks_end = 20 + bs * nblocks
    o = blocks_end
    subs = {}
    for _ in range(sub_count):
        idx, _z, blen, n, _n2 = struct.unpack(">IIIII", b[o:o + 20])
        o += 20
        ids = struct.unpack(">%dI" % n, b[o:o + 4 * n])
        o += 4 * n
        data = b"".join(b[20 + bs * i: 20 + bs * (i + 1)] for i in ids)[:blen]
        subs[idx] = data
    return [subs[k] for k in sorted(subs)]


def pieces(b):
    """[(units bytes, style bytes)]"""
    subs = substreams(b)
    h = subs[0]
    if h[:8] == b"TextV.01":
        n = struct.unpack(">I", h[8:12])[0]
        return [(h[12:12 + 2 * n], h[12 + 2 * n:])]
    if h[:8] == b"QLSTV.01":
        k = struct.unpack(">I", h[8:12])[0]
        out = []
        for j in range(k):
            n, si = struct.unpack(">II", h[12 + 8 * j:20 + 8 * j])
            s = subs[si]
            out.append((s[:2 * n], s[2 * n:]))
        return out
    raise ValueError(h[:8])
