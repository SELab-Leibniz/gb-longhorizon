#!/bin/bash
# Harbor verifier for selab/gb-longhorizon. Runs in the agent's container
# (shared mode) after the agent phase. Writes /logs/verifier/reward.json.
#
# /tests holds (staged by harness/harbor/sync.sh): grade.py, golden/,
# frozen/ (reference copies of the files the agent must not change).
set -uo pipefail
OUT=/logs/verifier
mkdir -p "$OUT"

# Stop the agent before grading. The adapters exit on their own before the
# Harbor timeout and clean up their process trees on TERM; anything left over
# (e.g. a renamed agent process orphaned by a hard kill) is killed here, by
# process tree and by known names — never trust names alone, iCode renames
# itself to "chrys".
kill_tree() { local c; for c in $(pgrep -P "$1" 2>/dev/null); do kill_tree "$c"; done; kill -KILL "$1" 2>/dev/null || true; }
for p in $(pgrep -f "gb-agents/" 2>/dev/null); do kill -TERM "$p" 2>/dev/null || true; done
sleep 5
for p in $(pgrep -f "gb-agents/" 2>/dev/null); do kill_tree "$p"; done
for name in chrys icode jiuwenswarm-process; do pkill -KILL -x "$name" 2>/dev/null || true; done
pkill -KILL -f "/opt/icode/" 2>/dev/null || true
pkill -KILL -f "/opt/jiuwenswarm/" 2>/dev/null || true
sleep 2

cd /work
git log --format='%H%x09%at%x09%s' > "$OUT/git-log.txt" 2>/dev/null || true
cp SUBMISSION.md "$OUT/SUBMISSION.md" 2>/dev/null || true
cp QUESTIONS.md "$OUT/QUESTIONS.md" 2>/dev/null || true
cp CHANGE_REQUESTS.md "$OUT/CHANGE_REQUESTS.md" 2>/dev/null || true

# Grade the last COMMIT, not the working tree: the brief says "whatever is on
# the branch is what we take", and a run cut off mid-edit must not be graded
# on a half-written file (nor on a stale binary left in target/).
G=/tmp/grade-head
rm -rf "$G" && mkdir -p "$G"
git -C /work archive HEAD | tar -x -C "$G"
[ -d /work/vendor ] && ln -sfn /work/vendor "$G/vendor"
# informational: does the uncommitted working tree build?
if (cd /work && cargo build --release --offline -q >/dev/null 2>&1); then WT=1; else WT=0; fi
echo "{\"working_tree_builds\": $WT, \"uncommitted_files\": $(git -C /work status --porcelain | wc -l)}" > "$OUT/working-tree.json"

python3 /tests/grade.py "$G" --roms /work/roms --golden /tests/golden --frozen-dir /tests/frozen \
        -o "$OUT/results.json" > "$OUT/grade-summary.txt" 2> "$OUT/grade-stderr.txt"
GRADE_RC=$?

# Screenshots for manual review: boot acid2 + every game on the agent's
# emulator, save agent-vs-reference PNGs and an index.html under
# /logs/verifier/screenshots (lands in <trial>/verifier/screenshots/).
python3 /tests/screenshots.py "$G" --roms /work/roms --golden /tests/golden \
        --out "$OUT/screenshots" > "$OUT/screenshots.log" 2>&1 || true

python3 - "$OUT/results.json" "$OUT/reward.json" <<'PY'
import json, sys
try:
    r = json.load(open(sys.argv[1]))
except Exception:
    r = {}
def rate(d):
    t = d.get("total", 0); return (d.get("passed", 0) / t) if t else 0.0
t0 = r.get("tier0", {}); t1 = r.get("tier1", {}); t2 = r.get("tier2", {}); t3 = r.get("tier3", {}); t4 = r.get("tier4", {})
build_ok = bool(t0.get("build", {}).get("ok"))
frozen_ok = all((t0.get("frozen_files_unchanged") or {"x": False}).values())
moon = t1.get("mooneye", {})
bm = t1.get("blargg-mem", {})
shots = {}
try:
    shots = json.load(open("/logs/verifier/screenshots/summary.json"))
except Exception:
    pass
m = {
    "build":              1.0 if build_ok else 0.0,
    "lint_clean":         1.0 if (t0.get("clippy", {}).get("ok") and t0.get("fmt", {}).get("ok")) else 0.0,
    "frozen_unchanged":   1.0 if frozen_ok else 0.0,
    "blargg":             rate(t1.get("blargg", {})),           # CPU, instruction + memory timing
    "mooneye_acceptance": rate(moon.get("acceptance_excl_ppu", {})),
    "mooneye_mbc":        (rate(moon.get("mbc1", {})) + rate(moon.get("mbc5", {}))) / 2,
    "acid2":              1.0 if t2.get("status") == "pass" else 0.0,
    "games":              t3.get("mean_score", 0.0),            # robust-frame fidelity + input response
    "apu":                rate(bm.get("dmg_sound", {})),        # Blargg dmg_sound
    "determinism":        1.0 if t4.get("determinism", {}).get("ok") else 0.0,
    "save_state":         1.0 if t4.get("save_state_round_trip", {}).get("ok") else 0.0,
    # stretch: beyond what the spec requires; reported, weighted lightly
    "stretch":            (rate(moon.get("acceptance_ppu", {})) + rate(bm.get("oam_bug", {}))) / 2,
}
# informational only (manual-review screenshots): fraction of ROMs that booted and rendered
m["boots"] = (sum(1 for v in shots.values() if v.get("status") == "ok") / len(shots)) if shots else 0.0
# Headline reward, weighted by the hidden spec's priorities. Zero if the build
# fails or the agent edited the frozen harness files (tests then meaningless).
w = {"blargg": .20, "acid2": .12, "mooneye_acceptance": .15, "mooneye_mbc": .07, "games": .18,
     "apu": .08, "save_state": .05, "determinism": .04, "lint_clean": .04, "stretch": .07}
assert abs(sum(w.values()) - 1.0) < 1e-9
reward = sum(m[k] * w[k] for k in w) if (build_ok and frozen_ok) else 0.0
m["reward"] = round(reward, 4)
json.dump(m, open(sys.argv[2], "w"), indent=2)
print(json.dumps(m))
PY
echo "grade.py exit=$GRADE_RC" >> "$OUT/grade-summary.txt"

exit 0
