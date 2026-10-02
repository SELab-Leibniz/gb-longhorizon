#!/usr/bin/env python3
"""Boot every test game on an emulator and save screenshots for manual review.

    screenshots.py CHECKOUT --roms ROMS --golden GOLDEN --out DIR [--every 300]
    screenshots.py --reference RUNNER --roms ROMS --golden GOLDEN [--every 300]

Agent mode (used by the Harbor verifier, writes to /logs/verifier/screenshots):
  for dmg-acid2 and each game in roms/games, run the agent's `gb` CLI with the
  game's input script, dump a frame every N frames, and write
    DIR/<game>/agent_NNNNNN.png      the agent's frame, 3x, DMG palette
    DIR/<game>/compare_NNNNNN.png    agent | reference side by side
    DIR/summary.json                 per game: loaded? panicked? blank? frames
    DIR/index.html                   one page with every comparison, for a human
  "loaded" means the CLI accepted the ROM and ran the whole script without a
  panic; "blank" means every captured frame is a single flat shade.

Reference mode (run on the host when goldens change): renders the same frames
with the SameBoy reference runner into GOLDEN/screens/<game>/ref_NNNNNN.png,
which the verifier ships alongside the agent shots.

No third-party dependencies (PNG written with zlib).
"""
from __future__ import annotations

import argparse
import html
import json
import re
import struct
import subprocess
import tempfile
import zlib
from pathlib import Path

W, H, SCALE = 160, 144, 3
# Classic DMG green, shade 0 (lightest) .. 3 (darkest)
PALETTE = bytes([0xE0, 0xF8, 0xD0, 0x88, 0xC0, 0x70, 0x34, 0x68, 0x56, 0x08, 0x18, 0x20])


def pgm_to_shades(data: bytes) -> bytes:
    """Read a 160x144 binary PGM (as written by gb-cli / the reference) to shades 0..3."""
    body = data[-W * H:]
    return bytes(3 - min(3, v * 4 // 256) for v in body)


def png(shade_rows, width, height) -> bytes:
    """Paletted 2-colour-depth PNG from rows of shade values (0..3)."""
    raw = b"".join(b"\x00" + bytes(row) for row in shade_rows)
    def chunk(t, d):
        return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)
    return (b"\x89PNG\r\n\x1a\n"
            + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 3, 0, 0, 0))
            + chunk(b"PLTE", PALETTE + b"\xff\xff\xff")      # index 4 = white separator
            + chunk(b"IDAT", zlib.compress(raw, 9))
            + chunk(b"IEND", b""))


def scaled_rows(shades: bytes):
    for y in range(H):
        row = [s for s in shades[y * W:(y + 1) * W] for _ in range(SCALE)]
        for _ in range(SCALE):
            yield row


def frame_png(shades: bytes) -> bytes:
    return png(list(scaled_rows(shades)), W * SCALE, H * SCALE)


def compare_png(left: bytes, right: bytes | None) -> bytes:
    gap = 8
    lrows = list(scaled_rows(left))
    rrows = list(scaled_rows(right)) if right else [[4] * (W * SCALE)] * (H * SCALE)
    rows = [l + [4] * gap + r for l, r in zip(lrows, rrows)]
    return png(rows, W * SCALE * 2 + gap, H * SCALE)


def png_to_shades(data: bytes) -> bytes | None:
    """Inverse of frame_png for our own files (reads IDAT, undoes the 3x scale)."""
    try:
        pos, idat = 8, b""
        while pos < len(data):
            n = struct.unpack(">I", data[pos:pos + 4])[0]
            t = data[pos + 4:pos + 8]
            if t == b"IDAT":
                idat += data[pos + 8:pos + 8 + n]
            pos += 12 + n
        raw = zlib.decompress(idat)
        stride = W * SCALE + 1
        out = bytearray()
        for y in range(H):
            row = raw[(y * SCALE) * stride + 1:(y * SCALE) * stride + stride]
            out += bytes(row[::SCALE])
        return bytes(out)
    except Exception:
        return None


def script_frames(script: Path) -> int:
    nums = [int(l.split()[0]) for l in script.read_text().splitlines() if l.strip() and not l.lstrip().startswith("#")]
    return (max(nums) if nums else 0) + 600


def targets(roms: Path, golden: Path):
    acid = roms / "test" / "acid2" / "dmg-acid2.gb"
    if acid.exists():
        yield "dmg-acid2", acid, None, 120
    for rom in sorted((roms / "games").glob("*.gb")):
        script = golden / f"{rom.stem}.input"
        yield rom.stem, rom, (script if script.exists() else None), (script_frames(script) if script.exists() else 1800)


def run_and_dump(cmd, cwd=None, timeout=900):
    try:
        p = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, timeout=timeout)
        return p.returncode, p.stdout, p.stderr
    except subprocess.TimeoutExpired:
        return 124, "", "timeout"


