#!/usr/bin/env python3
"""Grade an agent's checkout of the emulator against all acceptance tiers.

    grade.py CHECKOUT [--golden DIR] [--tier N] [-o results.json]

Tiers
  0  builds in release with -D warnings; clippy + fmt clean; `gb --help`
  1  Blargg + Mooneye (acceptance, emulator-only/mbc1, emulator-only/mbc5)
  2  dmg-acid2 frame hash
  3  homebrew games vs golden frame hashes (needs --golden)
  4  save-state round trip, determinism across two runs
"""
import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROMS = HERE.parent.parent / "roms"          # overridden by --roms
FROZEN_DIR = HERE.parent.parent             # overridden by --frozen-dir (holds the reference copies of frozen files)
SAMPLE_EVERY = 60   # compare one frame per second of emulated time
ALIGN_WINDOW = 2    # ± frames of slack between reference and agent frame numbering


def sh(cmd, cwd, timeout=None, env=None):
    t = time.time()
    p = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, timeout=timeout, env=env)
    return p.returncode, p.stdout, p.stderr, time.time() - t


def tier0(co):
    r = {}
    code, out, err, secs = sh(["cargo", "build", "--release", "--offline"], co, timeout=900)
    r["build"] = {"ok": code == 0, "secs": round(secs, 1), "stderr_tail": err[-2000:]}
    if code != 0:
        return r
    code, out, err, secs = sh(["cargo", "clippy", "--release", "--offline", "--all-targets"], co, timeout=900)
    r["clippy"] = {"ok": code == 0, "secs": round(secs, 1), "stderr_tail": err[-2000:]}
    code, out, err, secs = sh(["cargo", "fmt", "--check"], co, timeout=120)
    r["fmt"] = {"ok": code == 0}
    code, out, err, secs = sh([str(co / "target/release/gb"), "--help"], co, timeout=30)
    r["cli_help"] = {"ok": code == 0 and "--rom" in out}
    # Harness files must be byte-identical to the frozen originals.
    frozen = {
        "gb-core/tests/rom_suite.rs": FROZEN_DIR / "gb-core/tests/rom_suite.rs",
        "gb-cli/src/main.rs": FROZEN_DIR / "gb-cli/src/main.rs",
    }
    r["frozen_files_unchanged"] = {
        rel: (co / rel).exists() and src.exists() and (co / rel).read_bytes() == src.read_bytes()
        for rel, src in frozen.items()
    }
    return r


def gb(co):
    return str(co / "target/release/gb")


def run_blargg(co, rom):
    code, out, err, secs = sh([gb(co), "--rom", str(rom), "--frames", str(120 * 60), "--serial-stdout"], co, timeout=600)
    if code == 2:
        return "panic", secs, err[-500:]
    if "Passed" in out:
        return "pass", secs, ""
    if "Failed" in out:
        return "fail", secs, out[-500:]
    return "timeout", secs, out[-500:]


def run_blargg_mem(co, rom):
    code, out, err, secs = sh([gb(co), "--rom", str(rom), "--frames", str(120 * 60), "--blargg-mem"], co, timeout=600)
    if code == 2:
        return "panic", secs, err[-500:]
    return ("pass" if code == 20 else "fail"), secs, out[-500:]


def run_mooneye(co, rom):
    code, out, err, secs = sh([gb(co), "--rom", str(rom), "--frames", str(20 * 60), "--mooneye"], co, timeout=300)
    if code == 2:
        return "panic", secs, err[-500:]
    return ("pass" if code == 10 else "fail"), secs, out[-300:]


def tier1(co):
    results = {}
    for family, runner in (("blargg", run_blargg), ("blargg-mem", run_blargg_mem), ("mooneye", run_mooneye)):
        fam = {}
        for rom in sorted((ROMS / "test" / family).rglob("*.gb")):
            status, secs, detail = runner(co, rom)
            fam[str(rom.relative_to(ROMS / "test" / family))] = {"status": status, "secs": round(secs, 1), "detail": detail}
        n = len(fam)
        p = sum(1 for v in fam.values() if v["status"] == "pass")
        results[family] = {"passed": p, "total": n, "roms": fam}
    bm = results["blargg-mem"]["roms"]
    for sub in ("dmg_sound", "oam_bug"):
        items = {k: v for k, v in bm.items() if k.startswith(sub + "/")}
        results["blargg-mem"][sub] = {"passed": sum(1 for v in items.values() if v["status"] == "pass"), "total": len(items)}
    # Mooneye sub-scores the hidden spec cares about.
    moon = results["mooneye"]["roms"]
    def sub(prefix):
        items = {k: v for k, v in moon.items() if k.startswith(prefix)}
        return {"passed": sum(1 for v in items.values() if v["status"] == "pass"), "total": len(items)}
    results["mooneye"]["acceptance_excl_ppu"] = sub_excl(moon, "acceptance/", "acceptance/ppu/")
    results["mooneye"]["acceptance_ppu"] = sub("acceptance/ppu/")
    results["mooneye"]["mbc1"] = sub("emulator-only/mbc1/")
    results["mooneye"]["mbc5"] = sub("emulator-only/mbc5/")
    return results


