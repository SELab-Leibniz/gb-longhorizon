#!/usr/bin/env python3
"""Hidden checks for the v2 showcase backlog (ISSUES/ and the waves in
harness/showcase/waves/): planted bugs (T..) and product decisions (P..).

    tickets_conformance.py CHECKOUT --roms ROMS --golden-trace DIR

Expects the agent's release binaries in CHECKOUT/target/release (gb, gb-server,
gb-trace, gb-web) and the WebAssembly module at
CHECKOUT/target/wasm32-unknown-unknown/release/gb_wasm.wasm (missing pieces
just fail their checks). Prints one JSON line:
{"tickets": {"score", "passed", "total", "checks"}, "decisions": {...}}; every
check carries its issue number and the wave that files the issue, so the
verifier can count only the waves that were delivered to the agent.

Every bug check passes on the unbugged reference code, fails with exactly that
bug planted, and passes with only that bug fixed (validate_tickets.sh).
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import socket
import subprocess
import sys
import tempfile
import time
import zlib
from collections import Counter
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "scripts"))
from romgen import make_rom, fix_checksums as _fix_checksums  # noqa: E402

# check -> (issue number, wave that files the issue)
TICKETS = {"T13": ("101", 0), "T11": ("102", 0), "T01": ("103", 0), "T04": ("104", 0), "T06": ("105", 0),
           "T07": ("106", 0), "T08": ("107", 0),
           "T14": ("112", 1), "T15": ("113", 1),
           "T16": ("116", 2), "T17": ("117", 2),
           "T18": ("120", 3), "T19": ("121", 3),
           "T20": ("123", 4)}
DECISIONS = {"P1": ("108", 0), "P4": ("109", 0), "P7": ("110", 0),
             "P3": ("114", 1), "P5": ("115", 1),
             "P2": ("118", 2), "P6": ("119", 2),
             "P8": ("122", 3),
             "P9": ("124", 4)}
PROFILE_ROM = "06-ld r,r"
RESULTS: dict = {}


def check(name, fn):
    try:
        ok, detail = fn()
    except Exception as e:
        ok, detail = False, f"{type(e).__name__}: {e}"
    RESULTS[name] = {"ok": bool(ok), "detail": str(detail)[:400]}


def run(cmd, timeout=300, **kw):
    try:
        p = subprocess.run(cmd, capture_output=True, text=True, errors="replace", timeout=timeout, **kw)
        return p.returncode, p.stdout, p.stderr
    except subprocess.TimeoutExpired:
        return 124, "", "timeout"
    except OSError as e:
        return 127, "", str(e)


def sha256(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


# ------------------------------------------------------------ crafted ROMs
def irq_priority_rom() -> bytes:
    """Requests VBlank and Timer together, then enables interrupts; each vector
    loops on itself, so PC shows which interrupt was serviced first."""
    rom = bytearray(make_rom(b"IRQPRIO", tag=b"irq-priority"))
    rom[0x40:0x42] = rom[0x50:0x52] = bytes([0x18, 0xFE])   # JR -2 at the VBlank and Timer vectors
    code = bytes([0xF3,              # DI
                  0x3E, 0x05,        # LD A,$05
                  0xE0, 0xFF,        # LDH ($FF),A  IE = VBlank | Timer
                  0xE0, 0x0F,        # LDH ($0F),A  IF = VBlank | Timer
                  0xFB,              # EI
                  0x00,              # NOP
                  0x18, 0xFE])       # JR -2
    rom[0x150:0x150 + len(code)] = code
    return _fix_checksums(rom)


def mbc5_banks_rom() -> bytes:
    """8 MiB MBC5 cartridge (512 banks); each bank's last 16 bytes start with
    its bank number, 16-bit little-endian (Mooneye's MBC5 tests only tell
    banks apart by one byte, so they cannot see the 9th bank bit)."""
    rom = bytearray(make_rom(b"MBC5BANKS", cart_type=0x19, size_code=8, tag=b"mbc5-banks"))
    for n in range(512):
        rom[n * 0x4000 + 0x3FF0: n * 0x4000 + 0x3FF2] = n.to_bytes(2, "little")
    return _fix_checksums(rom)


def halt_key_rom() -> bytes:
    """Waits for a key with HALT and the joypad interrupt (IME off), the usual
    'press a key' loop: white screen until it wakes, black after."""
    rom = bytearray(make_rom(b"HALTKEY", tag=b"halt-key"))
    code = bytes([0x3E, 0x81, 0xE0, 0x40,   # LD A,$81; LDH ($40),A   LCD on, BG on
                  0x3E, 0x10, 0xE0, 0x00,   # LD A,$10; LDH ($00),A   select the action buttons
                  0xAF, 0xE0, 0x47,         # XOR A; LDH ($47),A      BGP = 0: all white
                  0xE0, 0x0F,               # LDH ($0F),A             IF = 0
                  0x3E, 0x10, 0xE0, 0xFF,   # LD A,$10; LDH ($FF),A   IE = joypad
                  0xF3, 0x76, 0x00,         # DI; HALT; NOP
                  0x3E, 0xFF, 0xE0, 0x47,   # LD A,$FF; LDH ($47),A   BGP = $FF: all black
                  0x18, 0xFE])              # JR -2
    rom[0x150:0x150 + len(code)] = code
    return _fix_checksums(rom)


# ---------------------------------------------------------------- helpers
def gb_hash(gb, rom, frames, model="dmg", script=None):
    args = [gb, "--rom", str(rom), "--frames", str(frames), "--model", model, "--hash"]
    if script:
        args += ["--input-script", str(script)]
    code, out, err = run(args)
    m = re.search(r"final ([0-9a-f]{16})", out)
    return m.group(1) if m else f"exit{code}"


def mooneye_pass(gb, rom):
    return run([gb, "--rom", str(rom), "--frames", "1200", "--mooneye"])[0] == 10


def http(port, method, path, body=b"", headers=None, stall_after=None, stall_sec=0.0, timeout=60):
    """A raw HTTP/1.1 exchange; optionally stalls for `stall_sec` after sending
    `stall_after` bytes of the body (a slow uplink). Returns (status, headers, body)."""
    sock = socket.create_connection(("127.0.0.1", port), timeout=timeout)
    head = (f"{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nContent-Length: {len(body)}\r\n"
            + "".join(f"{k}: {v}\r\n" for k, v in (headers or {}).items()) + "\r\n").encode()
    try:
        if stall_after is None:
            sock.sendall(head + body)
        else:
            sock.sendall(head + body[:stall_after])
            time.sleep(stall_sec)
            sock.sendall(body[stall_after:])
    except OSError:
        pass                                   # the server may answer early and close
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
    head_b, _, rest = data.partition(b"\r\n\r\n")
    m = re.match(rb"HTTP/1\.[01] (\d{3})", head_b)
    hdrs = {}
    for line in head_b.split(b"\r\n")[1:]:
        k, _, v = line.decode("latin-1").partition(":")
        hdrs[k.strip().lower()] = v.strip()
    return (int(m.group(1)) if m else None), hdrs, rest


def png_idat(data: bytes) -> bytes:
    """Concatenated IDAT payload of a PNG (chunk CRCs checked)."""
    pos, idat = 8, b""
    while pos < len(data):
        n = int.from_bytes(data[pos:pos + 4], "big")
        kind, body = data[pos + 4:pos + 8], data[pos + 8:pos + 8 + n]
        if zlib.crc32(kind + body) & 0xFFFFFFFF != int.from_bytes(data[pos + 8 + n:pos + 12 + n], "big"):
            raise ValueError(f"bad CRC in {kind!r}")
        if kind == b"IDAT":
            idat += body
        pos += 12 + n
    return idat


# ------------------------------------------------------------------ main
def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("checkout", type=Path)
    ap.add_argument("--roms", type=Path, required=True)
    ap.add_argument("--golden-trace", type=Path, required=True)
    a = ap.parse_args()
    rel = a.checkout / "target" / "release"
    gb, server_bin, trace_bin, web_bin = (str(rel / n) for n in ("gb", "gb-server", "gb-trace", "gb-web"))
    wasm = a.checkout / "target" / "wasm32-unknown-unknown" / "release" / "gb_wasm.wasm"
    R = a.roms
    td = Path(tempfile.mkdtemp(prefix="gbtickets-"))
    individual = R / "test/blargg/cpu_instrs/individual"

    # ================================================== emulator core (frozen `gb` CLI)
    check("T11", lambda: (all(mooneye_pass(gb, R / f"test/mooneye/emulator-only/mbc1/{n}.gb") for n in ("rom_1Mb", "rom_2Mb")),
                          "mbc1/rom_1Mb + rom_2Mb"))

    def t13():   # dual-mode cartridges run as CGB under --model auto
        rom = R / "games/tobudx.gb"
        auto, cgb = gb_hash(gb, rom, 30, "auto"), gb_hash(gb, rom, 30, "cgb")
        return auto == cgb and not auto.startswith("exit"), (auto, cgb)
    check("T13", t13)

    def t14():   # the joypad interrupt fires on the press (a HALT-until-key loop wakes at once)
        rom = td / "halt-key.gb"
        rom.write_bytes(halt_key_rom())
        script = td / "a-10-30.input"
        script.write_text("10 A\n30\n")           # A held from frame 10 to frame 30
        before, held, after = (gb_hash(gb, rom, n, script=script) for n in (5, 20, 40))
        ok = before != held and held == after and not before.startswith("exit")
        return ok, {"frame 5 (waiting)": before, "frame 20 (A held)": held, "frame 40 (released)": after}
    check("T14", t14)

    # ================================================== debugger (gb-server)
    srv = None
    try:
        from api_conformance import Server as DebugServer
        srv = DebugServer(server_bin, doctor=False)
    except Exception as e:
        for t in ("T01", "T04"):
            RESULTS[t] = {"ok": False, "detail": f"gb-server: {e}"[:300]}
    if srv:
        try:
            def t01():   # MBC5's 9th ROM-bank bit (banks $100-$1FF of an 8 MiB cartridge)
                rom = td / "mbc5-banks.gb"
                rom.write_bytes(mbc5_banks_rom())
                srv.call("POST", "/load", {"path": str(rom), "model": "dmg"})
                got = {}
                for name, writes, bank in (("high-then-low", ((0x3000, 0x01), (0x2000, 0x23)), 0x123),
                                           ("low-then-high", ((0x2000, 0xFF), (0x3000, 0x01)), 0x1FF),
                                           ("back-to-low", ((0x3000, 0x00), (0x2000, 0x05)), 0x005)):
                    for addr, value in writes:
                        srv.call("POST", "/memory", {"addr": addr, "data": f"{value:02x}"})
                    st, mem = srv.call("GET", "/memory?addr=0x7FF0&len=2")
                    got[name] = ((mem or {}).get("data"), bank.to_bytes(2, "little").hex())
                return all(g == w for g, w in got.values()), got
            check("T01", t01)

            def t04():   # VBlank (bit 0) is serviced before Timer (bit 2)
                rom = td / "irq.gb"
                rom.write_bytes(irq_priority_rom())
                srv.call("POST", "/load", {"path": str(rom), "model": "dmg"})
                srv.call("POST", "/step", {"instructions": 40})
                st, regs = srv.call("GET", "/registers")
                pc = (regs or {}).get("pc")
                return st == 200 and pc == 0x0040, f"PC after both were requested = {pc} (want 64 = $40, the VBlank vector)"
            check("T04", t04)
        finally:
            srv.stop()

    # ================================================== gb-trace profile tie order
    def t08():
        ref = json.loads((a.golden_trace / f"{PROFILE_ROM}.profile.json").read_text())
        want = [(e["pc"], e["count"]) for e in ref["top"]]
        code, out, err = run([trace_bin, "--rom", str(individual / f"{PROFILE_ROM}.gb"), "--instructions",
                              str(ref["instructions"]), "--doctor", "--profile", "--top", "20"], timeout=600)
        got = [(int(p, 16), int(c)) for p, c in re.findall(r"PC:([0-9A-Fa-f]{4}) COUNT:(\d+)", out)]
        return got == want, {"first_diff": next(((g, w) for g, w in zip(got, want) if g != w), None), "n": len(got)}
    check("T08", t08)

    # ================================================== library service (gb-web)
    web_names = [n for n, _ in TICKETS.items() if n in ("T06", "T07", "T15", "T16", "T17", "T18", "T19", "T20")]
    try:
        from web_conformance import Server as WebServer, upload_raw, is_error, raw_post
        start_web = lambda: WebServer(web_bin, td / "lib", [R / "games"], wasm if wasm.exists() else td / "none.wasm")  # noqa: E731
        web = start_web()
    except Exception as e:
        web = None
        for t in web_names + list(DECISIONS):
            RESULTS[t] = {"ok": False, "detail": f"gb-web: {e}"[:300]}
    if web:
        try:
            games = (web.json("GET", "/api/games")[2] or {}).get("games", [])
            by_file = {g["filename"]: g for g in games}
            octet = {"Content-Type": "application/octet-stream"}

            # --- bugs ---------------------------------------------------------
            def t15():   # a double quote in a title is escaped in JSON (the listing stays valid)
                rom = make_rom(b'SAY "HI"', tag=b"t15")
                st, _, _ = web.req("POST", "/api/games", rom, {**octet, "X-Filename": "say-hi.gb"})
                st2, _, raw = web.req("GET", "/api/games")
                try:
                    listed = json.loads(raw)
                    titles = [g.get("title") for g in listed.get("games", [])]
                    ok = st == 201 and 'SAY "HI"' in titles
                    detail = (st, 'SAY "HI"' in titles)
                except ValueError as e:
                    ok, detail = False, f"listing is not valid JSON after the upload: {e}"
                web.req("DELETE", f"/api/games/{sha256(rom)}")        # leave the library as it was
                return ok, detail
            check("T15", t15)

            def t16():   # a '=' in the file name survives storage (the game stays listed)
                rom = make_rom(b"JAM", tag=b"t16")
                st, _, _ = web.req("POST", "/api/games", rom, {**octet, "X-Filename": "jam=2024.gb"})
                gid = sha256(rom)
                st2, _, g = web.json("GET", f"/api/games/{gid}")
                ids = [x["id"] for x in (web.json("GET", "/api/games")[2] or {}).get("games", [])]
                return (st == 201 and st2 == 200 and (g or {}).get("filename") == "jam=2024.gb" and gid in ids,
                        (st, st2, (g or {}).get("filename"), gid in ids))
            check("T16", t16)

            def t17():   # deleting a game deletes its save: a re-upload starts without one
                rom = make_rom(b"SAVEKEEP", cart_type=0x03, ram_code=0x02, tag=b"t17")
                gid = sha256(rom)
                st1, _, _ = web.req("POST", "/api/games", rom, {**octet, "X-Filename": "savekeep.gb"})
                st2, _, _ = web.req("PUT", f"/api/games/{gid}/save", b"\xab" * 8192, octet)
                st3, _, _ = web.req("DELETE", f"/api/games/{gid}")
                st4, _, _ = web.req("POST", "/api/games", rom, {**octet, "X-Filename": "savekeep.gb"})
                st5, _, _ = web.req("GET", f"/api/games/{gid}/save")
                return (st1, st2, st3, st4, st5) == (201, 204, 204, 201, 404), (st1, st2, st3, st4, st5)
            check("T17", t17)

            def t18():   # an upload whose body stalls for 2 s mid-way is stored whole
                rom = make_rom(b"SLOWLINK", tag=b"t18")
                st, _, body = http(web.port, "POST", "/api/games", rom, {**octet, "X-Filename": "slow.gb"},
                                   stall_after=8192, stall_sec=2.0)
                try:
                    b = json.loads(body)
                except ValueError:
                    b = {}
                return st == 201 and b.get("id") == sha256(rom), (st, b.get("id") == sha256(rom), b.get("code"))
            check("T18", t18)

            def t19():   # a %XX escape at the very end of the query is decoded ("C++" finds "QUEST C++")
                rom = make_rom(b"QUEST C++", tag=b"t19")
                web.req("POST", "/api/games", rom, {**octet, "X-Filename": "quest.gb"})
                st, _, b = web.json("GET", "/api/games?q=C%2B%2B")
                got = [g["id"] for g in (b or {}).get("games", [])]
                return st == 200 and sha256(rom) in got, (st, len(got))
            check("T19", t19)

            def t06():   # PNG screenshots are valid zlib streams (Adler-32)
                g = by_file["2048.gb"]
                st, h, data = web.req("GET", f"/api/games/{g['id']}/screenshot.png?frames=5")
                zlib.decompress(png_idat(data))        # raises on a bad Adler-32
                return st == 200, "valid PNG"
            check("T06", t06)

            def t07():   # $7F in a header title is shown as '?'
                rom = make_rom(b"AB\x7fCD", tag=b"t07")
                st, _, b = upload_raw(web, rom, "t07.gb")
                return st == 201 and (b or {}).get("title") == "AB?CD", (st, (b or {}).get("title"))
            check("T07", t07)

            def t20():   # a CGB screenshot (> 64 KiB of pixel data) inflates to the whole image
                g = by_file["tobudx.gb"]
                st, h, data = web.req("GET", f"/api/games/{g['id']}/screenshot.png?frames=5&model=cgb")
                d = zlib.decompressobj(-15)            # raw deflate: block structure only, not the Adler-32
                pixels = d.decompress(png_idat(data)[2:-4])
                want = 144 * (1 + 160 * 3)
                return st == 200 and len(pixels) == want and d.eof, (st, len(pixels), want, d.eof)
            check("T20", t20)

            # --- product decisions (harness/HIDDEN_SPEC.md) -------------------
            def p1():    # search also matches the mapper name
                st, _, b = web.json("GET", "/api/games?q=mbc5")
                want = sorted(g["id"] for g in games if any("mbc5" in g[k].lower() for k in ("title", "filename", "mapper")))
                got = sorted(g["id"] for g in (b or {}).get("games", []))
                return st == 200 and got == want and want, (len(got), len(want))
            check("P1", p1)

            def p2():    # save downloads are named after the game file, .sav
                g = next(g for g in games if g["battery"] and g["ram_size"] > 0)
                web.req("PUT", f"/api/games/{g['id']}/save", bytes(g["ram_size"]), octet)
                st, h, data = web.req("GET", f"/api/games/{g['id']}/save")
                want = g["filename"].rsplit(".", 1)[0] + ".sav"
                cd = h.get("content-disposition", "")
                return st == 200 and "attachment" in cd and f'filename="{want}"' in cd, cd
            check("P2", p2)

            def p3():    # statistics count games per mapper
                st, _, b = web.json("GET", "/api/stats")
                now = (web.json("GET", "/api/games")[2] or {}).get("games", [])
                want = dict(Counter(g["mapper"] for g in now))
                return st == 200 and (b or {}).get("by_mapper") == want, (b or {}).get("by_mapper")
            check("P3", p3)

            def p4():    # declined: re-uploading under a new name does not rename (409, unchanged)
                g = by_file["2048.gb"]
                rom = web.req("GET", f"/api/games/{g['id']}/rom")[2]
                st, _, b = upload_raw(web, rom, "2048-renamed.gb")
                after = web.json("GET", f"/api/games/{g['id']}")[2] or {}
                return st == 409 and after.get("filename") == "2048.gb", (st, after.get("filename"))
            check("P4", p4)

            def p5():    # declined: the API's default order stays title-ascending
                b = web.json("GET", "/api/games")[2] or {}
                ids = [g["id"] for g in b.get("games", [])]
                want = [g["id"] for g in sorted(b.get("games", []), key=lambda g: (g["title"].lower(), g["id"]))]
                return ids == want and ids, "default order"
            check("P5", p5)

            def p6():    # declined: the upload limit stays 8 MiB
                big = make_rom(b"BIG", cart_type=0x19, size_code=8, tag=b"d6")
                st, _, b = raw_post(web.port, "/api/games", big + b"\0", octet)
                return st == 413, st
            check("P6", p6)

            def p7():    # ruled out: no Japanese decoding, bytes outside $20-$7E stay '?' (OI-5)
                rom = make_rom(b"\xb6\xde\xd1 GB", tag=b"p7")
                st, _, b = upload_raw(web, rom, "kana.gb")
                return st == 201 and (b or {}).get("title") == "??? GB", (st, (b or {}).get("title"))
            check("P7", p7)

            def p8():    # favourites: PUT/DELETE /favorite, `favorite` field and filter, persistent
                nonlocal web
                g = by_file["2048.gb"]
                gid = g["id"]
                res = {}
                res["put"] = web.req("PUT", f"/api/games/{gid}/favorite")[0]
                res["field"] = (web.json("GET", f"/api/games/{gid}")[2] or {}).get("favorite")
                listed = [x["id"] for x in (web.json("GET", "/api/games?favorite=true")[2] or {}).get("games", [])]
                res["filter_true"] = listed == [gid]
                others = (web.json("GET", "/api/games?favorite=false")[2] or {}).get("games", [])
                res["filter_false"] = bool(others) and all(x.get("favorite") is False for x in others) and gid not in [x["id"] for x in others]
                st, _, b = web.json("GET", "/api/games?favorite=maybe")
                res["bad_value"] = st == 400 and is_error(b, "bad_request")
                st, _, b = web.json("PUT", "/api/games/" + "0" * 64 + "/favorite")
                res["unknown"] = st == 404 and is_error(b, "not_found")
                web.stop()
                web = start_web()                          # restart on the same library
                res["persists"] = (web.json("GET", f"/api/games/{gid}")[2] or {}).get("favorite")
                res["delete"] = web.req("DELETE", f"/api/games/{gid}/favorite")[0]
                res["cleared"] = (web.json("GET", f"/api/games/{gid}")[2] or {}).get("favorite")
                ok = (res["put"] == 204 and res["field"] is True and res["filter_true"] and res["filter_false"]
                      and res["bad_value"] and res["unknown"] and res["persists"] is True
                      and res["delete"] == 204 and res["cleared"] is False)
                return ok, res
            check("P8", p8)

            def p9():    # library export: GET /api/export, a JSON manifest without ROM data
                st, h, raw = web.req("GET", "/api/export")
                b = json.loads(raw)
                listed = (web.json("GET", "/api/games")[2] or {}).get("games", [])
                entries = b.get("games", [])
                keys_ok = all(set(e) == {"id", "title", "filename", "added", "has_save"} for e in entries)
                order_ok = [e["id"] for e in entries] == [g["id"] for g in sorted(listed, key=lambda g: (g["added"], g["id"]))]
                saves_ok = all(e["has_save"] == (web.req("GET", f"/api/games/{e['id']}/save")[0] == 200) for e in entries)
                cd = h.get("content-disposition", "")
                ok = (st == 200 and h.get("content-type", "").startswith("application/json") and b.get("version") == 1
                      and 'filename="library.json"' in cd and "attachment" in cd and keys_ok and order_ok and saves_ok
                      and any(e["has_save"] for e in entries))
                return ok, {"status": st, "version": b.get("version"), "keys": keys_ok, "order": order_ok,
                            "has_save": saves_ok, "disposition": cd}
            check("P9", p9)
        finally:
            if web:
                web.stop()

    def block(spec):
        names = list(spec)
        for n in names:
            RESULTS.setdefault(n, {"ok": False, "detail": "not reached"})
            RESULTS[n]["issue"], RESULTS[n]["wave"] = spec[n]
        passed = sum(1 for n in names if RESULTS[n]["ok"])
        return {"score": round(passed / len(names), 4), "passed": passed, "total": len(names),
                "checks": {n: RESULTS[n] for n in names}}
    print(json.dumps({"tickets": block(TICKETS), "decisions": block(DECISIONS)}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
