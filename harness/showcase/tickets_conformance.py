#!/usr/bin/env python3
"""Hidden checks for the v2 showcase tickets (harness/tickets/) and product decisions.

    tickets_conformance.py CHECKOUT --roms ROMS --golden-trace DIR

Expects the agent's release binaries in CHECKOUT/target/release (gb, gb-server,
gb-trace, gb-web) and the WebAssembly module at
CHECKOUT/target/wasm32-unknown-unknown/release/gb_wasm.wasm (missing pieces
just fail their checks). Prints one JSON line:
{"tickets": {"score", "passed", "total", "checks"}, "decisions": {...}}.

Every bug check passes on the unbugged reference code and fails with exactly
that bug planted (validate_tickets.sh).
"""
from __future__ import annotations

import argparse
import json
import re
import shutil
import subprocess
import sys
import tempfile
import zlib
from collections import Counter
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "scripts"))
from romgen import make_rom, header_checksum, global_checksum  # noqa: E402

TICKETS = ["T01", "T02", "T03", "T04", "T05", "T06", "T07", "T08", "T09", "T10", "T11", "T12", "T13"]
DECISIONS = ["P1", "P2", "P3", "P4", "P5", "P6", "P7"]
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


def joypad_rom() -> bytes:
    """Reads the action buttons (A, B, Select, Start) and writes P1 into BGP every
    loop: the whole screen's shade follows the buttons held."""
    rom = bytearray(make_rom(b"JOYPAD", tag=b"joypad-bgp"))
    code = bytes([0x3E, 0x10,        # LD A,$10    select the action-button group
                  0xE0, 0x00,        # LDH ($00),A
                  0xF0, 0x00,        # LDH A,($00)
                  0xF0, 0x00,        # LDH A,($00)  (second read: settled)
                  0xE0, 0x47,        # LDH ($47),A  BGP = buttons
                  0x18, 0xF4])       # JR -12
    rom[0x150:0x150 + len(code)] = code
    return _fix_checksums(rom)


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


def wrap_rom() -> bytes:
    """A cartridge whose first two bytes ($C3 $A7) differ from anything at the top of memory."""
    rom = bytearray(make_rom(b"WRAP", tag=b"wrap"))
    rom[0x0000:0x0002] = bytes([0xC3, 0xA7])
    return _fix_checksums(rom)


def _fix_checksums(rom: bytearray) -> bytes:
    rom[0x14D] = header_checksum(rom)
    g = global_checksum(rom)
    rom[0x14E], rom[0x14F] = g >> 8, g & 0xFF
    return bytes(rom)


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


