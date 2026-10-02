# A short notice for the README screenshot: our own text, made in 一太郎
# with a centred title, a right-aligned date and a ruled table.
ROWS = [
    ("日時", "２０２６年１０月１５日（木）１４：００～１６：００"),
    ("会場", "本社３階　第２会議室"),
    ("内容", "一太郎・ＤｏｃｕＷｏｒｋｓ文書の開き方と直し方"),
    ("持ち物", "ノートパソコン（ブラウザが使えるもの）"),
]
# 一太郎's ruled lines take a character cell, so every row is full-width
# text padded to the same length, with spaces where the lines go. A box
# starts below the line the cursor is on, so the table starts from a line
# of spaces above it. In ruled-line mode the cursor moves in half-width
# columns.
LABEL = 4
WIDTH = 2 + LABEL + 2 + max(len(b) for _, b in ROWS) + 2


def ins(s):
    return f'Insert("{s}", 1)\n'


SEL = "StartOfLine()\nRangeMode(3)\nRangeStart()\nRangeEnd(1, 1)\n"


def fmt_prev(call):
    # format the line just typed (a range is needed), then go on below it
    return "CursorUp(1)\n" + SEL + call + "\nCancelMode()\nCursorDown(1)\nStartOfLine()\n"


def cell(a, b):
    row = "　" + a.ljust(LABEL, "　") + "　　" + b
    return ins(row.ljust(WIDTH, "　"))


TITLE = ins("社内研修のご案内") + fmt_prev("FormatCenter()")
BODY = (
    ins("2026年10月2日") + fmt_prev("FormatRight()")
    + ins("各位")
    + ins("")
    + ins("下記のとおり、ファイル形式ツールの社内研修を行います。一太郎の文書とDocuWorksの文書を、"
          "ブラウザだけで開いて直す方法を実際に試します。どなたでもご参加ください。")
    + ins("")
    + ins("記") + fmt_prev("FormatCenter()")
    + ins("　" * WIDTH)
)
N = len(ROWS)
TABLE = "".join(cell(a, b) for a, b in ROWS) + f"""CursorUp({N + 1})
StartOfLine()
KeisenMode(1, 2)
KeisenStart()
CursorDown({N})
StartOfLine()
CursorRight({2 * (WIDTH - 1)})
KeisenLineDivide(1)
KeisenLineDivideInterval(1)
KeisenEnd()
Keisen(1, 1)
CancelMode()
CursorUp({N})
KeisenMode(1, 2)
StartOfLine()
CursorRight({2 * (1 + LABEL + 1)})
KeisenStart()
CursorDown({N})
KeisenEnd()
Keisen(1, 1)
CancelMode()
"""
END = "CursorDown(1)\nStartOfLine()\n" + ins("") + ins("以上") + fmt_prev("FormatRight()")

SAMPLES = [
    ("demo-notice", TITLE + BODY + TABLE + END),
]
