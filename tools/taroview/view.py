#!/usr/bin/env python3
"""Open a .jtd in the real Ichitaro Viewer (Wine) and report what happened.

Usage: view.py FILE.jtd OUT.png [--text OUT.txt]

Prints one line: STATUS<TAB>detail, where STATUS is
  OPEN    the document window appeared
  DIALOG  some other window appeared (message box) - its title is in detail
  CRASH   the viewer process ended without showing the document
  TIMEOUT nothing happened in time
Needs: Xvfb on $DISPLAY, wine prefix with TaroView installed (see README.md here).
"""
import os, subprocess, sys, time, shutil

PREFIX = os.environ.get("WINEPREFIX", "/home/claude/wine/prefix")
EXE = r"C:\Program Files\JustSystems\TaroView\TAROVIEW.EXE"
ENV = dict(os.environ, WINEPREFIX=PREFIX, WINEARCH="win32", LANG="ja_JP.UTF-8",
           LC_ALL="ja_JP.UTF-8", WINEDEBUG="-all", DISPLAY=os.environ.get("DISPLAY", ":99"))


def windows():
    r = subprocess.run(["xdotool", "search", "--onlyvisible", "--name", "."], capture_output=True, text=True, env=ENV)
    out = []
    for wid in r.stdout.split():
        n = subprocess.run(["xdotool", "getwindowname", wid], capture_output=True, text=True, env=ENV).stdout.strip()
        if n and n != "Default IME":
            out.append((wid, n))
    return out


def kill():
    subprocess.run(["pkill", "-f", "TAROVIEW.EXE"], env=ENV)
    time.sleep(1.0)


def main():
    src, png = sys.argv[1], sys.argv[2]
    text_out = sys.argv[sys.argv.index("--text") + 1] if "--text" in sys.argv else None
    kill()
    tmpdir = os.path.join(PREFIX, "drive_c", "t", "run")
    os.makedirs(tmpdir, exist_ok=True)
    name = os.path.basename(src)
    shutil.copy(src, os.path.join(tmpdir, name))
    p = subprocess.Popen(["wine", EXE, "C:\\t\\run\\" + name], env=ENV,
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    status, detail, wid = "TIMEOUT", "", None
    t0 = time.time()
    while time.time() - t0 < 40:
        time.sleep(1.0)
        ws = windows()
        doc = [w for w in ws if name in w[1]]
        other = [w for w in ws if name not in w[1] and "一太郎ビューア" not in w[1]] + \
                [w for w in ws if w[1].strip() == "一太郎ビューア"]
        if doc and not other:
            status, detail, wid = "OPEN", doc[0][1], doc[0][0]
            time.sleep(4.0)  # let it finish drawing
            ws2 = windows()
            other = [w for w in ws2 if name not in w[1] and w[1] != doc[0][1]]
            if other:
                status, detail, wid = "DIALOG", " | ".join(w[1] for w in other), other[0][0]
            break
        if other:
            time.sleep(2.0)
            status, detail, wid = "DIALOG", " | ".join(w[1] for w in other), other[0][0]
            break
        if p.poll() is not None and not subprocess.run(["pgrep", "-f", "TAROVIEW.EXE"], capture_output=True).stdout:
            status, detail = "CRASH", f"exit={p.returncode}"
            break
    if status == "OPEN":
        subprocess.run(["xdotool", "windowsize", wid, "1280", "900"], env=ENV)
        subprocess.run(["xdotool", "windowmove", wid, "0", "0"], env=ENV)
        time.sleep(2.5)
        if text_out:
            copy_text(wid, text_out)
    subprocess.run(["import", "-window", "root", png], env=ENV)
    kill()
    print(f"{status}\t{detail}")


def copy_text(wid, path):
    """Select all + copy in the viewer, then read the X clipboard."""
    subprocess.run(["xdotool", "windowactivate", wid], env=ENV, stderr=subprocess.DEVNULL)
    time.sleep(0.5)
    subprocess.run(["xdotool", "key", "--window", wid, "ctrl+a"], env=ENV, stderr=subprocess.DEVNULL)
    time.sleep(0.5)
    subprocess.run(["xdotool", "key", "--window", wid, "ctrl+c"], env=ENV, stderr=subprocess.DEVNULL)
    time.sleep(1.5)
    r = subprocess.run(["xclip", "-selection", "clipboard", "-o"], capture_output=True, env=ENV)
    open(path, "wb").write(r.stdout)
    subprocess.run(["xdotool", "key", "--window", wid, "Escape"], env=ENV, stderr=subprocess.DEVNULL)


if __name__ == "__main__":
    main()
