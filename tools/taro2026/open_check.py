#!/usr/bin/env python3
"""Open .jtd files one by one in the latest 一太郎 (under Wine) and record what happens.

usage: open_check.py OUT_DIR FILE.jtd...
Per file: start Ichitaro with the file, close the start-up notice, wait for the document,
save a screenshot (<name>.png) and the text copied out of Ichitaro (<name>.txt), then quit.
Any message box on the way is photographed (<name>-dlgN.png) and closed with Enter.
results.tsv: name, OPEN|TIMEOUT, seconds, message boxes seen.
"""
import os, re, shutil, subprocess, sys, time

ENV = dict(os.environ, DISPLAY=os.environ.get("DISPLAY", ":94"),
           WINEPREFIX=os.environ.get("WINEPREFIX", os.path.expanduser("~/wine/taro")),
           WINEARCH="win64", LANG="ja_JP.UTF-8", LC_ALL="ja_JP.UTF-8", WINEDEBUG="-all")
EXE = r"C:\Program Files (x86)\JustSystems\TARO36\taro36.exe"
RUN = os.path.join(ENV["WINEPREFIX"], "drive_c", "t", "run")
SKIP = {"Default IME", "ジャストシステム製品インストールプログラム", "T36com"}
# the notice shown at start-up, titled with the product name and year
NOTICE = re.compile(r"^一太郎\s*\d{4}")


def sh(*a, **k):
    return subprocess.run(list(a), env=ENV, capture_output=True, text=True, **k)


def windows():
    out = []
    for wid in sh("xdotool", "search", "--onlyvisible", "--name", ".").stdout.split():
        n = sh("xdotool", "getwindowname", wid).stdout.strip()
        if not n or n in SKIP:
            continue
        g = sh("xdotool", "getwindowgeometry", wid).stdout
        try:
            w, h = map(int, g.split("Geometry:")[1].split()[0].split("x"))
        except Exception:
            w, h = 0, 0
        out.append((wid, n, w, h))
    return out


def click(x, y):
    sh("xdotool", "mousemove", str(x), str(y), "click", "1")


def key(*k):
    sh("xdotool", "key", *k)


def shot(path):
    sh("import", "-window", "root", path)


def kill():
    sh("wineserver", "-k", timeout=60)
    time.sleep(1.5)


def is_msgbox(n, w, h):
    if w >= 800 or h >= 600:
        return False
    if n.startswith("一太郎 - [") or n.startswith("文書") or ".jtd" in n or NOTICE.match(n):
        return False
    return True


def open_one(path, out, idx):
    name = os.path.basename(path)[:-4]
    fname = "f%04d.jtd" % idx
    kill()
    os.makedirs(RUN, exist_ok=True)
    for f in os.listdir(RUN):
        os.remove(os.path.join(RUN, f))
    shutil.copy(path, os.path.join(RUN, fname))
    afs = os.path.join(ENV["WINEPREFIX"], "drive_c/users/root/AppData/Local/Justsystem/Taro36/~afstyle")
    if os.path.isdir(afs):
        for f in os.listdir(afs):
            if f.endswith(".$$$") or f == "lock.tmp":
                os.remove(os.path.join(afs, f))
    subprocess.Popen(["wine", EXE, "C:\\t\\run\\" + fname], env=ENV, stdin=subprocess.DEVNULL,
                     stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
    t0 = time.time()
    status, seen = "TIMEOUT", []
    while time.time() - t0 < 120:
        time.sleep(1)
        ws = windows()
        if any(NOTICE.match(n) for _, n, _, _ in ws):
            click(748, 551)  # the start-up notice: 閉じる
            continue
        boxes = [(wid, n) for wid, n, w, h in ws if is_msgbox(n, w, h)]
        if boxes:
            time.sleep(1)
            shot(os.path.join(out, f"{name}-dlg{len(seen)}.png"))
            seen.append(boxes[0][1])
            key("alt+n")  # 自動バックアップ… 読み込みますか? → いいえ
            time.sleep(1)
            if any(is_msgbox(n, w, h) for _, n, w, h in windows()):
                key("Return")
                time.sleep(1)
            if len(seen) > 5:
                break
            continue
        if any(fname in n and n.startswith("一太郎 - [") for _, n, _, _ in ws):
            time.sleep(4)
            if any(is_msgbox(n, w, h) for _, n, w, h in windows()):
                continue
            status = "OPEN"
            break
    secs = round(time.time() - t0, 1)
    shot(os.path.join(out, name + ".png"))
    if status == "OPEN":
        subprocess.run(["xclip", "-selection", "clipboard", "-i", "/dev/null"], env=ENV,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        key("ctrl+a")
        time.sleep(1)
        key("ctrl+c")
        time.sleep(1.5)
        try:
            r = subprocess.run(["xclip", "-selection", "clipboard", "-o"], env=ENV,
                               capture_output=True, timeout=20)
            open(os.path.join(out, name + ".txt"), "wb").write(r.stdout)
        except subprocess.TimeoutExpired:
            pass
    kill()
    return status, secs, " | ".join(seen)


def main():
    out = sys.argv[1]
    os.makedirs(out, exist_ok=True)
    res = os.path.join(out, "results.tsv")
    done = set()
    if os.path.exists(res):
        done = {l.split("\t")[0] for l in open(res, encoding="utf-8")}
    for i, f in enumerate(sys.argv[2:]):
        name = os.path.basename(f)[:-4]
        if name in done:
            continue
        st, secs, seen = open_one(f, out, i)
        with open(res, "a", encoding="utf-8") as fo:
            fo.write(f"{name}\t{st}\t{secs}\t{seen}\n")
        print(name, st, secs, seen, flush=True)
    kill()


if __name__ == "__main__":
    main()
