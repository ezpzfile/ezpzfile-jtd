# Checking files in the real Ichitaro Viewer

JustSystems publishes 一太郎ビューア (Ichitaro Viewer) for free. We use it as
the referee: a file we wrote is good when the viewer opens it and shows what
we expect. It runs on Linux through Wine, so the check can be automated.

The viewer is **not** part of this repository. Get the installer from
JustSystems yourself and read its licence (it allows installing on any
number of computers; it forbids reverse engineering the program, which we do
not do — we only open documents with it).

## Setup (Ubuntu 24.04)

```sh
sudo dpkg --add-architecture i386 && sudo apt-get update
sudo apt-get install wine wine32:i386 xvfb xdotool xclip imagemagick \
    fonts-ipafont-mincho fonts-ipafont-gothic locales
sudo locale-gen ja_JP.UTF-8
Xvfb :99 -screen 0 1280x900x24 &
export DISPLAY=:99 WINEPREFIX=$HOME/wine/prefix WINEARCH=win32 LANG=ja_JP.UTF-8
wineboot -i
wine jstv3203.exe            # the viewer installer; click through the wizard
```

Map the Windows Japanese font names to IPA fonts so dialogs are readable
(`HKCU\Software\Wine\Fonts\Replacements`: `ＭＳ ゴシック` → `IPAGothic`,
`ＭＳ 明朝` → `IPAMincho`, `MS UI Gothic` → `IPAPGothic`, …).

The Japanese locale matters: on a Korean Windows (code page 949) the viewer
crashes on every file with an access violation. Wine with `LANG=ja_JP.UTF-8`
gives it code page 932 without touching any real computer.

## Use

```sh
python3 view.py  file.jtd shot.png --text shown.txt   # one file
python3 batch.py corpus_dir cache_dir                 # a folder (skips done files)
python3 compare.py corpus_dir cache_dir               # our text vs the viewer's text
```

`view.py` prints `OPEN`, `DIALOG <title>` (an error box, e.g. 「ファイルを
読み込むことができません。」), `CRASH` or `TIMEOUT`. With `--text` it copies
the document text out of the viewer (Ctrl+A, Ctrl+C), which `compare.py`
uses to measure how close our reader is.

Two prefixes (`WINEPREFIX`) on two displays can run side by side.
