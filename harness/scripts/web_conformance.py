#!/usr/bin/env python3
"""Hidden conformance suite for the game library service (GEP 1 §7, Appendix D,
and the product owner's Open Issue answers in HIDDEN_SPEC.md).

    web_conformance.py GB_WEB --roms ROMS --wasm GB_WASM.wasm --gb GB_CLI

Starts `gb-web` on a fresh library seeded with ROMS/games, runs every check,
restarts it on the same library to test persistence, and prints one JSON line:
{"score", "passed", "total", "checks": {name: {"ok", "detail"}}}.

Expected values come from the GEP (computed here by romgen.expected_game),
from crafted ROMs (romgen.make_rom) and, for screenshots, from the agent's own
`gb` CLI — the same core seen through two interfaces must agree.
"""
from __future__ import annotations

import argparse
import concurrent.futures as cf
import http.client
import json
import os
import re
import signal
import socket
import subprocess
import sys
import tempfile
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from pngio import read_png, fnv1a64  # noqa: E402
from romgen import make_rom, expected_game, sha256  # noqa: E402

ALL_CHECKS = [
    "server_starts", "health", "seeded_list", "metadata", "default_order", "get_game", "unknown_id_404",
    "malformed_id_404", "download_rom", "query_q", "query_filters", "query_sort", "paging", "bad_query_400",
    "upload_raw", "upload_multipart", "not_a_rom", "unsupported_cartridge", "duplicate", "size_limit",
    "unsupported_media_type", "bad_multipart", "filename_sanitised", "title_rules", "delete",
    "method_not_allowed", "unknown_api_404", "screenshot_dmg", "screenshot_cgb", "screenshot_default",
    "screenshot_bad_params", "saves", "stats", "concurrent_distinct", "concurrent_same", "concurrent_reads",
    "static_pages", "survives_bad_requests", "default_order_mixed_case", "persistence", "seed_after_delete",
]
CHECKS: dict = {}
GAME_FIELDS = {"id": str, "title": str, "filename": str, "size": int, "cartridge_type": int, "mapper": str,
               "battery": bool, "rom_banks": int, "ram_size": int, "cgb": str, "sgb": bool,
               "header_checksum_ok": bool, "global_checksum_ok": bool, "playable": bool, "added": int}


def check(name, fn):
    try:
        ok, detail = fn()
    except Exception as e:  # any crash in a check is a failed check
        ok, detail = False, f"{type(e).__name__}: {e}"
    CHECKS[name] = {"ok": bool(ok), "detail": str(detail)[:400]}
    return ok


# ------------------------------------------------------------------ server
class Server:
    def __init__(self, binary, library, seeds, wasm):
        s = socket.socket()
        s.bind(("127.0.0.1", 0))
        self.port = s.getsockname()[1]
        s.close()
        args = [binary, "--port", str(self.port), "--library", str(library), "--wasm", str(wasm)]
        for d in seeds:
            args += ["--seed", str(d)]
        self.log = open(Path(library).parent / f"gb-web-{self.port}.log", "wb")
        self.proc = subprocess.Popen(args, stdout=self.log, stderr=subprocess.STDOUT, cwd=Path(library).parent,
                                     start_new_session=True)
        deadline = time.time() + 30
        while time.time() < deadline:
            if self.proc.poll() is not None:
                raise RuntimeError(f"gb-web exited with {self.proc.returncode}")
            try:
                if self.req("GET", "/api/health", timeout=2)[0] == 200:
                    return
            except Exception:
                pass
            time.sleep(0.25)
        raise RuntimeError("gb-web did not answer /api/health within 30 s")

    def req(self, method, path, body=None, headers=None, timeout=60):
        c = http.client.HTTPConnection("127.0.0.1", self.port, timeout=timeout)
        try:
            c.request(method, path, body=body, headers=headers or {})
            r = c.getresponse()
            data = r.read()
            return r.status, {k.lower(): v for k, v in r.getheaders()}, data
        finally:
            c.close()

    def json(self, method, path, body=None, headers=None, timeout=60):
        st, h, data = self.req(method, path, body, headers, timeout)
        try:
            return st, h, json.loads(data) if data else None
        except Exception:
            return st, h, None

    def alive(self):
        try:
            return self.proc.poll() is None and self.req("GET", "/api/health", timeout=5)[0] == 200
        except Exception:
            return False

    def stop(self):
        if self.proc.poll() is None:
            try:
                os.killpg(self.proc.pid, signal.SIGTERM)
                self.proc.wait(timeout=10)
            except Exception:
                os.killpg(self.proc.pid, signal.SIGKILL)
                self.proc.wait(timeout=10)
        self.log.close()


