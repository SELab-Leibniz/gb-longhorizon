#!/usr/bin/env python3
"""Boot every test game on an emulator and save screenshots for manual review.

    screenshots.py CHECKOUT --roms ROMS --golden GOLDEN --out DIR [--every 300]
    screenshots.py --reference RUNNER --roms ROMS --golden GOLDEN [--every 300]

Agent mode (used by the Harbor verifier, writes to /logs/verifier/screenshots):
  for dmg-acid2 and each game in roms/games (plus, with --golden-cgb,
  cgb-acid2 and the CGB games in CGB mode), run the agent's `gb` CLI with the
  game's input script, dump a frame every N frames, and write
    DIR/<game>/agent_NNNNNN.png      the agent's frame, 3x (DMG palette / CGB colour)
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
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from pngio import read_png  # noqa: E402

W, H, SCALE = 160, 144, 3
# Classic DMG green, shade 0 (lightest) .. 3 (darkest)
DMG_RGB = [(0xE0, 0xF8, 0xD0), (0x88, 0xC0, 0x70), (0x34, 0x68, 0x56), (0x08, 0x18, 0x20)]
SEPARATOR = (0xFF, 0xFF, 0xFF)


def dump_to_rgb(path: Path) -> list:
    """A 160x144 frame dump from gb-cli / the reference runner -> list of (r, g, b).

    PGM (DMG shades as grey levels) is shown in the DMG palette; PPM (CGB
    colour, 8-bit channels) as is."""
    data = path.read_bytes()
    if path.suffix == ".ppm":
        body = data[-W * H * 3:]
        return [tuple(body[3 * i:3 * i + 3]) for i in range(W * H)]
    body = data[-W * H:]
    return [DMG_RGB[3 - min(3, v * 4 // 256)] for v in body]


def png(rows, width, height) -> bytes:
    """Truecolour PNG from rows of (r, g, b) tuples."""
    raw = b"".join(b"\x00" + bytes(c for px in row for c in px) for row in rows)
    def chunk(t, d):
        return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)
    return (b"\x89PNG\r\n\x1a\n"
            + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw, 9))
            + chunk(b"IEND", b""))


def scaled_rows(pixels: list):
    for y in range(H):
        row = [px for px in pixels[y * W:(y + 1) * W] for _ in range(SCALE)]
        for _ in range(SCALE):
            yield row


def frame_png(pixels: list) -> bytes:
    return png(list(scaled_rows(pixels)), W * SCALE, H * SCALE)


def compare_png(left: list, right: list | None) -> bytes:
    gap = 8
    lrows = list(scaled_rows(left))
    rrows = list(scaled_rows(right)) if right else [[SEPARATOR] * (W * SCALE)] * (H * SCALE)
    rows = [l + [SEPARATOR] * gap + r for l, r in zip(lrows, rrows)]
    return png(rows, W * SCALE * 2 + gap, H * SCALE)


def read_reference(path: Path) -> list | None:
    """A reference screen written by frame_png (any earlier paletted version too)."""
    try:
        w, h, ch, rows = read_png(path.read_bytes())
        return [tuple(rows[y * SCALE][x * SCALE][:3]) if ch >= 3 else (rows[y * SCALE][x * SCALE][0],) * 3
                for y in range(H) for x in range(W)]
    except Exception:
        return None


def script_frames(script: Path) -> int:
    nums = [int(l.split()[0]) for l in script.read_text().splitlines() if l.strip() and not l.lstrip().startswith("#")]
    return (max(nums) if nums else 0) + 600


def targets(a):
    """(name, rom, input script, frames, model, reference-screens dir)."""
    roms, golden = a.roms, a.golden
    acid = roms / "test" / "acid2" / "dmg-acid2.gb"
    if acid.exists():
        yield "dmg-acid2", acid, None, 120, "dmg", golden / "screens" / "dmg-acid2"
    for rom in sorted((roms / "games").glob("*.gb")):
        script = golden / f"{rom.stem}.input"
        yield (rom.stem, rom, (script if script.exists() else None),
               (script_frames(script) if script.exists() else 1800), "dmg", golden / "screens" / rom.stem)
    if a.golden_cgb:
        cr1 = roms
        acid = cr1 / "test" / "cgb-acid2" / "cgb-acid2.gbc"
        if acid.exists():
            yield "cgb-acid2", acid, None, 120, "cgb", a.golden_cgb / "screens" / "cgb-acid2"
        for rom in sorted((cr1 / "games-cgb").glob("*.gbc")):
            script = a.golden_cgb / f"{rom.stem}.input"
            yield (f"cgb-{rom.stem}", rom, (script if script.exists() else None),
                   (script_frames(script) if script.exists() else 1800), "cgb", a.golden_cgb / "screens" / rom.stem)


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
    if not gb.exists():
        # The graded commit did not build: nothing can boot. Never fall back to
        # some other binary — that would show screenshots of code not graded.
        for name, *_ in targets(a):
            summary[name] = {"status": "no_binary", "loaded": False, "exit_code": None,
                             "frames_captured": [], "stderr_tail": "gb binary missing (build failed)"}
        (out / "summary.json").write_text(json.dumps(summary, indent=2))
        (out / "index.html").write_text("<!doctype html><meta charset=utf-8><title>Emulator screenshots</title>"
                                        "<h1>No screenshots: the graded commit did not build.</h1>")
        print(f"screenshots: build missing -> {out}")
        return
    rows_html = []
    for name, rom, script, frames, model, ref_dir in targets(a):
        every = 120 if name.endswith("acid2") else a.every
        gdir = out / name
        gdir.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory() as td:
            cmd = [str(gb), "--rom", str(rom), "--frames", str(frames), "--dump-every", str(every), "--dump-dir", td,
                   "--model", model]
            if script:
                cmd += ["--input-script", str(script)]
            code, so, se = run_and_dump(cmd, cwd=co)
            dumps = sorted(list(Path(td).glob("frame_*.pgm")) + list(Path(td).glob("frame_*.ppm")))
            flat = True
            shots = []
            for d in dumps:
                n = int(re.search(r"(\d+)", d.name).group(1))
                pixels = dump_to_rgb(d)
                if len(set(pixels)) > 1:
                    flat = False
                (gdir / f"agent_{n:06d}.png").write_bytes(frame_png(pixels))
                ref_p = ref_dir / f"ref_{n:06d}.png"
                ref = read_reference(ref_p) if ref_p.exists() else None
                (gdir / f"compare_{n:06d}.png").write_bytes(compare_png(pixels, ref))
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
    for name, rom, script, frames, model, sdir in targets(a):
        every = 120 if name.endswith("acid2") else a.every
        sdir.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory() as td:
            subprocess.run([runner, str(rom), str(frames), str(script) if script else "/dev/null", td,
                            "--dump-every", str(every), "--model", model], check=True, capture_output=True)
            for d in sorted(list(Path(td).glob("frame_*.pgm")) + list(Path(td).glob("frame_*.ppm"))):
                n = int(re.search(r"(\d+)", d.name).group(1))
                (sdir / f"ref_{n:06d}.png").write_bytes(frame_png(dump_to_rgb(d)))
        print(f"reference screens: {name}: {len(list(sdir.glob('ref_*.png')))}")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("checkout", type=Path, nargs="?")
    ap.add_argument("--roms", type=Path, required=True)
    ap.add_argument("--golden", type=Path, required=True)
    ap.add_argument("--out", type=Path)
    ap.add_argument("--every", type=int, default=300, help="capture a frame every N frames (default 300 = 5 s)")
    ap.add_argument("--reference", help="path to sameboy_runner: render reference screens instead")
    ap.add_argument("--golden-cgb", type=Path, help="golden data for the CGB games (input scripts, screens/)")
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