def blargg_pass(gb, rom):
    code, out, err = run([gb, "--rom", str(rom), "--frames", "7200", "--serial-stdout"], timeout=600)
    return "Passed" in out


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

    # --- emulator core tickets (via the frozen `gb` CLI)
    check("T02", lambda: (blargg_pass(gb, individual / "04-op r,imm.gb") and blargg_pass(gb, individual / "09-op r,r.gb"),
                          "cpu_instrs 04 + 09"))
    check("T03", lambda: (blargg_pass(gb, individual / "01-special.gb"), "cpu_instrs 01-special"))
    check("T11", lambda: (all(mooneye_pass(gb, R / f"test/mooneye/emulator-only/mbc1/{n}.gb") for n in ("rom_1Mb", "rom_2Mb")),
                          "mbc1/rom_1Mb + rom_2Mb"))

    def t13():   # dual-mode cartridges run as CGB under --model auto
        rom = R / "games/tobudx.gb"
        auto, cgb = gb_hash(gb, rom, 30, "auto"), gb_hash(gb, rom, 30, "cgb")
        return auto == cgb and not auto.startswith("exit"), (auto, cgb)
    check("T13", t13)

    # --- debugger-server tickets (register level: independent of the CPU)
    srv = None
    try:
        sys.path.insert(0, str(Path(__file__).resolve().parent))
        from api_conformance import Server as DebugServer
        srv = DebugServer(server_bin, doctor=False)
    except Exception as e:
        for t in ("T01", "T04", "T05", "T09"):
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

            def t09():   # GET /memory wraps at $FFFF (distinct bytes at $FFFE, $FFFF, $0000, $0001)
                rom = td / "wrap.gb"
                rom.write_bytes(wrap_rom())
                srv.call("POST", "/load", {"path": str(rom), "model": "dmg"})
                srv.call("POST", "/memory", {"addr": 0xFFFE, "data": "5a"})
                srv.call("POST", "/memory", {"addr": 0xFFFF, "data": "1f"})
                st, mem = srv.call("GET", "/memory?addr=0xFFFE&len=4")
                return st == 200 and (mem or {}).get("data", "").lower() == "5a1fc3a7", mem
            check("T09", t09)

            def t05():   # BCPS auto-increment crosses index $1F -> $20
                st, _ = srv.call("POST", "/load", {"path": str(R / "games-cgb/ucity.gbc"), "model": "cgb"})
                srv.call("POST", "/memory", {"addr": 0xFF40, "data": "00"})       # LCD off: palette RAM always writable
                srv.call("POST", "/memory", {"addr": 0xFF68, "data": "9e"})       # auto-increment, index $1E
                for b in ("11", "22", "33", "44"):
                    srv.call("POST", "/memory", {"addr": 0xFF69, "data": b})
                srv.call("POST", "/memory", {"addr": 0xFF68, "data": "20"})
                st2, mem = srv.call("GET", "/memory?addr=0xFF69&len=1")
                return st == 200 and mem.get("data", "").lower() == "33", mem
            check("T05", t05)

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

    # --- gb-trace profile tie order (reference profile: equal counts listed by ascending PC)
    def t08():
        ref = json.loads((a.golden_trace / f"{PROFILE_ROM}.profile.json").read_text())
        want = [(e["pc"], e["count"]) for e in ref["top"]]
        code, out, err = run([trace_bin, "--rom", str(individual / f"{PROFILE_ROM}.gb"), "--instructions",
                              str(ref["instructions"]), "--doctor", "--profile", "--top", "20"], timeout=600)
        got = [(int(p, 16), int(c)) for p, c in re.findall(r"PC:([0-9A-Fa-f]{4}) COUNT:(\d+)", out)]
        return got == want, {"first_diff": next(((g, w) for g, w in zip(got, want) if g != w), None), "n": len(got)}
    check("T08", t08)

    # --- WebAssembly button bits (crafted ROM: buttons -> BGP)
    def t10():
        if not wasm.exists() or not shutil.which("node"):
            return False, "no wasm module or no node"
        rom = td / "joypad.gb"
        rom.write_bytes(joypad_rom())
        res = {}
        for name, mask in (("A", 0x10), ("B", 0x20)):
            js = td / "j.js"
            js.write_text(f"""const fs=require('fs');(async()=>{{const {{instance}}=await WebAssembly.instantiate(fs.readFileSync({json.dumps(str(wasm))}),{{}});
const e=instance.exports,rom=fs.readFileSync({json.dumps(str(rom))});const p=e.gb_alloc(rom.length);
new Uint8Array(e.memory.buffer,p,rom.length).set(rom);e.gb_load(p,rom.length,0);e.gb_set_buttons({mask});e.gb_run_frames(3);
const f=new Uint8Array(e.memory.buffer,e.gb_frame_ptr(),e.gb_frame_len());let h=0xcbf29ce484222325n;
for(const b of f)h=((h^BigInt(b))*0x100000001b3n)&0xffffffffffffffffn;console.log(h.toString(16).padStart(16,'0'));}})();""")
            code, out, err = run(["node", str(js)])
            script = td / f"{name}.input"
            script.write_text(f"0 {name}\n")
            res[name] = (out.strip(), gb_hash(gb, rom, 3, "dmg", script))
        ok = all(w == n and not n.startswith("exit") for w, n in res.values()) and res["A"][1] != res["B"][1]
        return ok, res
    check("T10", t10)

    # --- library-service tickets and product decisions (one gb-web instance)
    try:
        from web_conformance import Server as WebServer, upload_raw, is_error
        web = WebServer(web_bin, td / "lib", [R / "games"], wasm if wasm.exists() else td / "none.wasm")
    except Exception as e:
        web = None
        for t in ("T06", "T07", "T12") + tuple(DECISIONS):
            RESULTS[t] = {"ok": False, "detail": f"gb-web: {e}"[:300]}
    if web:
        try:
            games = (web.json("GET", "/api/games")[2] or {}).get("games", [])
            by_file = {g["filename"]: g for g in games}

            def t06():   # PNG screenshots are valid zlib streams (Adler-32)
                g = by_file["2048.gb"]
                st, h, data = web.req("GET", f"/api/games/{g['id']}/screenshot.png?frames=5")
                pos, idat = 8, b""
                while pos < len(data):
                    n = int.from_bytes(data[pos:pos + 4], "big")
                    kind, body = data[pos + 4:pos + 8], data[pos + 8:pos + 8 + n]
                    if zlib.crc32(kind + body) & 0xFFFFFFFF != int.from_bytes(data[pos + 8 + n:pos + 12 + n], "big"):
                        return False, f"bad CRC in {kind!r}"
                    if kind == b"IDAT":
                        idat += body
                    pos += 12 + n
                zlib.decompress(idat)        # raises on a bad Adler-32
                return st == 200, "valid PNG"
            check("T06", t06)

            def t07():   # $7F in a header title is shown as '?'
                rom = make_rom(b"AB\x7fCD", tag=b"t07")
                st, _, b = upload_raw(web, rom, "t07.gb")
                return st == 201 and b.get("title") == "AB?CD", (st, (b or {}).get("title"))
            check("T07", t07)

            def t12():   # a save must be exactly ram_size bytes
                g = next(g for g in games if g["battery"] and g["ram_size"] > 0)
                st, _, b = web.json("PUT", f"/api/games/{g['id']}/save", bytes(g["ram_size"] + 1),
                                    {"Content-Type": "application/octet-stream"})
                return st == 400 and is_error(b, "bad_request"), st
            check("T12", t12)

            # product decisions P1-P6 (harness/HIDDEN_SPEC.md): new behaviour and traps
            def p1():    # search also matches the mapper name
                st, _, b = web.json("GET", "/api/games?q=mbc5")
                want = sorted(g["id"] for g in games if any("mbc5" in g[k].lower() for k in ("title", "filename", "mapper")))
                got = sorted(g["id"] for g in (b or {}).get("games", []))
                return st == 200 and got == want and want, (len(got), len(want))
            check("P1", p1)

            def p2():    # save downloads are named after the game file, .sav
                g = next(g for g in games if g["battery"] and g["ram_size"] > 0)
                web.req("PUT", f"/api/games/{g['id']}/save", bytes(g["ram_size"]), {"Content-Type": "application/octet-stream"})
                st, h, data = web.req("GET", f"/api/games/{g['id']}/save")
                want = g["filename"].rsplit(".", 1)[0] + ".sav"
                cd = h.get("content-disposition", "")
                return st == 200 and "attachment" in cd and f'filename="{want}"' in cd, cd
            check("P2", p2)

            def p3():    # statistics count games per mapper
                st, _, b = web.json("GET", "/api/stats")
                now = (web.json("GET", "/api/games")[2] or {}).get("games", [])
                want = dict(Counter(g["mapper"] for g in now))
                return st == 200 and b.get("by_mapper") == want, (b or {}).get("by_mapper")
            check("P3", p3)

            def p4():    # trap: re-uploading under a new name does not rename (409, unchanged)
                g = by_file["2048.gb"]
                rom = web.req("GET", f"/api/games/{g['id']}/rom")[2]
                st, _, b = upload_raw(web, rom, "2048-renamed.gb")
                after = web.json("GET", f"/api/games/{g['id']}")[2] or {}
                return st == 409 and after.get("filename") == "2048.gb", (st, after.get("filename"))
            check("P4", p4)

            def p5():    # trap: the API's default order stays title-ascending
                b = web.json("GET", "/api/games")[2] or {}
                ids = [g["id"] for g in b.get("games", [])]
                want = [g["id"] for g in sorted(b.get("games", []), key=lambda g: (g["title"].lower(), g["id"]))]
                return ids == want and ids, "default order"
            check("P5", p5)

            def p6():    # trap: the upload limit stays 8 MiB
                from web_conformance import raw_post
                big = make_rom(b"BIG", cart_type=0x19, size_code=8, tag=b"d6")
                st, _, b = raw_post(web.port, "/api/games", big + b"\0", {"Content-Type": "application/octet-stream"})
                return st == 413, st
            check("P6", p6)

            def p7():    # trap: no Japanese decoding, bytes outside $20-$7E stay '?' (#104, OI-5)
                rom = make_rom(b"\xb6\xde\xd1 GB", tag=b"p7")
                st, _, b = upload_raw(web, rom, "kana.gb")
                return st == 201 and (b or {}).get("title") == "??? GB", (st, (b or {}).get("title"))
            check("P7", p7)
        finally:
            web.stop()

    def block(names):
        for n in names:
            RESULTS.setdefault(n, {"ok": False, "detail": "not reached"})
        passed = sum(1 for n in names if RESULTS[n]["ok"])
        return {"score": round(passed / len(names), 4), "passed": passed, "total": len(names),
                "checks": {n: RESULTS[n] for n in names}}
    print(json.dumps({"tickets": block(TICKETS), "decisions": block(DECISIONS)}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