def multipart(fields, boundary="----gbconformance7d0e2"):
    """fields: list of (name, filename|None, bytes, content_type)."""
    out = b""
    for name, filename, data, ctype in fields:
        out += f"--{boundary}\r\n".encode()
        disp = f'form-data; name="{name}"' + (f'; filename="{filename}"' if filename is not None else "")
        out += f"Content-Disposition: {disp}\r\n".encode()
        if ctype:
            out += f"Content-Type: {ctype}\r\n".encode()
        out += b"\r\n" + data + b"\r\n"
    out += f"--{boundary}--\r\n".encode()
    return out, {"Content-Type": f"multipart/form-data; boundary={boundary}"}


def raw_post(port, path, body, headers, timeout=120):
    """POST that tolerates the server answering (e.g. 413) and closing before the body is sent."""
    sock = socket.create_connection(("127.0.0.1", port), timeout=timeout)
    head = (f"POST {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nContent-Length: {len(body)}\r\n"
            + "".join(f"{k}: {v}\r\n" for k, v in headers.items()) + "\r\n")
    try:
        sock.sendall(head.encode())
        for i in range(0, len(body), 1 << 16):
            sock.sendall(body[i:i + (1 << 16)])
    except OSError:
        pass                      # the server may answer early and close; read what it said
    data = b""
    try:
        while True:
            chunk = sock.recv(1 << 16)
            if not chunk:
                break
            data += chunk
    except OSError:
        pass
    sock.close()
    head, _, rest = data.partition(b"\r\n\r\n")
    m = re.match(rb"HTTP/1\.[01] (\d{3})", head)
    if b"transfer-encoding: chunked" in head.lower():
        out, rest2 = b"", rest
        while rest2:
            size_line, _, rest2 = rest2.partition(b"\r\n")
            n = int(size_line.split(b";")[0] or b"0", 16)
            if n == 0:
                break
            out, rest2 = out + rest2[:n], rest2[n + 2:]
        rest = out
    try:
        parsed = json.loads(rest)
    except Exception:
        parsed = None
    return (int(m.group(1)) if m else None), {}, parsed


def upload_raw(s, rom, filename=None):
    h = {"Content-Type": "application/octet-stream"}
    if filename is not None:
        h["X-Filename"] = filename
    return s.json("POST", "/api/games", rom, h)


def upload_mp(s, rom, filename):
    body, h = multipart([("rom", filename, rom, "application/octet-stream")])
    return s.json("POST", "/api/games", body, h)


def is_error(body, code):
    return isinstance(body, dict) and body.get("code") == code and isinstance(body.get("error"), str)


def game_shape_ok(g):
    return isinstance(g, dict) and all(isinstance(g.get(k), t) and not (t is int and isinstance(g.get(k), bool))
                                       for k, t in GAME_FIELDS.items())


def same_meta(got, want):
    return {k: (got.get(k), v) for k, v in want.items() if got.get(k) != v}


def default_key(g):
    return (g["title"].lower(), g["id"])


