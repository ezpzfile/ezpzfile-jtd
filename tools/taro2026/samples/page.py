# Page setup (文書スタイル): paper, margins, characters per line, lines per page.
# Each sample changes one setting of the same text. p00-probe writes the
# current settings and the names of the registered papers into the document,
# so the saved file says what the defaults are.
L = "あいうえおかきくけこさしすせそたちつてと"
T = "\n".join(f'Insert("{L}", 1)' for _ in range(5)) + "\n"


def style(*calls):
    return T + "\n".join(calls) + "\nSetDocumentStyle()\n"


PROBE = """%p = GetDocumentStylePaper()
foreach %v in %p
Insert("A " & ForeachIndex() & "=" & %v, 1)
next
%m = GetDocumentStyleMargin()
foreach %v in %m
Insert("M " & ForeachIndex() & "=" & %v, 1)
next
%l = GetDocumentStyleLayout()
foreach %v in %l
Insert("L " & ForeachIndex() & "=" & %v, 1)
next
%list = GetPrintPaperList()
foreach %v in %list
Insert("P " & %v, 1)
next
"""

SAMPLES = [
    ("p00-probe", PROBE),
    ("p01-base", T),
    ("p02-mtop20", style("DocumentStyleMargin(20, , , , 1)")),
    ("p03-mbottom20", style("DocumentStyleMargin(, 20, , , 1)")),
    ("p04-mleft20", style("DocumentStyleMargin(, , 20, , 1)")),
    ("p05-mright20", style("DocumentStyleMargin(, , , 20, 1)")),
    ("p06-chars30", style("DocumentStyleLayout(30)")),
    ("p07-lines30", style("DocumentStyleLayout(, 30)")),
    ("p08-chars35-lines40", style("DocumentStyleLayout(35, 40)")),
    ("p09-vertical", style("DocumentStyleLayout(, , , , , 2)")),
    ("p10-font12", style("DocumentStyleFont(, , , , , 12, 3)")),
    ("p11-b5", style('DocumentStylePaper("B5 単票・縦方向")')),
    ("p12-a4-landscape", style('DocumentStylePaper("A4 単票・横方向")')),
    ("p13-a3", style('DocumentStylePaper("A3 単票・縦方向")')),
    ("p14-linegap100", style("DocumentStyleLayout(, , , 100, 100, , 0)")),
    ("p15-margins-all", style("DocumentStyleMargin(15, 25, 20, 10, 1)")),
]
