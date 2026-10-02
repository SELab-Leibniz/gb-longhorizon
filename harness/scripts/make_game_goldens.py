#!/usr/bin/env python3
"""Build timing-robust golden data for the Tier-3 game checks.

    make_game_goldens.py --runner harness/ref/bin/sameboy_runner --roms roms/games \
                         --golden harness/golden

For each game with an input script (<golden>/<game>.input) this runs the
reference emulator several times:

  * a base run                                  -> <game>.fnv   (hash of every frame)
  * perturbed runs: boot handover delayed by a few dozen M-cycles (shifts the
    DIV/PPU phase at PC=0100) and/or inputs landing one frame early/late —
    the kind of sub-frame difference any two correct emulators have
  * a run with no input at all                   (to see where input matters)

A sampled frame (every 60th) is ROBUST if every perturbed run produces the
base run's picture within +-WINDOW frames. Only robust frames are graded:
they are what any accurate emulator must reproduce, whereas frames after an
RNG divergence differ even between two runs of the reference itself.

Writes <game>.robust.json:
  {"frames": total, "sample_every": 60, "window": 2,
   "robust": [n, ...],                 # sampled frame numbers to grade
   "input_sensitive": [n, ...],        # samples where the no-input run differs
   "variants": [...], "reference_commit": "..."}
"""
import argparse
import json
import re
import subprocess
import tempfile
from pathlib import Path

SAMPLE_EVERY = 60
WINDOW = 2
# (boot delay in M-cycles, input shift in frames)
VARIANTS = [(0, 1), (0, -1), (37, 0), (101, 0), (173, 1), (59, -1)]


def run(runner, rom, script, frames, delay=0):
    with tempfile.TemporaryDirectory() as td:
        subprocess.run([runner, str(rom), str(frames), str(script), td, "--dump-every", "0", "--boot-delay", str(delay)],
                       check=True, capture_output=True)
        return {int(a): h for a, h in (line.split() for line in open(f"{td}/hashes.txt"))}


def shifted(script: Path, d: int, tmp: Path) -> Path:
    out = []
    for line in script.read_text().splitlines(keepends=True):
        m = re.match(r"\s*(\d+)(.*)", line, re.S)
        if m and not line.lstrip().startswith("#"):
            out.append(f"{max(1, int(m.group(1)) + d)}{m.group(2)}")
        else:
            out.append(line)
    p = tmp / f"{script.stem}.shift{d}.input"
    p.write_text("".join(out))
    return p


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--runner", required=True)
    ap.add_argument("--roms", type=Path, required=True)
    ap.add_argument("--golden", type=Path, required=True)
    ap.add_argument("--only", default="")
    a = ap.parse_args()
    commit = (a.golden / "SAMEBOY_COMMIT").read_text().strip() if (a.golden / "SAMEBOY_COMMIT").exists() else ""
    tmp = Path(tempfile.mkdtemp())
    empty = tmp / "empty.input"
    empty.write_text("")
    print(f"{'game':14} {'samples':>7} {'robust':>7} {'input-sensitive':>16}")
    for script in sorted(a.golden.glob("*.input")):
        game = script.stem
        if a.only and game not in a.only.split(","):
            continue
        rom = a.roms / f"{game}.gb"
        last = max(int(l.split()[0]) for l in script.read_text().splitlines() if l.strip() and not l.lstrip().startswith("#"))
        frames = last + 600
        base = run(a.runner, rom, script, frames)
        (a.golden / f"{game}.fnv").write_text("".join(f"{n} {h}\n" for n, h in sorted(base.items())))
        samples = list(range(SAMPLE_EVERY, frames + 1, SAMPLE_EVERY))

        def in_window(other, n):
            return other.get(n) in {base.get(n + d) for d in range(-WINDOW, WINDOW + 1)}

        robust = set(samples)
        for delay, shift in VARIANTS:
            sc = shifted(script, shift, tmp) if shift else script
            v = run(a.runner, rom, sc, frames, delay)
            robust &= {n for n in samples if in_window(v, n)}
        noinput = run(a.runner, rom, empty, frames)
        sensitive = [n for n in samples if not in_window(noinput, n)]
        rob = sorted(robust)
        (a.golden / f"{game}.robust.json").write_text(json.dumps({
            "frames": frames, "sample_every": SAMPLE_EVERY, "window": WINDOW,
            "robust": rob, "input_sensitive": sensitive,
            "variants": [{"boot_delay": d, "input_shift": s} for d, s in VARIANTS],
            "reference_commit": commit,
        }, indent=1))
        print(f"{game:14} {len(samples):7d} {len(rob):7d} {len(sensitive):16d}")


if __name__ == "__main__":
    main()