def sub_excl(moon, prefix, excl):
    items = {k: v for k, v in moon.items() if k.startswith(prefix) and not k.startswith(excl)}
    return {"passed": sum(1 for v in items.values() if v["status"] == "pass"), "total": len(items)}


def tier2(co):
    rom = ROMS / "test/acid2/dmg-acid2.gb"
    expected = (ROMS / "test/acid2/expected.fnv").read_text().strip()
    code, out, err, secs = sh([gb(co), "--rom", str(rom), "--frames", "120", "--hash"], co, timeout=120)
    if code == 2:
        return {"status": "panic", "detail": err[-500:]}
    m = re.search(r"final ([0-9a-f]{16})", out)
    got = m.group(1) if m else None
    return {"status": "pass" if got == expected else "fail", "got": got, "expected": expected, "secs": round(secs, 1)}


def _run_game(co, rom, frames, script):
    """Run a game headless; return (exit_code, {frame: hash}, stderr, secs)."""
    with tempfile.TemporaryDirectory() as td:
        code, out, err, secs = sh(
            [gb(co), "--rom", str(rom), "--frames", str(frames), "--input-script", str(script),
             "--dump-every", str(SAMPLE_EVERY), "--dump-dir", td, "--hash"],
            co, timeout=900,
        )
    got = {int(k): v for k, v in re.findall(r"frame (\d+) ([0-9a-f]{16})", out)}
    return code, got, err, secs


def tier3(co, golden):
    """Per-game score in [0, 1]:

      fidelity  (0.7) fraction of the game's ROBUST sample frames whose hash
                      appears in the reference's +-window around the same frame.
                      Robust frames are the ones the reference reproduces under
                      every boot-phase / input-timing perturbation
                      (make_game_goldens.py) — i.e. what any accurate emulator
                      must render, independent of RNG divergence.
      responds  (0.3) the game reacts to the scripted input: on the samples where
                      the reference's no-input run differs from its scripted run,
                      the agent's two runs also differ on at least half of them.
      A panic in either run scores 0. A game "passes" at score >= 0.8.
    """
    games = {}
    for rom in sorted((ROMS / "games").glob("*.gb")):
        name = rom.stem
        gold = golden / f"{name}.fnv"
        meta_p = golden / f"{name}.robust.json"
        script = golden / f"{name}.input"
        if not (gold.exists() and script.exists() and meta_p.exists()):
            games[name] = {"status": "no-golden", "score": 0.0}
            continue
        expected = {int(k): v for k, v in (line.split() for line in gold.read_text().split("\n") if line.strip())}
        meta = json.loads(meta_p.read_text())
        frames, window = meta["frames"], meta.get("window", ALIGN_WINDOW)
        code, got, err, secs = _run_game(co, rom, frames, script)
        if code == 2:
            games[name] = {"status": "panic", "score": 0.0, "detail": err[-500:]}
            continue
        with tempfile.TemporaryDirectory() as td:
            empty = Path(td) / "empty.input"
            empty.write_text("")
            code2, got_noinput, err2, secs2 = _run_game(co, rom, frames, empty)
        if code2 == 2:
            games[name] = {"status": "panic", "score": 0.0, "detail": err2[-500:]}
            continue
        robust = meta["robust"]
        matched = sum(1 for n in robust
                      if got.get(n) in {expected.get(n + d) for d in range(-window, window + 1)})
        fidelity = matched / len(robust) if robust else 0.0
        sens = meta.get("input_sensitive", [])
        differs = sum(1 for n in sens if n in got and got.get(n) != got_noinput.get(n))
        responds = 1.0 if (not sens or differs >= len(sens) / 2) else differs / max(1, len(sens) / 2)
        score = round(0.7 * fidelity + 0.3 * responds, 4)
        games[name] = {"status": "pass" if score >= 0.8 else "fail", "score": score,
                       "fidelity": round(fidelity, 3), "robust_matched": matched, "robust_total": len(robust),
                       "responds": round(responds, 3), "secs": round(secs + secs2, 1)}
    passed = sum(1 for g in games.values() if g["status"] == "pass")
    mean = sum(g["score"] for g in games.values()) / len(games) if games else 0.0
    return {"passed": passed, "total": len(games), "mean_score": round(mean, 4), "games": games}


