#!/usr/bin/env python3
"""Hidden conformance suite for the CR-3 debugger API (docs/specs/debugger-api.md).

    api_conformance.py GB_SERVER --roms ROMS --golden-trace DIR --gb GB_CLI

Prints a JSON line {"score", "passed", "total", "checks": {name: {"ok", "detail"}}}.

Expected values come only from: the Gameboy Doctor reference trace of
cpu_instrs/01-special (hidden 50k-line excerpt), the opcode table, small
hand-assembled programs written into WRAM through the API itself, and the
agent's own `gb` CLI (for screenshot hashes) — so the suite tests the API
contract, not other parts of the emulator twice.
"""
from __future__ import annotations

import argparse
import gzip
import json
import re
import socket
import subprocess
import sys
import time
import urllib.error
import urllib.request
from collections import Counter
from pathlib import Path

CHECKS: dict = {}


def check(name, ok, detail=""):
    CHECKS[name] = {"ok": bool(ok), "detail": str(detail)[:300]}


# ---------------------------------------------------------------- HTTP client
class Server:
    def __init__(self, binary, doctor):
        s = socket.socket()
        s.bind(("127.0.0.1", 0))
        self.port = s.getsockname()[1]
        s.close()
        args = [binary, "--port", str(self.port)] + (["--doctor"] if doctor else [])
        self.proc = subprocess.Popen(args, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        deadline = time.time() + 15
        while time.time() < deadline:
            try:
                if self.call("GET", "/health")[0] == 200:
                    return
            except Exception:
                pass
            time.sleep(0.2)
        raise RuntimeError("server did not answer /health within 15 s")

    def call(self, method, path, body=None, raw=None, timeout=120):
        data = raw if raw is not None else (json.dumps(body).encode() if body is not None else None)
        req = urllib.request.Request(f"http://127.0.0.1:{self.port}{path}", data=data, method=method,
                                     headers={"Content-Type": "application/json"})
        try:
            with urllib.request.urlopen(req, timeout=timeout) as r:
                return r.status, json.loads(r.read() or b"null")
        except urllib.error.HTTPError as e:
            try:
                return e.code, json.loads(e.read() or b"null")
            except Exception:
                return e.code, None

    def stop(self):
        if self.proc.poll() is None:
            self.proc.kill()
        self.proc.wait(timeout=10)


# ------------------------------------------------------------ reference data
REG_RE = re.compile(r"A:(\w\w) F:(\w\w) B:(\w\w) C:(\w\w) D:(\w\w) E:(\w\w) H:(\w\w) L:(\w\w) SP:(\w{4}) PC:(\w{4})")


def parse_line(line):
    m = REG_RE.search(line)
    keys = ["a", "f", "b", "c", "d", "e", "h", "l", "sp", "pc"]
    return {k: int(v, 16) for k, v in zip(keys, m.groups())}


def regs_match(got, want):
    if not isinstance(got, dict):
        return False, "no registers in response"
    diff = {k: (got.get(k), v) for k, v in want.items() if got.get(k) != v}
    return not diff, diff


# ------------------------------------------------- reference disassembler
class Disasm:
    def __init__(self, opcodes_json):
        d = json.loads(Path(opcodes_json).read_text())
        self.base = {int(k, 16): v for k, v in d["unprefixed"].items()}
        self.cb = {int(k, 16): v for k, v in d["cbprefixed"].items()}

    def decode(self, mem, addr):
        """mem: callable(addr)->byte. Returns (length, canonical form)."""
        op = mem(addr)
        if op == 0xCB:
            ent, length = self.cb[mem(addr + 1)], 2
            imm_at = addr + 2
        else:
            ent, length = self.base[op], self.base[op]["bytes"]
            imm_at = addr + 1
        mnemonic = ent["mnemonic"]
        if mnemonic.startswith("ILLEGAL") or mnemonic.startswith("PREFIX"):
            return length, (mnemonic, [])
        ops = []
        operands = ent["operands"]
        i = 0
        while i < len(operands):
            o = operands[i]
            name = o["name"]
            if name in ("n8", "a8", "e8"):
                v = mem(imm_at)
                if name == "e8":
                    v = v - 256 if v >= 128 else v
                    if mnemonic == "JR":
                        v = (addr + 2 + v) & 0xFFFF
                elif name == "a8":
                    v = 0xFF00 + v
                txt, vals = "#", [v]
            elif name in ("n16", "a16"):
                txt, vals = "#", [mem(imm_at) | (mem(imm_at + 1) << 8)]
            elif name.startswith("$"):
                txt, vals = "#", [int(name[1:], 16)]
            elif name.isdigit():                      # bit numbers (BIT 7, H)
                txt, vals = "#", [int(name)]
            else:
                txt, vals = name.upper(), []
                if o.get("increment"):
                    txt += "+"
                if o.get("decrement"):
                    txt += "-"
                if name == "SP" and o.get("increment") and i + 1 < len(operands) and operands[i + 1]["name"] == "e8":
                    v = mem(imm_at)
                    txt, vals = "SP+#", [v - 256 if v >= 128 else v]
                    i += 1
            if name == "C" and mnemonic == "LDH" and not o["immediate"]:
                txt, vals = "#+C", [0xFF00]
            if not o["immediate"]:
                txt = f"[{txt}]"
            ops.append((txt, vals))
            i += 1
        if mnemonic == "LDH":
            mnemonic = "LD"
        return length, (mnemonic, ops)


NUM_RE = re.compile(r"(?<![A-Z0-9])(-?\$[0-9A-F]+|-?0X[0-9A-F]+|-?[0-9A-F]+H\b|-?\d+)")


def canon_text(text):
    """Agent text -> canonical (mnemonic, [(template, [ints])])."""
    t = text.strip().upper().replace("(", "[").replace(")", "]")
    t = t.replace("HLI", "HL+").replace("HLD", "HL-")
    parts = t.split(None, 1)
    mnemonic = parts[0]
    rest = parts[1] if len(parts) > 1 else ""
    if mnemonic == "LDH":
        mnemonic = "LD"
    ops = []
    for raw in [x for x in re.split(r",(?![^\[]*\])", rest) if x.strip()]:
        raw = raw.replace(" ", "")
        raw = raw.replace("[C]", "[$FF00+C]")
        vals = []

        def num(m):
            s = m.group(0)
            neg = s.startswith("-")
            s = s.lstrip("-")
            if s.startswith("$"):
                v = int(s[1:], 16)
            elif s.startswith("0X"):
                v = int(s[2:], 16)
            elif s.endswith("H"):
                v = int(s[:-1], 16)
            else:
                v = int(s)
            vals.append(-v if neg else v)
            return "#"
        tmpl = NUM_RE.sub(num, raw)
        tmpl = tmpl.replace("SP+-#", "SP+#").replace("SP-#", "SP+#") if "SP" in tmpl else tmpl
        if "SP-" in raw and vals:
            vals[-1] = -abs(vals[-1])
        ops.append((tmpl, vals))
    return mnemonic, ops


def same_instruction(got_text, want):
    try:
        gm, gops = canon_text(got_text)
    except Exception:
        return False
    wm, wops = want
    if gm != wm or len(gops) != len(wops):
        return False
    for (gt, gv), (wt, wv) in zip(gops, wops):
        # tolerate "$FF00+C" vs "#+C" templates and bare register bit numbers
        if gt.replace("$", "") != wt and not (wt == "[#+C]" and gt in ("[#+C]", "[C]")):
            # bit numbers / RST vectors may be written as plain numbers
            if not (gt == "#" and wt == "#"):
                return False
        if len(gv) != len(wv):
            if wt == "[#+C]" and not gv:
                continue
            return False
        for a, b in zip(gv, wv):
            if a == b or (a - b) % 256 == 0 or a + 0xFF00 == b:
                continue
            return False
    return True


# -------------------------------------------------------------------- suite
def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("server")
    ap.add_argument("--roms", type=Path, required=True)
    ap.add_argument("--golden-trace", type=Path, required=True)
    ap.add_argument("--gb", required=True, help="the agent's gb CLI (for screenshot hashes)")
    a = ap.parse_args()

    rom01 = a.roms / "test" / "blargg" / "cpu_instrs" / "individual" / "01-special.gb"
    acid = a.roms / "test" / "acid2" / "dmg-acid2.gb"
    game = a.roms / "games" / "2048.gb"
    trace = gzip.open(a.golden_trace / "01-special.head50k.txt.gz", "rt").read().splitlines()
    dis = Disasm(a.golden_trace / "Opcodes.json")
    rom_bytes = rom01.read_bytes()

    # ---------------- doctor-mode server: trace-exact CPU behaviour
    try:
        s = Server(a.server, doctor=True)
    except Exception as e:
        check("server_starts", False, e)
        return finish()
    check("server_starts", True)
    try:
        st, body = s.call("GET", "/health")
        check("health", st == 200 and isinstance(body, dict) and body.get("ok") is True, body)
        st, body = s.call("GET", "/registers")
        check("409_before_load", st == 409 and isinstance(body, dict) and "error" in body, (st, body))
        st, body = s.call("GET", "/no-such-endpoint")
        check("404_unknown_endpoint", st == 404 and isinstance(body, dict) and "error" in body, (st, body))

        st, body = s.call("POST", "/load", {"path": str(rom01), "model": "dmg"})
        check("load", st == 200 and isinstance(body, dict) and body.get("ok") is True and body.get("model") == "dmg", body)
        st, body = s.call("POST", "/step", raw=b"{not json")
        check("400_malformed_json", st == 400 and isinstance(body, dict) and "error" in body, (st, body))

        st, regs = s.call("GET", "/registers")
        ok, diff = regs_match(regs, parse_line(trace[0]))
        check("registers_after_load", st == 200 and ok, diff)

        done = 0
        for target in (1, 7, 99, 1999, 19999, 49999):
            st, regs = s.call("POST", "/step", {"instructions": target - done})
            done = target
            ok, diff = regs_match(regs, parse_line(trace[target]))
            check(f"step_to_line_{target + 1}", st == 200 and ok, diff)

        st, _ = s.call("POST", "/reset")
        st2, regs = s.call("GET", "/registers")
        ok, diff = regs_match(regs, parse_line(trace[0]))
        check("reset", st == 200 and st2 == 200 and ok, diff)

        s.call("POST", "/step", {"instructions": 1999})
        st, prof = s.call("GET", "/profile?top=10")
        counts = Counter(parse_line(l)["pc"] for l in trace[:1999])
        want = [{"pc": pc, "count": c} for pc, c in sorted(counts.items(), key=lambda kv: (-kv[1], kv[0]))[:10]]
        check("profile", st == 200 and isinstance(prof, dict) and prof.get("instructions") == 1999
              and prof.get("hot") == want, prof if not isinstance(prof, dict) else {"got": prof.get("hot", [])[:3], "want": want[:3]})

        # breakpoint accuracy: first and second occurrence of a PC in the trace
        pcs = [parse_line(l)["pc"] for l in trace]
        first, bp = {}, None
        for i, pc in enumerate(pcs):
            if pc not in first:
                first[pc] = i
        for pc, i in sorted(first.items(), key=lambda kv: kv[1]):
            if i > 3000 and pc in pcs[i + 1:]:
                bp, l1 = pc, i
                l2 = pcs.index(pc, i + 1)
                break
        s.call("POST", "/reset")
        st, body = s.call("POST", "/breakpoints", {"pc": bp})
        check("add_breakpoint", st == 200 and isinstance(body, dict) and bp in body.get("breakpoints", []), body)
        st, run = s.call("POST", "/run", {"frames": 3000})
        st2, regs = s.call("GET", "/registers")
        ok, diff = regs_match(regs, parse_line(trace[l1]))
        check("breakpoint_stops_at_first_hit", st == 200 and isinstance(run, dict) and run.get("stopped") == "breakpoint"
              and run.get("pc") == bp and ok, {"run": run, "diff": diff})
        st, run = s.call("POST", "/run", {"frames": 3000})
        st2, regs = s.call("GET", "/registers")
        ok, diff = regs_match(regs, parse_line(trace[l2]))
        check("breakpoint_resume_hits_next", st == 200 and isinstance(run, dict) and run.get("stopped") == "breakpoint" and ok,
              {"run": run, "diff": diff})
        s.call("POST", "/breakpoints", {"pc": 0x0150})
        st, body = s.call("GET", "/breakpoints")
        check("list_breakpoints_sorted", st == 200 and isinstance(body, dict)
              and body.get("breakpoints") == sorted([bp, 0x0150]), body)
        st, body = s.call("DELETE", f"/breakpoints/{bp}")
        st, body2 = s.call("DELETE", "/breakpoints/0x150")
        check("delete_breakpoints", st == 200 and isinstance(body2, dict) and body2.get("breakpoints") == [], body2)

        # memory
        st, mem = s.call("GET", "/memory?addr=0x0100&len=16")
        check("read_rom", st == 200 and isinstance(mem, dict) and mem.get("addr") == 0x100
              and bytes.fromhex(mem.get("data", "")) == rom_bytes[0x100:0x110], mem)
        s.call("POST", "/memory", {"addr": 0xC000, "data": "deadbeef"})
        st, mem = s.call("GET", "/memory?addr=49152&len=4")
        check("write_read_wram", st == 200 and isinstance(mem, dict) and mem.get("data", "").lower() == "deadbeef", mem)
        st, mem = s.call("GET", "/memory?addr=0xFFFE&len=4")
        check("read_wraps", st == 200 and isinstance(mem, dict) and len(bytes.fromhex(mem.get("data", ""))) == 4
              and bytes.fromhex(mem["data"])[2:] == rom_bytes[0:2], mem)

        # disassembly: the ROM entry, then a hand-written program covering awkward encodings
        # entry point (NOP; JP nn), then the real code the entry jumps to —
        # never linearly into the cartridge header, which is data
        st, d = s.call("GET", "/disassemble?addr=0x0100&count=2")
        ok1, bad1 = disasm_ok(d, dis, lambda x: rom_bytes[x] if x < 0x8000 else 0)
        entry = rom_bytes[0x102] | (rom_bytes[0x103] << 8)
        st2, d2 = s.call("GET", f"/disassemble?addr={entry}&count=20")
        ok2, bad2 = disasm_ok(d2, dis, lambda x: rom_bytes[x] if x < 0x8000 else 0)
        check("disassemble_rom", st == 200 and st2 == 200 and ok1 and ok2, bad1 or bad2)
        prog = bytes.fromhex("18FE" "20FC" "E044" "F044" "E2" "F2" "F8FE" "E805" "22" "3A" "36A5" "EA00C0" "FAFFCF"
                             "CB7C" "CB11" "FF" "C7" "08F0FF" "C30001" "CD5000" "D9")
        s.call("POST", "/memory", {"addr": 0xC100, "data": prog.hex()})
        st, d = s.call("GET", f"/disassemble?addr={0xC100}&count=21")
        ok, bad = disasm_ok(d, dis, lambda x: prog[x - 0xC100] if 0xC100 <= x < 0xC100 + len(prog) else 0)
        check("disassemble_tricky", st == 200 and ok, bad)

        # watchpoints and breakpoints on a program in WRAM
        program = bytes.fromhex("3E42" "EA00D0" "FA00D0" "3C" "18FD")   # C000..C00A
        s.call("POST", "/memory", {"addr": 0xC000, "data": program.hex()})
        s.call("POST", "/memory", {"addr": 0xD000, "data": "00"})
        st, regs = s.call("POST", "/registers", {"pc": 0xC000, "sp": 0xDFF0, "ime": False})
        check("set_registers", st == 200 and isinstance(regs, dict) and regs.get("pc") == 0xC000 and regs.get("sp") == 0xDFF0, regs)
        st, body = s.call("POST", "/watchpoints", {"addr": 0xD000, "kind": "write"})
        st, run = s.call("POST", "/run", {"frames": 5})
        st2, regs = s.call("GET", "/registers")
        w = (run or {}).get("watch") if isinstance(run, dict) else None
        check("watch_write", isinstance(run, dict) and run.get("stopped") == "watchpoint" and isinstance(w, dict)
              and w.get("addr") == 0xD000 and w.get("kind") == "write" and w.get("value") == 0x42 and w.get("pc") == 0xC002
              and isinstance(regs, dict) and regs.get("pc") == 0xC005, {"run": run, "pc": (regs or {}).get("pc")})
        s.call("DELETE", "/watchpoints", {"addr": 0xD000, "kind": "write"})
        s.call("POST", "/watchpoints", {"addr": 0xD000, "kind": "read"})
        st, run = s.call("POST", "/run", {"frames": 5})
        st2, regs = s.call("GET", "/registers")
        w = (run or {}).get("watch") if isinstance(run, dict) else None
        check("watch_read", isinstance(run, dict) and run.get("stopped") == "watchpoint" and isinstance(w, dict)
              and w.get("kind") == "read" and w.get("value") == 0x42 and w.get("pc") == 0xC005
              and isinstance(regs, dict) and regs.get("pc") == 0xC008 and regs.get("a") == 0x42, {"run": run})
        st, body = s.call("DELETE", "/watchpoints", {"addr": 0xD000, "kind": "read"})
        check("watch_list_empty", st == 200 and isinstance(body, dict) and body.get("watchpoints") == [], body)
        s.call("POST", "/breakpoints", {"pc": 0xC009})
        st, run = s.call("POST", "/run", {"frames": 5})
        st2, r1 = s.call("GET", "/registers")
        st, run2 = s.call("POST", "/run", {"frames": 5})
        st2, r2 = s.call("GET", "/registers")
        check("breakpoint_loop_semantics", isinstance(r1, dict) and isinstance(r2, dict)
              and r1.get("pc") == 0xC009 and r1.get("a") == 0x43 and r2.get("pc") == 0xC009 and r2.get("a") == 0x44,
              {"r1": r1, "r2": r2})
        st, regs = s.call("POST", "/step", {"instructions": 4})
        check("step_ignores_breakpoints", isinstance(regs, dict) and regs.get("pc") == 0xC009 and regs.get("a") == 0x46, regs)
        s.call("DELETE", "/breakpoints/0xC009")
        st, body = s.call("POST", "/state/load", {"id": "no-such-state"})
        check("unknown_state_404", st == 404, (st, body))
    except Exception as e:
        check("doctor_server_crashed", False, repr(e))
    finally:
        s.stop()

    # ---------------- normal server: screenshots, input, save states
    try:
        s = Server(a.server, doctor=False)
        st, body = s.call("POST", "/load", {"path": str(acid)})
        st, run = s.call("POST", "/run", {"frames": 120})
        st, shot = s.call("GET", "/screenshot")
        cli = subprocess.run([a.gb, "--rom", str(acid), "--frames", "120", "--hash"], capture_output=True, text=True, errors="replace", timeout=300)
        m = re.search(r"final ([0-9a-f]{16})", cli.stdout)
        check("screenshot_matches_cli", isinstance(shot, dict) and m and shot.get("hash") == m.group(1)
              and shot.get("format") == "dmg-shades" and shot.get("frames") == 120, {"api": shot, "cli": m and m.group(1)})

        s.call("POST", "/load", {"path": str(game)})
        s.call("POST", "/run", {"frames": 60})
        st, saved = s.call("POST", "/state/save")
        sid = saved.get("id") if isinstance(saved, dict) else None
        s.call("POST", "/run", {"frames": 60})
        _, h1 = s.call("GET", "/screenshot")
        st, _ = s.call("POST", "/state/load", {"id": sid})
        s.call("POST", "/run", {"frames": 60})
        _, h2 = s.call("GET", "/screenshot")
        check("save_load_state", sid is not None and st == 200 and isinstance(h1, dict) and isinstance(h2, dict)
              and h1.get("hash") == h2.get("hash"), {"h1": h1, "h2": h2})

        s.call("POST", "/input", {"buttons": ["A", "START"]})
        s.call("POST", "/memory", {"addr": 0xFF00, "data": "10"})       # select the action-button group
        st, mem = s.call("GET", "/memory?addr=0xFF00&len=1")
        v = bytes.fromhex(mem["data"])[0] if isinstance(mem, dict) and mem.get("data") else None
        check("input_reaches_joypad", v is not None and (v & 0x0F) == 0x06, mem)

        st, body = s.call("POST", "/shutdown")
        try:
            s.proc.wait(timeout=10)
            check("shutdown", st == 200 and s.proc.returncode is not None)
        except subprocess.TimeoutExpired:
            check("shutdown", False, "process still running 10 s after /shutdown")
    except Exception as e:
        check("normal_server_crashed", False, repr(e))
    finally:
        try:
            s.stop()
        except Exception:
            pass
    return finish()


def disasm_ok(resp, dis, mem):
    if not isinstance(resp, dict) or not isinstance(resp.get("instructions"), list) or not resp["instructions"]:
        return False, "no instructions"
    bad = []
    for ins in resp["instructions"]:
        addr = ins.get("addr")
        length, want = dis.decode(mem, addr)
        want_bytes = bytes(mem(addr + k) for k in range(length)).hex()
        if (ins.get("bytes", "").lower() != want_bytes) or not same_instruction(ins.get("text", ""), want):
            bad.append({"addr": addr, "got": ins.get("text"), "bytes": ins.get("bytes"), "want_bytes": want_bytes})
    return not bad, bad[:4]


ALL_CHECKS = [
    "server_starts", "health", "409_before_load", "404_unknown_endpoint", "load", "400_malformed_json",
    "registers_after_load", "step_to_line_2", "step_to_line_8", "step_to_line_100", "step_to_line_2000",
    "step_to_line_20000", "step_to_line_50000", "reset", "profile", "add_breakpoint",
    "breakpoint_stops_at_first_hit", "breakpoint_resume_hits_next", "list_breakpoints_sorted",
    "delete_breakpoints", "read_rom", "write_read_wram", "read_wraps", "disassemble_rom", "disassemble_tricky",
    "set_registers", "watch_write", "watch_read", "watch_list_empty", "breakpoint_loop_semantics",
    "step_ignores_breakpoints", "unknown_state_404", "screenshot_matches_cli", "save_load_state",
    "input_reaches_joypad", "shutdown",
]


def finish():
    # A check that never ran (the server crashed or hung first) counts as failed,
    # so a crash can never shrink the denominator and inflate the score.
    for name in ALL_CHECKS:
        CHECKS.setdefault(name, {"ok": False, "detail": "not reached"})
    passed = sum(1 for k, c in CHECKS.items() if c["ok"] and k in ALL_CHECKS)
    total = len(ALL_CHECKS)
    print(json.dumps({"score": round(passed / total, 4), "passed": passed, "total": total, "checks": CHECKS}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