def agent_mode(a):
    co = a.checkout.resolve()
    gb = co / "target" / "release" / "gb"
    out = a.out
    out.mkdir(parents=True, exist_ok=True)
    summary = {}
    rows_html = []
    for name, rom, script, frames in targets(a.roms, a.golden):
        every = 120 if name == "dmg-acid2" else a.every
        gdir = out / name
        gdir.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory() as td:
            cmd = [str(gb), "--rom", str(rom), "--frames", str(frames), "--dump-every", str(every), "--dump-dir", td]
            if script:
                cmd += ["--input-script", str(script)]
            code, so, se = run_and_dump(cmd, cwd=co)
            dumps = sorted(Path(td).glob("frame_*.pgm"))
            flat = True
            shots = []
            for d in dumps:
                n = int(re.search(r"(\d+)", d.name).group(1))
                shades = pgm_to_shades(d.read_bytes())
                if len(set(shades)) > 1:
                    flat = False
                (gdir / f"agent_{n:06d}.png").write_bytes(frame_png(shades))
                ref_p = a.golden / "screens" / name / f"ref_{n:06d}.png"
                ref = png_to_shades(ref_p.read_bytes()) if ref_p.exists() else None
                (gdir / f"compare_{n:06d}.png").write_bytes(compare_png(shades, ref))
                shots.append(n)
        status = ("load_error" if code == 1 else "panic" if code == 2 else "timeout" if code == 124
                  else "no_frames" if not shots else "blank" if flat else "ok")
        summary[name] = {"status": status, "loaded": code in (0,) and bool(shots), "exit_code": code,
                         "frames_captured": shots, "stderr_tail": se[-400:] if code else ""}
        cells = "".join(
            f'<figure><img src="{html.escape(name)}/compare_{n:06d}.png" loading="lazy"><figcaption>frame {n}</figcaption></figure>'
            for n in shots)
        rows_html.append(f'<section><h2>{html.escape(name)} — <span class="{status}">{status}</span></h2>'
                         f'<p class="hint">left: agent emulator · right: SameBoy reference</p>'
                         f'<div class="strip">{cells or "<em>no frames captured</em>"}</div>'
                         + (f'<pre>{html.escape(se[-400:])}</pre>' if code else "") + "</section>")
    (out / "summary.json").write_text(json.dumps(summary, indent=2))
    ok = sum(1 for v in summary.values() if v["status"] == "ok")
    (out / "index.html").write_text(f"""<!doctype html><meta charset="utf-8"><title>Emulator screenshots</title>
<style>body{{font:14px system-ui,sans-serif;margin:24px;background:#fafafa;color:#222}}
h2{{font-size:16px;margin:28px 0 4px}}.hint{{color:#777;margin:0 0 8px}}
.strip{{display:flex;gap:12px;overflow-x:auto;padding-bottom:8px}}figure{{margin:0}}
img{{height:216px;image-rendering:pixelated;border:1px solid #ccc;background:#fff}}figcaption{{color:#777;font-size:12px}}
.ok{{color:#18794e}}.blank,.no_frames,.timeout{{color:#b35900}}.panic,.load_error{{color:#c0261d}}
pre{{background:#fff;border:1px solid #eee;padding:8px;white-space:pre-wrap}}</style>
<h1>Emulator screenshots — {ok}/{len(summary)} booted and rendered</h1>
{''.join(rows_html)}
""")
    print(f"screenshots: {ok}/{len(summary)} ok -> {out}")


def reference_mode(a):
    runner = a.reference
    for name, rom, script, frames in targets(a.roms, a.golden):
        every = 120 if name == "dmg-acid2" else a.every
        sdir = a.golden / "screens" / name
        sdir.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory() as td:
            subprocess.run([runner, str(rom), str(frames), str(script) if script else "/dev/null", td,
                            "--dump-every", str(every)], check=True, capture_output=True)
            for d in sorted(Path(td).glob("frame_*.pgm")):
                n = int(re.search(r"(\d+)", d.name).group(1))
                (sdir / f"ref_{n:06d}.png").write_bytes(frame_png(pgm_to_shades(d.read_bytes())))
        print(f"reference screens: {name}: {len(list(sdir.glob('ref_*.png')))}")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("checkout", type=Path, nargs="?")
    ap.add_argument("--roms", type=Path, required=True)
    ap.add_argument("--golden", type=Path, required=True)
    ap.add_argument("--out", type=Path)
    ap.add_argument("--every", type=int, default=300, help="capture a frame every N frames (default 300 = 5 s)")
    ap.add_argument("--reference", help="path to sameboy_runner: render reference screens instead")
    a = ap.parse_args()
    a.roms, a.golden = a.roms.resolve(), a.golden.resolve()
    if a.reference:
        reference_mode(a)
    else:
        if not a.checkout or not a.out:
            ap.error("agent mode needs CHECKOUT and --out")
        agent_mode(a)


if __name__ == "__main__":
    main()
