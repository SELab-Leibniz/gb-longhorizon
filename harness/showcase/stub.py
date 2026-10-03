#!/usr/bin/env python3
"""Reduce chosen Rust functions to their signature + `todo!(message)`.

    stub.py REPO_DIR

Doc comments and attributes stay; the body is replaced; functions with
parameters other than self get #[allow(unused_variables)] so the stub builds
under -D warnings. Every (file, function) must match exactly the expected
number of definitions, so a renamed function fails loudly.
"""
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from rsfns import functions  # noqa: E402

GEP = "GEP 1"
STUBS = {
    "gb-core/src/cpu/opcodes.rs": {
        "dispatch": (1, "execute one unprefixed opcode, M-cycle by M-cycle through the bus (R-CORE-1, DECISIONS D1)"),
        "execute_cb": (1, "execute one CB-prefixed opcode (rotates, shifts, BIT/RES/SET) and return its cycles"),
    },
    "gb-core/src/ppu.rs": {
        "tick": (1, "advance the PPU by `cycles` dots: mode 2/3/0/1 state machine, LY/LYC, STAT and VBlank interrupts (R-CORE-4)"),
        "render_scanline": (1, "draw the current line into the framebuffer(s): background, window, sprites (R-CORE-4, R-CGB-1)"),
        "bg_pixel": (1, "background colour/priority for pixel x on the current line"),
        "window_pixel": (1, "window colour/priority for pixel x on the current line"),
        "render_sprites": (1, "draw up to 10 sprites on line `ly` with DMG/CGB priority rules"),
        "save_state": (1, "append the PPU's state to `out` (Emulator::save_state, R-CORE-6)"),
        "load_state": (1, "restore the PPU's state written by save_state (R-CORE-6)"),
    },
    "gb-core/src/mmu.rs": {
        "start_oam_dma": (1, "start an OAM DMA from page `value` (write to $FF46)"),
        "tick_dma": (1, "advance OAM DMA and H-blank HDMA by `real` master-clock cycles"),
        "hdma_write": (1, "CGB HDMA registers $FF51-$FF55: general-purpose and H-blank transfers (R-CGB-1)"),
        "hdma_transfer_block": (1, "copy one 16-byte HDMA block from source to VRAM"),
        "save_state": (1, "append the bus state (WRAM/HRAM, banks, DMA, I/O) and its peripherals' state to `out` (R-CORE-6)"),
        "load_state": (1, "restore the bus state written by save_state (R-CORE-6)"),
    },
    "gb-core/src/timer.rs": {
        "write": (1, "DIV/TIMA/TMA/TAC writes, including the falling-edge effects of DIV and TAC writes (R-CORE-2)"),
        "tick_t": (1, "advance the internal counter by one T-cycle; return true when TIMA overflows"),
        "increment_tima": (1, "increment TIMA, handling overflow and the delayed TMA reload"),
        "save_state": (1, "append the timer's state to `out` (R-CORE-6)"),
        "load_state": (1, "restore the timer's state written by save_state (R-CORE-6)"),
    },
    "gb-core/src/cpu/mod.rs": {
        "save_state": (1, "append the CPU's registers and flags to `out` (R-CORE-6)"),
        "load_state": (1, "restore the CPU's state written by save_state (R-CORE-6)"),
    },
    "gb-tools/src/disasm.rs": {
        "disassemble": (1, f"decode the instruction at `pc` into (length, text) in the {GEP} Appendix B syntax"),
    },
    "gb-tools/src/bin/gb-server.rs": {
        "core_step": (1, "execute one instruction for /run and /step, recording breakpoint and watchpoint hits (Appendix B semantics)"),
        "handle_run": (1, "POST /run: run up to n frames, stopping at breakpoints (before) and watchpoints (after) — Appendix B"),
    },
    "gb-wasm/src/lib.rs": {
        "gb_load": (1, "load a ROM from linear memory, model 0 = DMG, 1 = CGB; 0 on success (Appendix C)"),
        "gb_run_frames": (1, "run n frames with the held buttons; return total frames since load (Appendix C)"),
        "refresh_frame": (1, "copy the emulator's current frame (shades or RGB555 LE) into the exported buffer (Appendix C)"),
    },
    "gb-web/src/main.rs": {
        "api_upload": (1, "POST /api/games: multipart or raw upload, validation and the product owner's upload rules (Appendix D.4)"),
        "api_list": (1, "GET /api/games: filters, search, sort, order, paging (Appendix D.3)"),
        "api_screenshot": (1, "GET /api/games/{id}/screenshot.png: parameters, caching, errors (Appendix D.6)"),
        "render_screenshot": (1, "run the game headless for `frames` frames and encode the frame as PNG (Appendix D.6)"),
    },
}


def params_besides_self(sig):
    inner = sig[sig.index("(") + 1: sig.rindex(")")] if "(" in sig else ""
    parts = [p.strip() for p in re.split(r",(?![^<]*>)", inner) if p.strip()]
    return [p for p in parts if not re.match(r"^(&\s*(mut\s+)?)?(mut\s+)?self$", p) and not p.startswith("&'")]


def stub_file(path, wanted):
    text = path.read_text()
    found = {}
    for name, start, op, cl in functions(text):
        if name in wanted:
            found.setdefault(name, []).append((start, op, cl))
    for name, (count, _) in wanted.items():
        got = len(found.get(name, []))
        if got != count:
            raise SystemExit(f"{path}: expected {count} fn {name}, found {got}")
    edits = []
    for name, spans in found.items():
        msg = wanted[name][1].replace('"', '\\"')
        for start, op, cl in spans:
            line_start = text.rfind("\n", 0, start) + 1
            indent = re.match(r"\s*", text[line_start:start]).group(0)
            sig = text[start:op]
            attr = f"#[allow(unused_variables)]\n{indent}" if params_besides_self(sig) else ""
            body = "{\n" + indent + "    todo!(\"" + msg + "\")\n" + indent + "}"
            edits.append((line_start, start, op, cl, indent, attr, body))
    for line_start, start, op, cl, indent, attr, body in sorted(edits, reverse=True):
        text = text[:op] + body + text[cl + 1:]
        if attr:
            text = text[:line_start] + indent + attr + text[line_start + len(indent):]
    path.write_text(text)
    return sum(len(v) for v in found.values())


if __name__ == "__main__":
    root = Path(sys.argv[1])
    total = 0
    for rel, wanted in STUBS.items():
        n = stub_file(root / rel, wanted)
        total += n
        print(f"{rel}: {n} functions stubbed")
    print("total", total)
