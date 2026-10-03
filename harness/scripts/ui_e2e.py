#!/usr/bin/env python3
"""Hidden end-to-end tests for the web front end and in-browser player
(GEP 1 §8, Appendix E), driven through headless Chromium's DevTools protocol.

    ui_e2e.py GB_WEB --roms ROMS --wasm GB_WASM.wasm --gb GB_CLI [--chromium BIN]

Starts `gb-web` on a fresh library seeded with ROMS/games, drives the library
page and the player like a user (typing, file upload, clicks, key presses),
and compares what the player shows with the agent's native `gb` CLI for the
same ROM, frames and buttons. Prints one JSON line
{"score", "passed", "total", "checks"}.

Needs the `websocket` module (Debian: python3-websocket) and Chromium.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import urllib.request
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from pngio import pgm_to_frame, ppm_to_frame  # noqa: E402
from romgen import make_rom, sha256  # noqa: E402
from web_conformance import Server, upload_raw  # noqa: E402

ALL_CHECKS = [
    "browser_starts", "library_lists_games", "library_item_contract", "thumbnails_load", "search_filters",
    "search_clears", "upload_ok", "upload_error", "titles_escaped", "delete_ui", "player_ready",
    "player_frame_dmg", "player_canvas_dmg", "player_input", "player_frame_cgb", "player_canvas_cgb",
    "player_realtime", "player_controls", "no_uncaught_errors", "no_dialogs", "no_external_requests",
]
CHECKS: dict = {}


def check(name, fn):
    try:
        ok, detail = fn()
    except Exception as e:
        ok, detail = False, f"{type(e).__name__}: {e}"
    CHECKS[name] = {"ok": bool(ok), "detail": str(detail)[:400]}
    return ok


# ----------------------------------------------------------- DevTools client
class CDP:
    def __init__(self, ws_url, origin_ok):
        import websocket  # python3-websocket
        self.ws = websocket.create_connection(ws_url, timeout=60, suppress_origin=True, enable_multithread=False)
        self.n = 0
        self.events = []
        self.dialogs = []
        self.origin_ok = origin_ok

    def _handle(self, msg):
        m = msg.get("method")
        if m == "Page.javascriptDialogOpening":
            self.dialogs.append(msg["params"].get("type"))
            self.n += 1
            self.ws.send(json.dumps({"id": self.n, "method": "Page.handleJavaScriptDialog", "params": {"accept": True}}))
        elif m:
            self.events.append(msg)

    def send(self, method, params=None, timeout=60):
        self.n += 1
        mid = self.n
        self.ws.send(json.dumps({"id": mid, "method": method, "params": params or {}}))
        deadline = time.time() + timeout
        while True:
            left = deadline - time.time()
            if left <= 0:
                raise TimeoutError(f"{method} timed out")
            self.ws.settimeout(left)
            msg = json.loads(self.ws.recv())
            if msg.get("id") == mid:
                if "error" in msg:
                    raise RuntimeError(f"{method}: {msg['error'].get('message')}")
                return msg.get("result", {})
            self._handle(msg)

    def js(self, expr, timeout=60):
        r = self.send("Runtime.evaluate", {"expression": expr, "awaitPromise": True, "returnByValue": True,
                                           "userGesture": True}, timeout=timeout)
        if "exceptionDetails" in r:
            d = r["exceptionDetails"]
            raise RuntimeError("JS: " + str(d.get("exception", {}).get("description") or d.get("text"))[:300])
        return r.get("result", {}).get("value")

    def wait(self, expr, timeout=10, interval=0.1):
        deadline = time.time() + timeout
        last = None
        while time.time() < deadline:
            try:
                last = self.js(expr, timeout=max(1, deadline - time.time()))
                if last:
                    return last
            except Exception as e:
                last = repr(e)
            time.sleep(interval)
        raise TimeoutError(f"condition not met within {timeout}s: {expr[:120]} (last: {str(last)[:120]})")

    def goto(self, url, timeout=30):
        self.send("Page.navigate", {"url": url})
        self.wait("document.readyState === 'complete' && location.href.startsWith(%s)" % json.dumps(url.split("?")[0]),
                  timeout=timeout)

    def key(self, kind, key, code, vk):
        self.send("Input.dispatchKeyEvent", {"type": kind, "key": key, "code": code, "windowsVirtualKeyCode": vk,
                                             "nativeVirtualKeyCode": vk})

    def type_text(self, text):
        for ch in text:
            self.send("Input.dispatchKeyEvent", {"type": "keyDown", "key": ch, "text": ch, "unmodifiedText": ch,
                                                 "code": f"Key{ch.upper()}", "windowsVirtualKeyCode": ord(ch.upper())})
            self.send("Input.dispatchKeyEvent", {"type": "keyUp", "key": ch, "code": f"Key{ch.upper()}",
                                                 "windowsVirtualKeyCode": ord(ch.upper())})

    def set_files(self, selector, path):
        doc = self.send("DOM.getDocument", {"depth": 0})
        node = self.send("DOM.querySelector", {"nodeId": doc["root"]["nodeId"], "selector": selector})
        if not node.get("nodeId"):
            raise RuntimeError(f"{selector} not found")
        self.send("DOM.setFileInputFiles", {"files": [str(path)], "nodeId": node["nodeId"]})

    def drain(self):
        self.ws.settimeout(0.05)
        try:
            while True:
                self._handle(json.loads(self.ws.recv()))
        except Exception:
            pass

    def uncaught(self):
        self.drain()
        return [e["params"]["exceptionDetails"].get("exception", {}).get("description", "")[:200] or
                e["params"]["exceptionDetails"].get("text", "")
                for e in self.events if e.get("method") == "Runtime.exceptionThrown"]

    def foreign_requests(self):
        self.drain()
        urls = [e["params"]["request"]["url"] for e in self.events if e.get("method") == "Network.requestWillBeSent"]
        return [u for u in urls if not self.origin_ok(u)]


def start_chromium(binary, td):
    prof = td / "chrome-profile"
    prof.mkdir()
    proc = subprocess.Popen([binary, "--headless=new", "--no-sandbox", "--disable-gpu", "--disable-dev-shm-usage",
                             "--no-first-run", "--no-default-browser-check", "--remote-allow-origins=*",
                             "--remote-debugging-port=0", f"--user-data-dir={prof}", "--window-size=1024,768",
                             "about:blank"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                            start_new_session=True)
    port_file = prof / "DevToolsActivePort"
    deadline = time.time() + 30
    while time.time() < deadline and not port_file.exists():
        time.sleep(0.1)
    port = int(port_file.read_text().split()[0])
    for _ in range(100):
        try:
            targets = json.load(urllib.request.urlopen(f"http://127.0.0.1:{port}/json/list", timeout=2))
            page = next(t for t in targets if t.get("type") == "page")
            return proc, page["webSocketDebuggerUrl"]
        except Exception:
            time.sleep(0.1)
    raise RuntimeError("no page target")


def native(gb, rom, frames, model, script=None, dump=None):
    args = [gb, "--rom", str(rom), "--frames", str(frames), "--model", model, "--hash"]
    if script:
        args += ["--input-script", str(script)]
    if dump:
        args += ["--dump-frame", str(dump)]
    p = subprocess.run(args, capture_output=True, text=True, timeout=300)
    m = re.search(r"final ([0-9a-f]{16})", p.stdout)
    return m.group(1) if m else None


CANVAS_JS = """(() => {
  const s = document.getElementById('screen');
  const c = document.createElement('canvas'); c.width = 160; c.height = 144;
  const x = c.getContext('2d'); x.imageSmoothingEnabled = false; x.drawImage(s, 0, 0, 160, 144);
  return Array.from(x.getImageData(0, 0, 160, 144).data);
})()"""


def canvas_matches_dmg(rgba, shades):
    seen, used = {}, {}
    for i, s in enumerate(shades):
        px = tuple(rgba[4 * i:4 * i + 3])
        if seen.setdefault(s, px) != px:
            return False, f"shade {s} drawn in two colours"
        if used.setdefault(px, s) != s:
            return False, f"colour {px} used for two shades"
    return True, seen


def canvas_matches_cgb(rgba, frame555):
    for i in range(160 * 144):
        v = frame555[2 * i] | frame555[2 * i + 1] << 8
        want = tuple(((c << 3) | (c >> 2)) for c in (v & 31, (v >> 5) & 31, (v >> 10) & 31))
        if tuple(rgba[4 * i:4 * i + 3]) != want:
            return False, f"pixel {i}: {tuple(rgba[4 * i:4 * i + 3])} != {want}"
    return True, "exact"


# ------------------------------------------------------------------- suite
def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("server")
    ap.add_argument("--roms", type=Path, required=True)
    ap.add_argument("--wasm", type=Path, required=True)
    ap.add_argument("--gb", required=True)
    ap.add_argument("--chromium", default=shutil.which("chromium") or shutil.which("chromium-browser")
                    or shutil.which("google-chrome") or "chromium")
    a = ap.parse_args()
    td = Path(tempfile.mkdtemp(prefix="gbui-"))
    srv = browser = None
    try:
        srv = Server(a.server, td / "library", [a.roms / "games"], a.wasm)
        base = f"http://127.0.0.1:{srv.port}"
        browser, ws = start_chromium(a.chromium, td)
        cdp = CDP(ws, lambda u: u.startswith(base + "/") or u == base or u.split(":", 1)[0] in ("data", "blob", "about"))
        for m in ("Page.enable", "Runtime.enable", "Network.enable", "DOM.enable"):
            cdp.send(m)
        cdp.send("Emulation.setFocusEmulationEnabled", {"enabled": True})
        cdp.send("Page.bringToFront")
        check("browser_starts", lambda: (True, ""))
        run(cdp, srv, base, a, td)
        check("no_uncaught_errors", lambda: (not cdp.uncaught(), cdp.uncaught()[:5]))
        check("no_dialogs", lambda: (not cdp.dialogs, cdp.dialogs))
        check("no_external_requests", lambda: (not cdp.foreign_requests(), cdp.foreign_requests()[:5]))
    except Exception as e:
        CHECKS.setdefault("browser_starts", {"ok": False, "detail": f"{type(e).__name__}: {e}"[:400]})
    finally:
        if browser and browser.poll() is None:
            os.killpg(browser.pid, signal.SIGKILL)
        if srv:
            srv.stop()
    for name in ALL_CHECKS:
        CHECKS.setdefault(name, {"ok": False, "detail": "not reached"})
    passed = sum(1 for k in ALL_CHECKS if CHECKS[k]["ok"])
    print(json.dumps({"score": round(passed / len(ALL_CHECKS), 4), "passed": passed, "total": len(ALL_CHECKS),
                      "checks": CHECKS}))
    return 0


def run(cdp, srv, base, a, td):
    api = srv.json("GET", "/api/games")[2]["games"]
    LIST_IDS = "Array.from(document.querySelectorAll('#game-list [data-game-id]')).map(e => e.dataset.gameId)"

    # ---------------- library page
    cdp.goto(base + "/")

    def library_lists_games():
        got = cdp.wait(f"(() => {{ const v = {LIST_IDS}; return v.length === {len(api)} ? v : null; }})()", 10)
        titles = cdp.js("Array.from(document.querySelectorAll('#game-list [data-game-id]'))"
                        ".map(e => (e.querySelector('.game-title') || {}).textContent)")
        return got == [g["id"] for g in api] and [t.strip() if t else t for t in titles] == [g["title"] for g in api], \
            {"ids_in_order": got == [g["id"] for g in api], "titles": titles[:3]}
    check("library_lists_games", library_lists_games)

    def library_item_contract():
        info = cdp.js("""Array.from(document.querySelectorAll('#game-list [data-game-id]')).map(e => ({
            id: e.dataset.gameId,
            play: (e.querySelector('a.play') || {}).href || null,
            thumb: (e.querySelector('img.thumb') || {}).src || null,
            del: !!e.querySelector('button.delete')}))""")
        bad = [i for i in info if not (i["play"] and i["play"].endswith(f"/play/{i['id']}") and i["thumb"]
                                       and f"/api/games/{i['id']}/screenshot.png" in i["thumb"] and i["del"])]
        return info and not bad, bad[:3]
    check("library_item_contract", library_item_contract)

    def thumbnails_load():
        n = cdp.wait("""(() => { const im = Array.from(document.querySelectorAll('#game-list img.thumb'));
            return im.length && im.every(i => i.complete && i.naturalWidth === 160 && i.naturalHeight === 144)
                   ? im.length : 0; })()""", 60, 0.5)
        return n == len(api), n
    check("thumbnails_load", thumbnails_load)

    def search_filters():
        cdp.js("document.querySelector('#search').focus()")
        cdp.type_text("tobu")
        want = [g["id"] for g in srv.json("GET", "/api/games?q=tobu")[2]["games"]]
        got = cdp.wait(f"(() => {{ const v = {LIST_IDS}.filter(id => {{"
                       f"  const e = document.querySelector('[data-game-id=\"' + id + '\"]');"
                       f"  return e && e.offsetParent !== null; }}); return v.length === {len(want)} ? v : null; }})()", 2)
        return got == want and want, (got, want)
    check("search_filters", search_filters)

    def search_clears():
        cdp.js("(() => { const s = document.querySelector('#search'); s.focus(); s.select(); })()")
        cdp.key("keyDown", "Backspace", "Backspace", 8)
        cdp.key("keyUp", "Backspace", "Backspace", 8)
        got = cdp.wait(f"(() => {{ const v = {LIST_IDS}.filter(id => {{"
                       f"  const e = document.querySelector('[data-game-id=\"' + id + '\"]');"
                       f"  return e && e.offsetParent !== null; }}); return v.length === {len(api)} ? v : null; }})()", 2)
        return got == [g["id"] for g in api], got
    check("search_clears", search_clears)

    rom_ok = make_rom(b"UI UPLOAD", tag=b"ui-upload")
    up = td / "ui-upload.gb"
    up.write_bytes(rom_ok)

    def upload_ok():
        cdp.set_files("#upload-input", up)
        cdp.js("document.querySelector('#upload-button').click()")
        st = cdp.wait("(() => { const s = document.querySelector('#upload-status');"
                      f" const inList = !!document.querySelector('[data-game-id=\"{sha256(rom_ok)}\"]');"
                      " return s && s.dataset.state === 'ok' && inList; })()", 5)
        on_server = srv.req("GET", f"/api/games/{sha256(rom_ok)}")[0]
        return st and on_server == 200, on_server
    check("upload_ok", upload_ok)

    def upload_error():
        bad = td / "not-a-rom.gb"
        bad.write_bytes(b"definitely not a cartridge\n" * 3000)
        cdp.set_files("#upload-input", bad)
        cdp.js("document.querySelector('#upload-button').click()")
        st = cdp.wait("(() => { const s = document.querySelector('#upload-status');"
                      " return s && s.dataset.state === 'error' ? [s.dataset.errorCode, s.textContent.trim()] : null; })()", 5)
        return st[0] == "not_a_rom" and len(st[1]) > 0, st
    check("upload_error", upload_error)

    def titles_escaped():
        evil = make_rom(b"<img src=x>", tag=b"xss")
        upload_raw(srv, evil, "x.gb")
        cdp.goto(base + "/")
        sel = f"[data-game-id=\"{sha256(evil)}\"] .game-title"
        r = cdp.wait(f"(() => {{ const t = document.querySelector('{sel}');"
                     " return t ? [t.textContent.trim(), t.querySelectorAll('img').length] : null; })()", 10)
        return r[0] == "<img src=x>" and r[1] == 0, r
    check("titles_escaped", titles_escaped)

    def delete_ui():
        gid = sha256(rom_ok)
        cdp.js(f"document.querySelector('[data-game-id=\"{gid}\"] button.delete').click()")
        gone = cdp.wait(f"!document.querySelector('[data-game-id=\"{gid}\"]')", 5)
        return gone and srv.req("GET", f"/api/games/{gid}")[0] == 404, gone
    check("delete_ui", delete_ui)

    # ---------------- player (DMG)
    g2048 = next(g for g in api if g["filename"] == "2048.gb")
    rom2048 = a.roms / "games" / "2048.gb"
    cdp.goto(f"{base}/play/{g2048['id']}")

    def player_ready():
        cdp.js("window.gbTest && window.gbTest.ready", timeout=30)
        size = cdp.js("(() => { const c = document.getElementById('screen'); return c ? [c.width, c.height] : null; })()")
        return size == [160, 144] and cdp.js("gbTest.model()") == "dmg", size
    ready = check("player_ready", player_ready)

    def player_frame_dmg():
        cdp.js("gbTest.pause(); gbTest.reset();")
        n = cdp.js("gbTest.step(120)")
        got = cdp.js("gbTest.frameHash()")
        want = native(a.gb, rom2048, 120, "dmg", dump=td / "f2048.pgm")
        return n == 120 and got == want and want, (n, got, want)
    if ready:
        check("player_frame_dmg", player_frame_dmg)

    def player_canvas_dmg():
        shades = pgm_to_frame((td / "f2048.pgm").read_bytes())
        return canvas_matches_dmg(cdp.js(CANVAS_JS), shades)
    if ready:
        check("player_canvas_dmg", player_canvas_dmg)

    def player_input():
        # press START at frame 180, release at 200 (holding it from power-on changes nothing in 2048)
        cdp.js("gbTest.pause(); gbTest.reset(); gbTest.step(180)")
        cdp.key("keyDown", "Enter", "Enter", 13)
        cdp.js("gbTest.step(20)")
        cdp.key("keyUp", "Enter", "Enter", 13)
        cdp.js("gbTest.step(100)")
        got = cdp.js("gbTest.frameHash()")
        script = td / "start.input"
        script.write_text("180 START\n200\n")
        want = native(a.gb, rom2048, 300, "dmg", script=script)
        idle = native(a.gb, rom2048, 300, "dmg")
        return got == want and want and want != idle, (got, want, idle)
    if ready:
        check("player_input", player_input)

    def player_realtime():
        cdp.js("gbTest.resume()")
        f0, t0 = cdp.js("gbTest.frames()"), time.time()
        time.sleep(3)
        f1, t1 = cdp.js("gbTest.frames()"), time.time()
        rate = (f1 - f0) / (t1 - t0)
        cdp.js("gbTest.pause()")
        return 50 <= rate <= 70, round(rate, 1)
    if ready:
        check("player_realtime", player_realtime)

    def player_controls():
        cdp.js("gbTest.resume()")
        cdp.wait("document.querySelector('#status').textContent.trim() === 'running'", 3)
        cdp.js("document.querySelector('#pause').click()")
        paused = cdp.wait("document.querySelector('#status').textContent.trim() === 'paused'", 3)
        cdp.js("document.querySelector('#pause').click()")
        running = cdp.wait("document.querySelector('#status').textContent.trim() === 'running'", 3)
        cdp.js("document.querySelector('#reset').click()")
        time.sleep(0.5)
        after_reset = cdp.js("gbTest.frames()")
        fps = cdp.js("(document.querySelector('#fps') || {}).textContent || ''")
        cdp.js("gbTest.pause()")
        return paused and running and after_reset < 60 and re.search(r"\d", fps), (after_reset, fps)
    if ready:
        check("player_controls", player_controls)

    # ---------------- player (CGB)
    ucity = a.roms / "games-cgb/ucity.gbc"
    st, _, b = upload_raw(srv, ucity.read_bytes(), "ucity.gbc")

    def player_frame_cgb():
        cdp.goto(f"{base}/play/{sha256(ucity.read_bytes())}")
        cdp.js("gbTest.ready", timeout=30)
        cdp.js("gbTest.pause(); gbTest.reset();")
        cdp.js("gbTest.step(200)")
        got = cdp.js("gbTest.frameHash()")
        want = native(a.gb, ucity, 200, "cgb", dump=td / "fucity.ppm")
        return cdp.js("gbTest.model()") == "cgb" and got == want and want, (got, want)
    cgb_ok = check("player_frame_cgb", player_frame_cgb)

    def player_canvas_cgb():
        return canvas_matches_cgb(cdp.js(CANVAS_JS), ppm_to_frame((td / "fucity.ppm").read_bytes()))
    if cgb_ok:
        check("player_canvas_cgb", player_canvas_cgb)


if __name__ == "__main__":
    sys.exit(main())
