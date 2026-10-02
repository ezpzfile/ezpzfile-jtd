L = "あいうえおかきくけこさしすせそたちつてと"
T = "\n".join(f'Insert("{L}", 1)' for _ in range(5)) + "\nCursorUp(5)\nStartOfLine()\n"

def box(kind, mode="KeisenMode(2, 2)", right=10):
    return T + f"""{mode}
KeisenStart()
CursorDown(2)
CursorRight({right})
KeisenLineDivide({kind})
KeisenEnd()
KeisenLineDivideInterval(1)
Keisen(1, {kind})
CancelMode()
"""

SAMPLES = [(f"t{k:02d}-half-gap", box(k)) for k in range(1, 17)] + [
    ("u01-half-normal", box(1, "KeisenMode(2, 1)")),
    ("u02-full-normal", box(1, "KeisenMode(1, 1)")),
    ("u03-half-gap-at4", T + """CursorRight(4)
KeisenMode(2, 2)
KeisenStart()
CursorDown(2)
CursorRight(10)
KeisenEnd()
Keisen(1, 1)
CancelMode()
"""),
]
