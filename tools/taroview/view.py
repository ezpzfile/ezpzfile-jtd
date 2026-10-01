#!/usr/bin/env python3
"""Open a .jtd in the real Ichitaro Viewer (Wine) and report what happened.

Usage: view.py FILE.jtd OUT.png [--text OUT.txt]

Prints one line: STATUS<TAB>detail, where STATUS is
  OPEN    the document window appeared
  DIALOG  some other window appeared (message box) - its title is in detail
  CRASH   the viewer process ended without showing the document
  TIMEOUT nothing happened in time
With --text, the document text is copied out of the viewer (Ctrl+A, Ctrl+C).
Needs: Xvfb on $DISPLAY, wine prefix with TaroView installed (see README.md here).
"""
import os, subprocess, sys, time, shutil

PREFIX = os.environ.get("WINEPREFIX", "/home/claude/wine/prefix")
EXE = r"C:\Program Files\JustSystems\TaroView\TAROVIEW.EXE"
BARE = "一太郎ビューア"  # main window title before a document is shown
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
    # only this prefix's programs (several prefixes can run side by side)
    subprocess.run(["wineserver", "-k"], env=ENV, stdin=subprocess.DEVNULL,
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=30)
    time.sleep(1.0)


def main():
    src, png = sys.argv[1], sys.argv[2]
    text_out = sys.argv[sys.argv.index("--text") + 1] if "--text" in sys.argv else None
    kill()
    tmpdir = os.path.join(PREFIX, "drive_c", "t", "run")
    os.makedirs(tmpdir, exist_ok=True)
    name = os.path.basename(src)
    shutil.copy(src, os.path.join(tmpdir, name))
    p = subprocess.Popen(["wine", EXE, "C:\\t\\run\\" + name], env=ENV, stdin=subprocess.DEVNULL,
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
    status, detail, wid = "TIMEOUT", "", None
    t0 = time.time()
    bare_since = None
    while time.time() - t0 < 40 + float(os.environ.get("TAROVIEW_WAIT", "15")):
        time.sleep(1.0)
        ws = windows()
        doc = [w for w in ws if name in w[1]]
        bare = [w for w in ws if w[1].strip() == BARE]
        other = [w for w in ws if name not in w[1] and w[1].strip() != BARE]
        if other:
            time.sleep(1.0)
            status, detail, wid = "DIALOG", " | ".join(w[1] for w in other), other[0][0]
            break
        if doc:
            time.sleep(4.0)  # let it finish drawing
            extra = [w for w in windows() if name not in w[1] and w[1].strip() != BARE]
            if extra:
                status, detail, wid = "DIALOG", " | ".join(w[1] for w in extra), extra[0][0]
            else:
                status, detail, wid = "OPEN", doc[0][1], doc[0][0]
            break
        if bare:
            bare_since = bare_since or time.time()
            if time.time() - bare_since > float(os.environ.get("TAROVIEW_WAIT", "15")):
                status, detail, wid = "DIALOG", BARE + " (no document)", bare[0][0]
                break
        if p.poll() is not None and not windows():
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
    # xclip -i stays in the background to serve the selection: detach its output
    subprocess.run(["xclip", "-selection", "clipboard", "-i", "/dev/null"], env=ENV,
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
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
