L = "あいうえおかきくけこさしすせそたちつてと"
T = "\n".join(f'Insert("{L}", 1)' for _ in range(5)) + "\nCursorUp(5)\nStartOfLine()\n"

def box(down, right, kind=1, extra_before_end="", extra_after_end="", mode="KeisenMode(1, 2)"):
    return T + f"""{mode}
KeisenStart()
CursorDown({down})
CursorRight({right})
{extra_before_end}
KeisenEnd()
{extra_after_end}
Keisen(1, {kind})
CancelMode()
"""

def hline(line, right, kind=1, mode="KeisenMode(1, 2)"):
    return T + (f"CursorDown({line})\n" if line else "") + f"""{mode}
KeisenStart()
CursorRight({right})
KeisenEnd()
Keisen(1, {kind})
CancelMode()
"""

SAMPLES = [
    ("r00-text", T),
    ("r01-box", box(2, 10)),
    ("r02-box-kind2", box(2, 10, 2)),
    ("r03-box-cols", box(2, 10, 1, "KeisenColumnDivide(1)", "KeisenColumnDivideInterval(5)")),
    ("r04-box-rows", box(2, 10, 1, "KeisenLineDivide(1)", "KeisenLineDivideInterval(1)")),
    ("r05-hline-l2", hline(1, 10)),
    ("r06-hline-l3", hline(2, 10)),
    ("r07-hline-l2-short", hline(1, 6)),
    ("r08-hline-l2-normal", hline(1, 10, 1, "KeisenMode(1, 1)")),
    ("r09-box-wide", box(2, 16)),
    ("r10-box-4rows", box(3, 10)),
]
