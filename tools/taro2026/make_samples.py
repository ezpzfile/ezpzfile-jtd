#!/usr/bin/env python3
"""Make .jtd samples in the latest 一太郎 (under Wine) by running macro text files.

usage: make_samples.py SAMPLES.py OUT_DIR [NAME...]
SAMPLES.py defines SAMPLES = [(name, macro_body), ...]. Each body runs in a new
document (or in the document it opens with OpenDocument); the driver adds
NewDocument() before and CloseDocumentFile(...) after, which saves the sample.
The macro runs through RunFileMacro from the 「ステートメント実行」 dialog
(Esc menu, Z, S). Wine does not paint that dialog's run button, so the dialog
is moved and sized to put the button at a known place.
"""
import os, re, runpy, shutil, subprocess, sys, time

ENV = dict(os.environ, DISPLAY=os.environ.get("DISPLAY", ":94"),
           WINEPREFIX=os.environ.get("WINEPREFIX", os.path.expanduser("~/wine/taro")),
           WINEARCH="win64", LANG="ja_JP.UTF-8", LC_ALL="ja_JP.UTF-8", WINEDEBUG="-all")
EXE = r"C:\Program Files (x86)\JustSystems\TARO36\taro36.exe"
C = os.path.join(ENV["WINEPREFIX"], "drive_c")
SKIP = {"Default IME", "ジャストシステム製品インストールプログラム", "T36com"}
# the notice shown at start-up, titled with the product name and year
NOTICE = re.compile(r"^一太郎\s*\d{4}")
DLG = "マクロステートメント実行"
RUN_XY = (865, 220)  # run button with the dialog at (100,200) sized 820x330


def sh(*a, **k):
    return subprocess.run(list(a), env=ENV, capture_output=True, text=True, **k)


def windows():
    out = []
    for wid in sh("xdotool", "search", "--onlyvisible", "--name", ".").stdout.split():
        n = sh("xdotool", "getwindowname", wid).stdout.strip()
        if n and n not in SKIP:
            out.append((wid, n))
    return out


def names():
    return [n for _, n in windows()]


def key(*k):
    sh("xdotool", "key", *k)


def click(x, y):
    sh("xdotool", "mousemove", str(x), str(y), "click", "1")


def shot(p):
    sh("import", "-window", "root", p)


def start():
    sh("wineserver", "-k", timeout=60)
    time.sleep(1.5)
    afs = os.path.join(C, "users/root/AppData/Local/Justsystem/Taro36/~afstyle")
    if os.path.isdir(afs):
        for f in os.listdir(afs):
            if f.endswith(".$$$") or f == "lock.tmp":
                os.remove(os.path.join(afs, f))
    subprocess.Popen(["wine", EXE], env=ENV, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                     stderr=subprocess.DEVNULL, start_new_session=True)
    t0 = time.time()
    while time.time() - t0 < 90:
        time.sleep(1)
        ns = names()
        if any(NOTICE.match(n) for n in ns):
            click(748, 551)
            continue
        if "一太郎" in ns:
            key("Return")
            continue
        if any(n.startswith("一太郎 - [") for n in ns):
            time.sleep(3)
            if "一太郎" in names():
                key("Return")
            return open_dialog()
    raise SystemExit("Ichitaro did not start")


def open_dialog():
    for _ in range(3):
        click(450, 400)
        key("Escape")
        time.sleep(1)
        key("z")
        time.sleep(1)
        key("s")
        for _ in range(8):
            time.sleep(1)
            if DLG in names():
                break
        for wid, n in windows():
            if n == DLG:
                sh("xdotool", "windowmove", wid, "100", "200")
                sh("xdotool", "windowsize", wid, "820", "330")
                time.sleep(1)
                return True
        key("Escape")
        key("Escape")
    raise SystemExit("statement dialog did not open")


def place_dialog():
    for wid, n in windows():
        if n == DLG:
            sh("xdotool", "windowraise", wid)
            sh("xdotool", "windowmove", wid, "100", "200")
            sh("xdotool", "windowsize", wid, "820", "330")
            time.sleep(0.8)
            return True
    return False


def run(stmt):
    if not place_dialog():
        open_dialog()
        place_dialog()
    click(400, 300)  # the statement editor
    key("ctrl+a")
    key("BackSpace")
    sh("xdotool", "type", "--delay", "20", stmt)
    time.sleep(0.3)
    click(*RUN_XY)


def make(name, body, out):
    mac = os.path.join(C, "t", "mac")
    dst = os.path.join(C, "t", "out")
    os.makedirs(mac, exist_ok=True)
    os.makedirs(dst, exist_ok=True)
    target = os.path.join(dst, name + ".jtd")
    for p in (target, target + ".$$$"):
        if os.path.exists(p):
            os.remove(p)
    win = "C:\\t\\out\\" + name + ".jtd"
    head = "" if body.lstrip().startswith("OpenDocument(") else "NewDocument()\n"
    text = head + body.strip() + "\n" + f'CloseDocumentFile("{win}")\n'
    open(os.path.join(mac, name + ".txt"), "wb").write(text.replace("\n", "\r\n").encode("cp932"))
    run(f'RunFileMacro("C:\\t\\mac\\{name}.txt")')
    t0, seen = time.time(), []
    while time.time() - t0 < 60:
        time.sleep(1)
        ns = names()
        extra = [n for n in ns if n != DLG and not n.startswith("一太郎 - [")]
        if extra:
            shot(os.path.join(out, f"{name}-dlg{len(seen)}.png"))
            seen.append(extra[0])
            key("Return")
            time.sleep(1)
            if len(seen) > 4:
                break
            continue
        if os.path.exists(target) and not any(name + ".jtd" in n for n in ns):
            break
    time.sleep(1)
    ok = os.path.exists(target)
    if ok:
        shutil.copy(target, os.path.join(out, name + ".jtd"))
    return ok, seen


def main():
    samples = runpy.run_path(sys.argv[1])["SAMPLES"]
    out = sys.argv[2]
    only = set(sys.argv[3:])
    os.makedirs(out, exist_ok=True)
    if DLG not in names():
        start()
    for name, body in samples:
        if only and name not in only:
            continue
        ok, seen = make(name, body, out)
        print(name, "OK" if ok else "FAILED", " | ".join(seen), flush=True)


if __name__ == "__main__":
    main()
