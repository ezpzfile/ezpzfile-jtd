def boxin(src, down, lines, right):
    return f'''OpenDocument("C:\\t\\src\\{src}.jtd")
JumpStart()
CursorDown({down})
StartOfLine()
InsertReturn(4)
CursorUp(4)
KeisenMode(2, 2)
KeisenStart()
CursorDown({lines})
CursorRight({right})
KeisenLineDivide(1)
KeisenEnd()
KeisenLineDivideInterval(1)
Keisen(1, 1)
CancelMode()
'''
SAMPLES = [
    ("i03-009-box", boxin("1242588_009", 2, 2, 20)),
    ("i04-004-box", boxin("1231907_004", 1, 2, 20)),
]
