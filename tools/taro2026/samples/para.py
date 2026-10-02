# Paragraph format: indents (インデント) and line spacing (改行幅).
# Three paragraphs of two lines each; the setting goes on the middle one.
L = "あいうえおかきくけこさしすせそたちつてと" * 3
T = "\n".join(f'Insert("{L}", 1)' for _ in range(3)) + "\nCursorUp(2)\nStartOfLine()\n"
PARA = "RangeMode(3)\nRangeStart()\nRangeEnd(1, 1)\n"


def fmt(call):
    return T + PARA + call + "\nCancelMode()\n"


SAMPLES = [
    ("q00-base", T),
    ("q01-left2", fmt("Indent(1, 2, 0, 0, 0, 101)")),
    ("q02-left5", fmt("Indent(1, 5, 0, 0, 0, 101)")),
    ("q03-right3", fmt("Indent(1, 0, 3, 0, 0, 101)")),
    ("q04-first1", fmt("Indent(1, 0, 0, 1, 0, 101)")),
    ("q05-hang", fmt("Indent(1, 4, 0, -2, 0, 101)")),
    ("q06-left10mm", fmt("Indent(1, 10, 0, 0, 0, 1)")),
    ("q07-firstend2", fmt("Indent(1, 0, 0, 0, 2, 101)")),
    ("q08-sp-half", fmt("LineSpacing(3)")),
    ("q09-sp-34", fmt("LineSpacing(1)")),
    ("q10-sp-0", fmt("LineSpacing(6)")),
    ("q11-sp-5mm", fmt("LineSpacing(8, 5)")),
    ("q12-sp-150pct", fmt("LineSpacing(9, 150)")),
    ("q13-sp-5mm-total", fmt("LineSpacing(8, 12, 2)")),
]
SAMPLES += [
    ("q14-sp-23", fmt("LineSpacing(2)")),
    ("q15-sp-13", fmt("LineSpacing(4)")),
    ("q16-sp-14", fmt("LineSpacing(5)")),
    ("q17-sp-ruby", fmt("LineSpacing(7)")),
    ("q18-sp-40pct", fmt("LineSpacing(9, 40)")),
]
