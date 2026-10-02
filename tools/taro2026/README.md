# Checking with the latest 一太郎

The free 一太郎ビューア (see `../taroview/`) shows whether a file opens. The
full program goes further: it can **make** documents, so we can ask it for
paired samples, and it shows whether a file we wrote works when edited. These
scripts drive the latest 一太郎 (installed as `TARO36`) under Wine on Linux.

一太郎 is **not** part of this repository; install your own copy.

## Setup (Ubuntu 24.04)

Same packages as `../taroview/README.md`, then a 64-bit prefix (the program
is 32-bit and runs in WOW64 mode):

```sh
Xvfb :94 -screen 0 1400x1000x24 &
export DISPLAY=:94 WINEPREFIX=$HOME/wine/taro WINEARCH=win64 LANG=ja_JP.UTF-8
wineboot -i
wine reg add 'HKCU\Software\Wine' /v Version /d win11 /f
wine <installer>.exe        # extract, then click through the installer
```

A notice window may show at every start; the scripts close it.

## Making samples

```sh
python3 make_samples.py samples/rules.py out/   # boxes and lines
python3 make_samples.py samples/types.py out/   # line types, 行間 / 通常
python3 make_samples.py samples/page.py out/    # paper, margins, 字数, 行数 (文書スタイル)
python3 make_samples.py samples/para.py out/    # indents, line feed (改行幅)
python3 make_samples.py samples/demo.py out/    # the notice in the README screenshot
```

`page.py` starts with `p00-probe`, which writes what the macro functions
report (`GetDocumentStyleMargin()` and so on) into the document itself, so
the saved file tells the defaults.

Two engine examples help to read the samples:
`cargo run --example ruletypes -- file.jtd` lists the style properties on the
words of each ruled line (where the line types are), and
`cargo run --example unitprops -- file.jtd FROM TO` prints the properties of
any range of text units.

A sample is a short 一太郎 macro (the help files `T36MAC.CHM` list the
functions). For example, a box over three lines:

```
Insert("あいうえおかきくけこさしすせそたちつてと", 1)   (five times)
CursorUp(5)
KeisenMode(2, 2)          half-width columns, rules between lines (行間)
KeisenStart()
CursorDown(2)
CursorRight(10)
KeisenEnd()
Keisen(1, 1)
CancelMode()
```

`make_samples.py` writes the macro to a text file, runs it with
`RunFileMacro` from the statement dialog, and saves the result with
`CloseDocumentFile`. Driving the program through OLE automation
(`JXW.Application`) does not work under Wine: the server never registers.

## Opening files

```sh
python3 open_check.py out/ files/*.jtd
```

starts 一太郎 with each file, closes the start-up notice, waits for the document,
and saves a screenshot and the text it shows (Ctrl+A, Ctrl+C). Message boxes
on the way are photographed and closed. Results: `out/results.tsv`.
