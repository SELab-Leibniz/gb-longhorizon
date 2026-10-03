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
import signal
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


HUNG = 124          # exit code sh() reports when it had to kill a command


def sh(cmd, cwd, timeout=None, env=None):
    """Run cmd; on timeout kill its whole process group and return exit code HUNG.

    A hung emulator (say, an infinite loop in step_frame) must cost one
    timeout, never the grading run: an uncaught TimeoutExpired here would
    leave no results.json at all and score the trial 0."""
    t = time.time()
    try:
        p = subprocess.Popen(cmd, cwd=cwd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, env=env,
                             start_new_session=True)
    except OSError as e:          # e.g. the binary was never built
        return 127, "", f"[grader] cannot run {cmd[0]}: {e}", 0.0
    try:
        out, err = p.communicate(timeout=timeout)
        return p.returncode, out, err, time.time() - t
    except subprocess.TimeoutExpired:
        try:
            os.killpg(p.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        out, err = p.communicate()
        return HUNG, out or "", (err or "") + f"\n[grader] killed after {timeout} s", time.time() - t


def run_roms(co, roms, root, runner, max_hung=3):
    """Run runner(co, rom) -> (status, secs, detail) over roms; after max_hung
    consecutive wall-clock timeouts the emulator is taken to be hanging and the
    rest of the family is skipped (counted as failures)."""
    fam, streak = {}, 0
    for rom in roms:
        rel = str(rom.relative_to(root))
        if streak >= max_hung:
            fam[rel] = {"status": "skipped", "secs": 0.0, "detail": f"skipped after {max_hung} consecutive hangs"}
            continue
        status, secs, detail = runner(co, rom)
        streak = streak + 1 if status == "hung" else 0
        fam[rel] = {"status": status, "secs": round(secs, 1), "detail": detail}
    return fam


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
    # the frozen CLI prints usage to stderr and exits 1 for --help; any non-panic
    # exit that shows the usage text counts
    r["cli_help"] = {"ok": code in (0, 1) and "--rom" in (out + err)}
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
    code, out, err, secs = sh([gb(co), "--rom", str(rom), "--frames", str(120 * 60), "--serial-stdout"], co, timeout=300)
    if code == HUNG:
        return "hung", secs, err[-300:]
    if code == 2:
        return "panic", secs, err[-500:]
    if "Passed" in out:
        return "pass", secs, ""
    if "Failed" in out:
        return "fail", secs, out[-500:]
    return "timeout", secs, out[-500:]


def run_blargg_mem(co, rom, model="dmg"):
    code, out, err, secs = sh([gb(co), "--rom", str(rom), "--frames", str(120 * 60), "--blargg-mem", "--model", model], co, timeout=300)
    if code == HUNG:
        return "hung", secs, err[-300:]
    if code == 2:
        return "panic", secs, err[-500:]
    return ("pass" if code == 20 else "fail"), secs, out[-500:]


def run_mooneye(co, rom, model="dmg"):
    code, out, err, secs = sh([gb(co), "--rom", str(rom), "--frames", str(20 * 60), "--mooneye", "--model", model], co, timeout=120)
    if code == HUNG:
        return "hung", secs, err[-300:]
    if code == 2:
        return "panic", secs, err[-500:]
    return ("pass" if code == 10 else "fail"), secs, out[-300:]


def tier1(co):
    results = {}
    for family, runner in (("blargg", run_blargg), ("blargg-mem", run_blargg_mem), ("mooneye", run_mooneye)):
        root = ROMS / "test" / family
        fam = run_roms(co, sorted(root.rglob("*.gb")), root, runner)
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


def run_screenshot(co, rom, model, expected):
    """Screenshot test (cgb-acid2, Mealybug): frame hash at LD B,B vs expected."""
    code, out, err, secs = sh([gb(co), "--rom", str(rom), "--frames", "1200", "--mooneye", "--model", model], co, timeout=120)
    if code == HUNG:
        return "hung", secs, err[-300:]
    if code == 2:
        return "panic", secs, err[-500:]
    m = re.search(r"frame-hash ([0-9a-f]{16})", out)
    if not m:
        return "fail", secs, "no LD B,B breakpoint reached"
    return ("pass" if m.group(1) == expected else "fail"), secs, f"got {m.group(1)} want {expected}"


def family(co, root, runner):
    """Run every ROM under root with runner(co, rom) -> (status, secs, detail)."""
    roms = run_roms(co, sorted(list(root.rglob("*.gb")) + list(root.rglob("*.gbc"))), root, runner)
    return {"passed": sum(1 for v in roms.values() if v["status"] == "pass"), "total": len(roms), "roms": roms}


def _run_game(co, rom, frames, script, model="dmg"):
    """Run a game headless; return (exit_code, {frame: hash}, stderr, secs)."""
    with tempfile.TemporaryDirectory() as td:
        code, out, err, secs = sh(
            [gb(co), "--rom", str(rom), "--frames", str(frames), "--input-script", str(script),
             "--dump-every", str(SAMPLE_EVERY), "--dump-dir", td, "--hash", "--model", model],
            co, timeout=300,
        )
    got = {int(k): v for k, v in re.findall(r"frame (\d+) ([0-9a-f]{16})", out)}
    return code, got, err, secs


def tier3(co, golden, roms_dir=None, model="dmg"):
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
    hung_streak = 0
    roms_dir = roms_dir or (ROMS / "games")
    for rom in sorted(list(roms_dir.glob("*.gb")) + list(roms_dir.glob("*.gbc"))):
        name = rom.stem
        if hung_streak >= 2:
            games[name] = {"status": "skipped", "score": 0.0, "detail": "skipped after 2 consecutive hangs"}
            continue
        gold = golden / f"{name}.fnv"
        meta_p = golden / f"{name}.robust.json"
        script = golden / f"{name}.input"
        if not (gold.exists() and script.exists() and meta_p.exists()):
            games[name] = {"status": "no-golden", "score": 0.0}
            continue
        expected = {int(k): v for k, v in (line.split() for line in gold.read_text().split("\n") if line.strip())}
        meta = json.loads(meta_p.read_text())
        frames, window = meta["frames"], meta.get("window", ALIGN_WINDOW)
        code, got, err, secs = _run_game(co, rom, frames, script, model)
        hung_streak = hung_streak + 1 if code == HUNG else 0
        if code == HUNG:
            games[name] = {"status": "hung", "score": 0.0, "detail": err[-300:]}
            continue
        if code == 2:
            games[name] = {"status": "panic", "score": 0.0, "detail": err[-500:]}
            continue
        with tempfile.TemporaryDirectory() as td:
            empty = Path(td) / "empty.input"
            empty.write_text("")
            code2, got_noinput, err2, secs2 = _run_game(co, rom, frames, empty, model)
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
        mh = re.search(r"final ([0-9a-f]{16})", out)
        hashes.append(mh.group(1) if (code == 0 and mh) else f"exit{code}")
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


# ---- GEP 1 §3: Game Boy Color ---------------------------------------------------
def tier5(co, golden_cgb):
    r = {
        "mooneye_cgb": family(co, ROMS / "test" / "mooneye-cgb", lambda c, rom: run_mooneye(c, rom, "cgb")),
        "cgb_sound": family(co, ROMS / "test" / "blargg-mem-cgb", lambda c, rom: run_blargg_mem(c, rom, "cgb")),
        "cgb_acid2": family(co, ROMS / "test" / "cgb-acid2",
                            lambda c, rom: run_screenshot(c, rom, "cgb", rom.with_suffix(".fnv").read_text().strip())),
    }
    r["games_cgb"] = tier3(co, golden_cgb, roms_dir=ROMS / "games-cgb", model="cgb")
    return r


# ---- GEP 1 §4: pixel-accurate PPU -----------------------------------------------
def tier6(co):
    root = ROMS / "test" / "mealybug-dmg"
    return {"mealybug_dmg": family(co, root,
            lambda c, rom: run_screenshot(c, rom, "dmg", rom.with_suffix(".fnv").read_text().strip()))}


# ---- GEP 1 §5: tooling ------------------------------------------------------------
def tier7(co, golden_trace):
    r = {}
    code, out, err, secs = sh(["cargo", "build", "--release", "--offline", "-p", "gb-tools"], co, timeout=1200)
    r["build"] = {"ok": code == 0, "stderr_tail": err[-1500:]}
    tr = co / "target" / "release" / "gb-trace"
    trace = {}
    for blocks in sorted(golden_trace.glob("*.blocks")):
        name = blocks.stem
        rom = ROMS / "test" / "blargg" / "cpu_instrs" / "individual" / f"{name}.gb"
        want_lines = next((int(l.split("=")[1]) for l in blocks.read_text().splitlines() if l.startswith("# lines=")), 0)
        if not tr.exists() or not rom.exists():
            trace[name] = {"fraction": 0.0, "detail": "gb-trace or ROM missing"}
            continue
        t = time.time()
        p1 = None
        try:
            p1 = subprocess.Popen([str(tr), "--rom", str(rom), "--instructions", str(want_lines), "--doctor"],
                                  cwd=co, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
            # the scorer stops reading at the first divergent block; gb-trace then dies of SIGPIPE
            p2 = subprocess.run([sys.executable, str(HERE / "trace_blocks.py"), "score", str(blocks)],
                                stdin=p1.stdout, capture_output=True, text=True, timeout=600)
            res = json.loads(p2.stdout)
        except Exception as e:  # timeouts, crashes, bad output
            res = {"fraction": 0.0, "detail": repr(e)[:300]}
        finally:
            if p1 is not None:
                p1.stdout.close()
                p1.kill()
                p1.wait()
        res["secs"] = round(time.time() - t, 1)
        trace[name] = res
    r["trace"] = trace
    r["trace_score"] = round(sum(v.get("fraction", 0.0) for v in trace.values()) / max(1, len(trace)), 4)
    # profiler: compare top-20 against the reference profile of the same instructions
    prof = {}
    for pj in sorted(golden_trace.glob("*.profile.json")):
        ref = json.loads(pj.read_text())
        name = pj.name[: -len(".profile.json")]
        rom = ROMS / "test" / "blargg" / "cpu_instrs" / "individual" / f"{name}.gb"
        if not tr.exists():
            prof[name] = {"ok": False, "detail": "gb-trace missing"}
            continue
        code, out, err, secs = sh([str(tr), "--rom", str(rom), "--instructions", str(ref["instructions"]),
                                   "--doctor", "--profile", "--top", "20"], co, timeout=600)
        got = [(int(a, 16), int(b)) for a, b in re.findall(r"PC:([0-9A-Fa-f]{4}) COUNT:(\d+)", out)]
        tot = re.search(r"TOTAL:(\d+)", out)
        want = [(e["pc"], e["count"]) for e in ref["top"]]
        prof[name] = {"ok": got == want and bool(tot) and int(tot.group(1)) == ref["instructions"],
                      "secs": round(secs, 1)}
    r["profile"] = prof
    r["profile_score"] = round(sum(1 for v in prof.values() if v.get("ok")) / max(1, len(prof)), 4)
    # debugger API conformance
    srv = co / "target" / "release" / "gb-server"
    if srv.exists():
        code, out, err, secs = sh([sys.executable, str(HERE / "api_conformance.py"), str(srv),
                                   "--roms", str(ROMS), "--golden-trace", str(golden_trace),
                                   "--gb", gb(co)], co, timeout=1200)
        try:
            r["api"] = json.loads(out.strip().splitlines()[-1])
        except Exception:
            r["api"] = {"score": 0.0, "detail": (out + err)[-800:]}
    else:
        r["api"] = {"score": 0.0, "detail": "gb-server missing"}
    return r


# ---- GEP 1 §6: portability --------------------------------------------------------
def tier8(co):
    r = {}
    code, out, err, secs = sh(["cargo", "build", "-p", "gb-core", "--release", "--offline", "--no-default-features",
                               "--target", "thumbv7em-none-eabihf"], co, timeout=1200)
    r["no_std"] = {"ok": code == 0, "stderr_tail": err[-1500:]}
    code, out, err, secs = sh(["cargo", "build", "-p", "gb-wasm", "--release", "--offline",
                               "--target", "wasm32-unknown-unknown"], co, timeout=1200)
    wasm = co / "target" / "wasm32-unknown-unknown" / "release" / "gb_wasm.wasm"
    r["wasm_build"] = {"ok": code == 0 and wasm.exists(), "stderr_tail": err[-1500:]}
    if r["wasm_build"]["ok"]:
        code, out, err, secs = sh([sys.executable, str(HERE / "wasm_check.py"), str(wasm), "--gb", gb(co),
                                   "--roms", str(ROMS)], co, timeout=1200)
        try:
            r["wasm"] = json.loads(out.strip().splitlines()[-1])
        except Exception:
            r["wasm"] = {"score": 0.0, "detail": (out + err)[-800:]}
    else:
        r["wasm"] = {"score": 0.0, "detail": "build failed"}
    return r


# ---- GEP 1 §7–§8: game library service and web front end ----------------------
def tier9(co):
    r = {}
    code, out, err, secs = sh(["cargo", "build", "--release", "--offline", "-p", "gb-web"], co, timeout=1200)
    srv = co / "target" / "release" / "gb-web"
    r["build"] = {"ok": code == 0 and srv.exists(), "stderr_tail": err[-1500:]}
    wasm = co / "target" / "wasm32-unknown-unknown" / "release" / "gb_wasm.wasm"
    if not wasm.exists():         # tier 8 builds it; without it the player cannot work, the API still can
        wasm = Path(tempfile.mkdtemp()) / "missing.wasm"
        wasm.write_bytes(b"\0asm\1\0\0\0")
    for key, script in (("api", "web_conformance.py"), ("ui", "ui_e2e.py")):
        if not r["build"]["ok"]:
            r[key] = {"score": 0.0, "detail": "gb-web did not build"}
            continue
        code, out, err, secs = sh([sys.executable, str(HERE / script), str(srv), "--roms", str(ROMS),
                                   "--wasm", str(wasm), "--gb", gb(co)], co, timeout=1500)
        try:
            r[key] = json.loads(out.strip().splitlines()[-1])
        except Exception:
            r[key] = {"score": 0.0, "detail": (out + err)[-800:]}
        r[key]["secs"] = round(secs, 1)
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
    ap.add_argument("--tier", type=int, help="run only this tier (0-9)")
    ap.add_argument("--roms", type=Path, default=None, help="roms/ directory (default: repo's own roms/)")
    ap.add_argument("--frozen-dir", type=Path, default=None, help="directory holding reference copies of the frozen files")
    ap.add_argument("--golden-cgb", type=Path, default=HERE.parent / "golden-cgb")
    ap.add_argument("--golden-trace", type=Path, default=HERE.parent / "golden-trace")
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
    tiers = [a.tier] if a.tier is not None else [0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
    # Write after every tier, so a run cut short by the verifier's time limit
    # still leaves everything graded so far.
    real_report = report

    class Incremental(dict):
        def __setitem__(self, k, v):
            super().__setitem__(k, v)
            a.out.write_text(json.dumps(self, indent=2))
    report = Incremental(real_report)
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
    if 5 in tiers:
        report["tier5"] = tier5(co, a.golden_cgb.resolve())
    if 6 in tiers:
        report["tier6"] = tier6(co)
    if 7 in tiers:
        report["tier7"] = tier7(co, a.golden_trace.resolve())
    if 8 in tiers:
        report["tier8"] = tier8(co)
    if 9 in tiers:
        report["tier9"] = tier9(co)

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
    t5, t6, t7, t8, t9 = (report.get(f"tier{i}", {}) for i in (5, 6, 7, 8, 9))
    pr = lambda d: f"{d.get('passed', '-')}/{d.get('total', '-')}"
    print(
        "cgb: mooneye {} · acid2 {} · games {} · sound {} | mealybug {} | trace {} · profile {} · debugger {} "
        "| no_std {} · wasm {} | web api {} · web ui {}".format(
            pr(t5.get("mooneye_cgb", {})), pr(t5.get("cgb_acid2", {})), pr(t5.get("games_cgb", {})),
            pr(t5.get("cgb_sound", {})), pr(t6.get("mealybug_dmg", {})),
            t7.get("trace_score", "-"), t7.get("profile_score", "-"), t7.get("api", {}).get("score", "-"),
            t8.get("no_std", {}).get("ok", "-"), t8.get("wasm", {}).get("score", "-"),
            t9.get("api", {}).get("score", "-"), t9.get("ui", {}).get("score", "-"),
        )
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