def frame_from_png(data, cgb):
    w, h, ch, rows = read_png(data)
    if (w, h) != (160, 144):
        raise ValueError(f"PNG is {w}x{h}")
    out = bytearray()
    for row in rows:
        for px in row:
            if cgb:
                if ch < 3:
                    raise ValueError("CGB screenshot must be RGB")
                r, g, b = px[0], px[1], px[2]
                v = (r >> 3) | ((g >> 3) << 5) | ((b >> 3) << 10)
                if any(((c >> 3) << 3) | (c >> 5) != c for c in (r, g, b)):
                    raise ValueError("CGB pixel not of the form (c<<3)|(c>>2)")
                out += bytes((v & 0xFF, v >> 8))
            else:
                grey = px[0]
                if ch >= 3 and not (px[0] == px[1] == px[2]):
                    raise ValueError("DMG screenshot pixel is not grey")
                if grey not in (255, 170, 85, 0):
                    raise ValueError(f"DMG grey level {grey} is not 255-85*s")
                out.append((255 - grey) // 85)
    return bytes(out)


def native_hash(gb, rom_path, frames, model):
    p = subprocess.run([gb, "--rom", str(rom_path), "--frames", str(frames), "--model", model, "--hash"],
                       capture_output=True, text=True, errors="replace", timeout=300)
    m = re.search(r"final ([0-9a-f]{16})", p.stdout)
    return m.group(1) if m else None


# -------------------------------------------------------------------- suite
def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("server")
    ap.add_argument("--roms", type=Path, required=True)
    ap.add_argument("--wasm", type=Path, required=True)
    ap.add_argument("--gb", required=True)
    a = ap.parse_args()
    seed_dir = a.roms / "games"
    seeds = {p.name: p.read_bytes() for p in sorted(seed_dir.glob("*.gb"))}
    td = Path(tempfile.mkdtemp(prefix="gbweb-"))
    lib = td / "library"
    stats = {"accepted": 0, "rejected": 0, "screens_ok": 0, "frames": 0}

    def counted(resp):
        if resp[0] == 201:
            stats["accepted"] += 1
        elif 400 <= resp[0] < 500:
            stats["rejected"] += 1
        return resp

    try:
        s = Server(a.server, lib, [seed_dir], a.wasm)
    except Exception as e:
        check("server_starts", lambda: (False, e))
        return finish()
    check("server_starts", lambda: (True, ""))
    try:
        run_checks(s, a, seeds, lib, td, stats, counted)
    except Exception as e:      # a crash mid-suite must not discard the checks already run
        CHECKS.setdefault("suite_aborted", {"ok": False, "detail": f"{type(e).__name__}: {e}"[:400]})
    finally:
        s.stop()
    if "before_restart" not in STATE:
        return finish()
    # ---- persistence: restart on the same library (seed dir passed again: OI-6)
    try:
        s2 = Server(a.server, lib, [seed_dir], a.wasm)
    except Exception as e:
        check("persistence", lambda: (False, f"restart failed: {e}"))
        return finish()
    try:
        def persistence():
            st, _, body = s2.json("GET", "/api/games")
            got = {g["id"]: g for g in body["games"]}
            want = STATE["before_restart"]
            diff = [i for i in want if i not in got or got[i] != want[i]]
            extra = [i for i in got if i not in want and i != STATE.get("deleted_seed")]
            st2, _, save = s2.req("GET", f"/api/games/{STATE['save_game']}/save")
            return (not diff and not extra and st2 == 200 and save == STATE["save_bytes"],
                    {"changed": diff[:3], "extra": extra[:3], "save": st2})
        check("persistence", persistence)

        def seed_after_delete():
            st, _, _ = s2.req("GET", f"/api/games/{STATE['deleted_seed']}")
            return st == 404, f"deleted seeded game after restart with --seed: {st}"
        check("seed_after_delete", seed_after_delete)
    finally:
        s2.stop()
    return finish()


STATE: dict = {}


def run_checks(s, a, seeds, lib, td, stats, counted):
    want_seed = {sha256(b): expected_game(b, n) for n, b in seeds.items()}

    def health():
        st, _, b = s.json("GET", "/api/health")
        return st == 200 and isinstance(b, dict) and b.get("ok") is True and b.get("games") == len(seeds), b
    check("health", health)

    def seeded_list():
        st, h, b = s.json("GET", "/api/games")
        ok = (st == 200 and "application/json" in h.get("content-type", "") and b["total"] == len(seeds)
              and {g["id"] for g in b["games"]} == set(want_seed) and all(game_shape_ok(g) for g in b["games"]))
        STATE["seed_list"] = b["games"] if st == 200 else []
        return ok, {"status": st, "total": b.get("total") if isinstance(b, dict) else None}
    check("seeded_list", seeded_list)

    def metadata():
        bad = {}
        for g in STATE["seed_list"]:
            d = same_meta(g, want_seed.get(g["id"], {}))
            if d:
                bad[g.get("filename")] = d
        return not bad and len(STATE["seed_list"]) == len(seeds), bad
    check("metadata", metadata)

    def default_order():
        ids = [g["id"] for g in STATE["seed_list"]]
        want = [g["id"] for g in sorted(STATE["seed_list"], key=default_key)]
        return ids == want and ids, [g["title"] for g in STATE["seed_list"]]
    check("default_order", default_order)

    some = sorted(STATE["seed_list"], key=lambda g: g["filename"]) or [{}]
    g0 = next((g for g in some if g.get("filename") == "2048.gb"), some[0])

    def get_game():
        st, _, b = s.json("GET", f"/api/games/{g0['id']}")
        return st == 200 and b == g0, b
    check("get_game", get_game)

    def unknown_id_404():
        st, _, b = s.json("GET", "/api/games/" + "0" * 64)
        return st == 404 and is_error(b, "not_found"), (st, b)
    check("unknown_id_404", unknown_id_404)

    def malformed_id_404():
        res = [s.req("GET", p)[0] for p in ("/api/games/" + g0["id"].upper(), "/api/games/abc",
                                            "/api/games/..%2F..%2Fetc%2Fpasswd", "/api/games/../../etc/passwd",
                                            f"/api/games/{g0['id']}/../../../etc/passwd")]
        return all(x == 404 for x in res), res
    check("malformed_id_404", malformed_id_404)

    def download_rom():
        st, h, data = s.req("GET", f"/api/games/{g0['id']}/rom")
        return (st == 200 and data == seeds[g0["filename"]] and "application/octet-stream" in h.get("content-type", "")
                and g0["filename"] in h.get("content-disposition", "")), (st, h.get("content-type"), len(data))
    check("download_rom", download_rom)

    def ids(path):
        st, _, b = s.json("GET", path)
        return st, (b or {}).get("total"), [g["id"] for g in (b or {}).get("games", [])]

    def query_q():
        st, total, got = ids("/api/games?q=TOBU")
        want = [g["id"] for g in STATE["seed_list"] if "tobu" in g["title"].lower() or "tobu" in g["filename"].lower()]
        st2, _, got2 = ids("/api/games?q=" + "zz-no-such-game")
        return st == 200 and got == want and total == len(want) and want and st2 == 200 and got2 == [], (got, want)
    check("query_q", query_q)

    def query_filters():
        out = {}
        for q, pred in (("cgb=dual", lambda g: g["cgb"] == "dual"), ("mapper=MBC5", lambda g: g["mapper"] == "MBC5"),
                        ("playable=true", lambda g: g["playable"]), ("cgb=none&mapper=ROM",
                                                                     lambda g: g["cgb"] == "none" and g["mapper"] == "ROM")):
            st, total, got = ids("/api/games?" + q)
            want = [g["id"] for g in STATE["seed_list"] if pred(g)]
            out[q] = st == 200 and got == want and total == len(want)
        return all(out.values()), out
    check("query_filters", query_filters)

    def query_sort():
        L = STATE["seed_list"]
        by_id = sorted(L, key=lambda g: g["id"])        # ties: id ascending, whatever the order
        cases = {
            "sort=size&order=desc": sorted(by_id, key=lambda g: g["size"], reverse=True),
            "sort=size": sorted(by_id, key=lambda g: g["size"]),
            "sort=title&order=desc": sorted(by_id, key=lambda g: g["title"].lower(), reverse=True),
            "sort=title": sorted(by_id, key=lambda g: g["title"].lower()),
            "sort=added&order=desc": sorted(by_id, key=lambda g: g["added"], reverse=True),
        }
        res = {}
        for q, want in cases.items():
            st, _, got = ids("/api/games?" + q)
            res[q] = st == 200 and got == [g["id"] for g in want]
        return all(res.values()), res
    check("query_sort", query_sort)

    def paging():
        full = [g["id"] for g in STATE["seed_list"]]
        st, total, got = ids("/api/games?limit=3&offset=2")
        st2, total2, got2 = ids(f"/api/games?offset={len(full)}")
        return st == 200 and total == len(full) and got == full[2:5] and st2 == 200 and got2 == [], (total, got)
    check("paging", paging)

    def bad_query_400():
        res = {q: s.json("GET", "/api/games?" + q) for q in ("sort=bogus", "limit=0", "offset=-1", "cgb=maybe",
                                                              "order=sideways", "playable=yes")}
        return all(r[0] == 400 and is_error(r[2], "bad_request") for r in res.values()), {k: v[0] for k, v in res.items()}
    check("bad_query_400", bad_query_400)

    # ---- uploads
    blargg = (a.roms / "test/blargg/cpu_instrs/cpu_instrs.gb").read_bytes()

    def upload_raw_check():
        st, h, b = counted(upload_raw(s, blargg, "cpu_instrs.gb"))
        STATE["blargg"] = b if st == 201 else None
        want = expected_game(blargg, "cpu_instrs.gb")
        return (st == 201 and h.get("location") == f"/api/games/{want['id']}" and game_shape_ok(b)
                and not same_meta(b, want)), (st, h.get("location"), b)
    check("upload_raw", upload_raw_check)

    mp_rom = make_rom(b"MULTIPART", tag=b"mp-1")

    def upload_multipart_check():
        st, h, b = counted(upload_mp(s, mp_rom, "my game.gb"))
        STATE["mp"] = b if st == 201 else None
        st2, _, lst = s.json("GET", "/api/games")
        listed = st2 == 200 and sha256(mp_rom) in {g["id"] for g in lst["games"]}
        return st == 201 and not same_meta(b, expected_game(mp_rom, "my game.gb")) and listed, (st, b)
    check("upload_multipart", upload_multipart_check)

    def not_a_rom():
        cases = {
            "text": b"hello, this is not a ROM\n" * 2000,
            "tiny": make_rom(b"TINY", tag=b"t")[:16384],
            "bad_logo": make_rom(b"NOLOGO", tag=b"l", logo=False),
            "bad_header_checksum": make_rom(b"BADSUM", tag=b"h", good_header=False),
            "size_mismatch": make_rom(b"SIZE", tag=b"s") + bytes(32768),
        }
        res = {k: counted(upload_raw(s, v, k + ".gb")) for k, v in cases.items()}
        st, _, lst = s.json("GET", "/api/games")
        stored = {sha256(v) for v in cases.values()} & {g["id"] for g in lst["games"]}
        return (all(r[0] == 400 and is_error(r[2], "not_a_rom") for r in res.values()) and not stored,
                {k: (r[0], (r[2] or {}).get("code")) for k, r in res.items()})
    check("not_a_rom", not_a_rom)

    def unsupported_cartridge():   # OI-3
        res = {hex(t): counted(upload_raw(s, make_rom(b"UNSUP", cart_type=t, tag=bytes([t])), "u.gb"))
               for t in (0x05, 0x20, 0xFC, 0x77)}
        return (all(r[0] == 422 and is_error(r[2], "unsupported_cartridge") for r in res.values()),
                {k: (r[0], (r[2] or {}).get("code")) for k, r in res.items()})
    check("unsupported_cartridge", unsupported_cartridge)

    def duplicate():               # OI-2
        before = s.json("GET", f"/api/games/{sha256(blargg)}")[2]
        st, _, b = counted(upload_mp(s, blargg, "renamed.gb"))
        after = s.json("GET", f"/api/games/{sha256(blargg)}")[2]
        return (st == 409 and is_error(b, "duplicate") and b.get("id") == sha256(blargg) and before == after
                and before is not None), (st, b, (after or {}).get("filename"))
    check("duplicate", duplicate)

    def size_limit():              # OI-1
        big = make_rom(b"BIG8MIB", cart_type=0x19, size_code=8, tag=b"big")
        assert len(big) == 8 * 1024 * 1024
        st1, _, b1 = counted(upload_raw(s, big, "big.gb"))
        st2, _, b2 = counted(raw_post(s.port, "/api/games", big + b"\x00",
                                      {"Content-Type": "application/octet-stream", "X-Filename": "toobig.gb"}))
        return st1 == 201 and st2 == 413 and is_error(b2, "too_large"), (st1, st2, b2)
    check("size_limit", size_limit)

    def unsupported_media_type():
        st, _, b = counted(s.json("POST", "/api/games", make_rom(b"MEDIA", tag=b"m"), {"Content-Type": "text/plain"}))
        return st == 415 and is_error(b, "unsupported_media_type"), (st, b)
    check("unsupported_media_type", unsupported_media_type)

    def bad_multipart():
        body, h = multipart([("file", "x.gb", make_rom(b"WRONGFIELD", tag=b"w"), "application/octet-stream")])
        st1, _, b1 = counted(s.json("POST", "/api/games", body, h))
        st2, _, b2 = counted(s.json("POST", "/api/games", b"--nope\r\ngarbage",
                                    {"Content-Type": "multipart/form-data; boundary=----gbconformance7d0e2"}))
        return (st1 == 400 and is_error(b1, "bad_request") and st2 == 400 and is_error(b2, "bad_request")), (st1, st2)
    check("bad_multipart", bad_multipart)

    def filename_sanitised():
        rom = make_rom(b"TRAVERSAL", tag=b"trav")
        st, _, b = counted(upload_mp(s, rom, "../../evil.gb"))
        escaped = [p for p in (td / "evil.gb", lib.parent.parent / "evil.gb", Path.cwd() / "evil.gb") if p.exists()]
        st2, _, b2 = counted(upload_raw(s, make_rom(b"TRAV2", tag=b"trav2"), "..\\..\\win.gb"))
        return (st == 201 and b.get("filename") == "evil.gb" and not escaped
                and st2 == 201 and "/" not in b2.get("filename", "/") and "\\" not in b2.get("filename", "\\")), \
            (st, (b or {}).get("filename"), escaped, (b2 or {}).get("filename"))
    check("filename_sanitised", filename_sanitised)

    def title_rules():             # OI-5
        cases = [
            (make_rom(b"\x01AB\x7fC", tag=b"np"), "np.gb", "?AB?C"),
            (make_rom(b"", tag=b"empty"), "empty-title.gb", "empty-title"),
            (make_rom(b"   ", tag=b"spaces"), "spaces.v1.gb", "spaces.v1"),
            (make_rom(b"COLOURTITLE1234X", cgb=0x80, tag=b"cgbt"), "c.gbc", "COLOURTITLE1234"),
            (make_rom(b"PAD   \x00JUNK", tag=b"pad"), "pad.gb", "PAD"),
        ]
        res = {}
        for rom, fn, want in cases:
            st, _, b = counted(upload_raw(s, rom, fn))
            res[fn] = (st, (b or {}).get("title"), want)
        return all(st == 201 and got == want for st, got, want in res.values()), res
    check("title_rules", title_rules)

    def delete():
        gid = sha256(mp_rom)
        st1, _, _ = s.req("DELETE", f"/api/games/{gid}")
        st2, _, b2 = s.json("GET", f"/api/games/{gid}")
        st3, _, b3 = s.json("DELETE", f"/api/games/{gid}")
        listed = gid in {g["id"] for g in s.json("GET", "/api/games")[2]["games"]}
        return st1 == 204 and st2 == 404 and st3 == 404 and is_error(b3, "not_found") and not listed, (st1, st2, st3)
    check("delete", delete)

    def method_not_allowed():
        st, _, b = s.json("PUT", "/api/games", b"{}", {"Content-Type": "application/json"})
        st2, _, b2 = s.json("POST", f"/api/games/{g0['id']}", b"", {"Content-Type": "application/octet-stream"})
        return st == 405 and is_error(b, "method_not_allowed") and st2 == 405, (st, st2)
    check("method_not_allowed", method_not_allowed)

    def unknown_api_404():
        st, h, b = s.json("GET", "/api/nope")
        return st == 404 and is_error(b, "not_found") and "application/json" in h.get("content-type", ""), (st, b)
    check("unknown_api_404", unknown_api_404)

    # ---- screenshots (pixels must equal the emulator frame the native CLI hashes)
    def shot(gid, query, cgb):
        st, h, data = s.req("GET", f"/api/games/{gid}/screenshot.png{query}", timeout=60)
        if st != 200:
            return st, None
        if "image/png" not in h.get("content-type", ""):
            return "not image/png", None
        stats["screens_ok"] += 1           # every request here is a distinct (game, frames, model)
        m = re.search(r"frames=(\d+)", query)
        stats["frames"] += int(m.group(1)) if m else 300
        return st, "%016x" % fnv1a64(frame_from_png(data, cgb))

    dmg_game = next((g for g in STATE["seed_list"] if g["filename"] == "tobudx.gb"), g0)

    def screenshot_dmg():
        res = {}
        for gname, frames in (("tobudx.gb", 120), ("2048.gb", 250)):
            g = next(g for g in STATE["seed_list"] if g["filename"] == gname)
            st, got = shot(g["id"], f"?frames={frames}&model=dmg", False)
            want = native_hash(a.gb, a.roms / "games" / gname, frames, "dmg")
            res[gname] = (st, got, want)
        return all(st == 200 and got == want and want for st, got, want in res.values()), res

    check("screenshot_dmg", screenshot_dmg)

    ucity = a.roms / "games-cgb/ucity.gbc"

    def screenshot_cgb():
        st, _, b = counted(upload_raw(s, ucity.read_bytes(), "ucity.gbc"))
        if st != 201:
            return False, f"upload of ucity.gbc: {st} {b}"
        st, got = shot(b["id"], "?frames=200", True)          # model auto -> cgb
        want = native_hash(a.gb, ucity, 200, "cgb")
        return st == 200 and got == want and want, (st, got, want)
    check("screenshot_cgb", screenshot_cgb)

    def screenshot_default():          # model auto: CGB for dual-mode cartridges
        cgb = dmg_game["cgb"] != "none"
        st, got = shot(dmg_game["id"], "", cgb)
        want = native_hash(a.gb, a.roms / "games" / dmg_game["filename"], 300, "cgb" if cgb else "dmg")
        return st == 200 and got == want and want, (st, got, want)
    check("screenshot_default", screenshot_default)

    def screenshot_bad_params():
        gid = dmg_game["id"]
        res = {q: s.json("GET", f"/api/games/{gid}/screenshot.png{q}")[0]
               for q in ("?frames=0", "?frames=3601", "?frames=abc", "?model=gba")}
        dmg_only = next((g for g in STATE["seed_list"] if g["cgb"] == "none"), None)
        if dmg_only:
            res["cgb-on-dmg-only"] = s.json("GET", f"/api/games/{dmg_only['id']}/screenshot.png?model=cgb")[0]
        res["unknown"] = s.json("GET", "/api/games/" + "1" * 64 + "/screenshot.png")[0]
        want = {k: 404 if k == "unknown" else 400 for k in res}
        return res == want, res
    check("screenshot_bad_params", screenshot_bad_params)

    # ---- battery saves
    def saves():
        g = next(g for g in STATE["seed_list"] if g["battery"] and g["ram_size"] > 0)
        nob = next(g for g in STATE["seed_list"] if not g["battery"])
        data = bytes((i * 7 + 3) & 0xFF for i in range(g["ram_size"]))
        r = {
            "get_none": s.req("GET", f"/api/games/{g['id']}/save")[0],
            "put": s.req("PUT", f"/api/games/{g['id']}/save", data, {"Content-Type": "application/octet-stream"})[0],
            "get": s.req("GET", f"/api/games/{g['id']}/save"),
            "wrong_len": s.json("PUT", f"/api/games/{g['id']}/save", data[:-1], {"Content-Type": "application/octet-stream"}),
            "no_battery": s.json("PUT", f"/api/games/{nob['id']}/save", bytes(8192), {"Content-Type": "application/octet-stream"}),
        }
        ok = (r["get_none"] == 404 and r["put"] == 204 and r["get"][0] == 200 and r["get"][2] == data
              and r["wrong_len"][0] == 400 and is_error(r["wrong_len"][2], "bad_request")
              and r["no_battery"][0] == 400 and is_error(r["no_battery"][2], "bad_request"))
        # delete + re-store for the persistence check
        d1 = s.req("DELETE", f"/api/games/{g['id']}/save")[0]
        d2 = s.req("GET", f"/api/games/{g['id']}/save")[0]
        d3 = s.req("DELETE", f"/api/games/{g['id']}/save")[0]
        s.req("PUT", f"/api/games/{g['id']}/save", data, {"Content-Type": "application/octet-stream"})
        STATE["save_game"], STATE["save_bytes"] = g["id"], data
        return ok and d1 == 204 and d2 == 404 and d3 == 404, {k: (v if isinstance(v, int) else v[0]) for k, v in r.items()}
    check("saves", saves)

    # ---- statistics (counters since start: every upload above went through counted())
    def stats_check():
        st, _, b = s.json("GET", "/api/stats")
        n = s.json("GET", "/api/games")[2]["total"]
        ok = (st == 200 and isinstance(b, dict) and b.get("games") == n
              and b.get("uploads_accepted") == stats["accepted"] and b.get("uploads_rejected") == stats["rejected"]
              and b.get("screenshots_rendered") == stats["screens_ok"] and b.get("emulated_frames") == stats["frames"]
              and isinstance(b.get("uptime_sec"), int) and not isinstance(b.get("uptime_sec"), bool)
              and b["uptime_sec"] >= 0)
        return ok, {"got": b, "want": dict(stats, games=n)}
    check("stats", stats_check)

    # ---- concurrency
    def concurrent_distinct():
        roms = [make_rom(b"PAR%02d" % i, tag=b"par-%d" % i) for i in range(16)]
        n0 = s.json("GET", "/api/games")[2]["total"]
        with cf.ThreadPoolExecutor(16) as ex:
            res = list(ex.map(lambda r: upload_raw(s, r, "p.gb")[0], roms))
        n1 = s.json("GET", "/api/games")[2]["total"]
        stats["accepted"] += sum(1 for x in res if x == 201)
        return res.count(201) == 16 and n1 == n0 + 16, (res, n0, n1)
    check("concurrent_distinct", concurrent_distinct)

    def concurrent_same():
        rom = make_rom(b"RACE", tag=b"race")
        with cf.ThreadPoolExecutor(8) as ex:
            res = list(ex.map(lambda _: upload_raw(s, rom, "race.gb")[0], range(8)))
        listed = [g for g in s.json("GET", "/api/games?q=race")[2]["games"] if g["id"] == sha256(rom)]
        return res.count(201) == 1 and res.count(409) == 7 and len(listed) == 1, res
    check("concurrent_same", concurrent_same)

    def concurrent_reads():
        with cf.ThreadPoolExecutor(32) as ex:
            res = list(ex.map(lambda _: s.req("GET", "/api/games"), range(64)))
        return all(r[0] == 200 for r in res) and len({r[2] for r in res}) == 1, [r[0] for r in res][:8]
    check("concurrent_reads", concurrent_reads)

    # ---- static pages and the wasm module
    def static_pages():
        r = {
            "index": s.req("GET", "/"),
            "play": s.req("GET", f"/play/{g0['id']}"),
            "play_unknown": s.req("GET", "/play/" + "2" * 64),
            "wasm": s.req("GET", "/gb_wasm.wasm"),
        }
        ok = (r["index"][0] == 200 and "text/html" in r["index"][1].get("content-type", "")
              and r["play"][0] == 200 and "text/html" in r["play"][1].get("content-type", "")
              and r["play_unknown"][0] == 404
              and r["wasm"][0] == 200 and "application/wasm" in r["wasm"][1].get("content-type", "")
              and r["wasm"][2] == a.wasm.read_bytes())
        return ok, {k: (v[0], v[1].get("content-type")) for k, v in r.items()}
    check("static_pages", static_pages)

    def survives_bad_requests():
        for payload in (b"\x00\xff garbage\r\n\r\n", b"GET /api/games HTTP/1.1\r\nHost: x\r\n",   # truncated
                        b"GET /api/games HTTP/1.1\r\nX-Big: " + b"a" * 70000 + b"\r\n\r\n",
                        b"POST /api/games HTTP/1.1\r\nContent-Length: 999999999\r\nContent-Type: application/octet-stream\r\n\r\nabc"):
            try:
                c = socket.create_connection(("127.0.0.1", s.port), timeout=5)
                c.sendall(payload)
                c.settimeout(3)
                try:
                    c.recv(4096)
                except Exception:
                    pass
                c.close()
            except Exception:
                pass
        time.sleep(0.5)
        return s.alive(), "server alive after malformed requests"
    check("survives_bad_requests", survives_bad_requests)

    def default_order_mixed_case():  # OI-4 with the lower-case titles uploaded above
        games = s.json("GET", "/api/games")[2]["games"]
        want = [g["id"] for g in sorted(games, key=default_key)]
        lower = [g["title"] for g in games if g["title"] != g["title"].upper()]
        return [g["id"] for g in games] == want and lower, lower[:4]
    check("default_order_mixed_case", default_order_mixed_case)

    # remember state for the restart checks; delete one seeded game (OI-6)
    victim = next((g for g in STATE["seed_list"] if g["filename"] != "tobudx.gb"
                   and g["id"] != STATE.get("save_game")), None)
    if victim is None:
        return                  # nothing listed: the restart checks stay "not reached"
    s.req("DELETE", f"/api/games/{victim['id']}")
    STATE["deleted_seed"] = victim["id"]
    STATE["before_restart"] = {g["id"]: g for g in s.json("GET", "/api/games")[2]["games"]}


def finish():
    for name in ALL_CHECKS:
        CHECKS.setdefault(name, {"ok": False, "detail": "not reached"})
    passed = sum(1 for k in ALL_CHECKS if CHECKS[k]["ok"])
    print(json.dumps({"score": round(passed / len(ALL_CHECKS), 4), "passed": passed, "total": len(ALL_CHECKS),
                      "checks": CHECKS}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