def tier4(co):
    r = {}
    roms = sorted((ROMS / "games").glob("*.gb"))
    if not roms:
        return {"status": "no-games"}
    rom = roms[0]
    # Determinism: two runs, identical hash.
    hashes = []
    for _ in range(2):
        code, out, err, secs = sh([gb(co), "--rom", str(rom), "--frames", "1800", "--hash"], co, timeout=300)
        hashes.append(re.search(r"final ([0-9a-f]{16})", out).group(1) if code == 0 else f"exit{code}")
    r["determinism"] = {"ok": hashes[0] == hashes[1] and not hashes[0].startswith("exit"), "hashes": hashes}
    # Save state: run 600, save; run 600 more → H1. Fresh: load state, run 600 → H2. H1 == H2.
    with tempfile.TemporaryDirectory() as td:
        st = os.path.join(td, "s.state")
        sh([gb(co), "--rom", str(rom), "--frames", "600", "--save-state", st], co, timeout=300)
        c1, o1, _, _ = sh([gb(co), "--rom", str(rom), "--frames", "1200", "--hash"], co, timeout=300)
        c2, o2, e2, _ = sh([gb(co), "--rom", str(rom), "--frames", "600", "--load-state", st, "--hash"], co, timeout=300)
        h1 = re.search(r"final ([0-9a-f]{16})", o1)
        h2 = re.search(r"final ([0-9a-f]{16})", o2)
        r["save_state_round_trip"] = {
            "ok": bool(h1 and h2 and h1.group(1) == h2.group(1)),
            "detail": e2[-300:] if c2 != 0 else "",
        }
    return r


def git_log(co):
    code, out, _, _ = sh(["git", "log", "--format=%H%x09%at%x09%s", "--reverse"], co, timeout=60)
    if code != 0:
        return []
    return [dict(zip(("sha", "unix_time", "subject"), line.split("\t", 2))) for line in out.splitlines() if line]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("checkout", type=Path)
    ap.add_argument("--golden", type=Path, default=HERE.parent / "golden")
    ap.add_argument("--tier", type=int, help="run only this tier (0-4)")
    ap.add_argument("--roms", type=Path, default=None, help="roms/ directory (default: repo's own roms/)")
    ap.add_argument("--frozen-dir", type=Path, default=None, help="directory holding reference copies of the frozen files")
    ap.add_argument("-o", "--out", type=Path, default=Path("results.json"))
    a = ap.parse_args()
    co = a.checkout.resolve()
    global ROMS, FROZEN_DIR
    if a.roms:
        ROMS = a.roms.resolve()
    if a.frozen_dir:
        FROZEN_DIR = a.frozen_dir.resolve()
    a.golden = a.golden.resolve()

    report = {"checkout": str(co), "graded_at": int(time.time()), "git_log": git_log(co)}
    tiers = [a.tier] if a.tier is not None else [0, 1, 2, 3, 4]
    if 0 in tiers:
        report["tier0"] = tier0(co)
        if not report["tier0"]["build"]["ok"]:
            print("build failed; stopping", file=sys.stderr)
            a.out.write_text(json.dumps(report, indent=2))
            return 1
    if 1 in tiers:
        report["tier1"] = tier1(co)
    if 2 in tiers:
        report["tier2"] = tier2(co)
    if 3 in tiers:
        report["tier3"] = tier3(co, a.golden)
    if 4 in tiers:
        report["tier4"] = tier4(co)

    a.out.write_text(json.dumps(report, indent=2))
    # One-line summary for the console.
    t1 = report.get("tier1", {})
    print(
        "blargg {}/{} · mooneye {}/{} · acid2 {} · games {}/{} · det {} · state {}".format(
            t1.get("blargg", {}).get("passed", "-"), t1.get("blargg", {}).get("total", "-"),
            t1.get("mooneye", {}).get("passed", "-"), t1.get("mooneye", {}).get("total", "-"),
            report.get("tier2", {}).get("status", "-"),
            report.get("tier3", {}).get("passed", "-"), report.get("tier3", {}).get("total", "-"),
            report.get("tier4", {}).get("determinism", {}).get("ok", "-"),
            report.get("tier4", {}).get("save_state_round_trip", {}).get("ok", "-"),
        )
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
