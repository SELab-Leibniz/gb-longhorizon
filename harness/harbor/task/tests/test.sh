#!/bin/bash
# Harbor verifier for selab/gb-longhorizon. Runs in the agent's container
# (shared mode) after the agent phase. Writes /logs/verifier/reward.json.
#
# /tests holds (staged by harness/harbor/sync.sh): grade.py, golden/,
# frozen/ (reference copies of the files the agent must not change).
set -uo pipefail
OUT=/logs/verifier
mkdir -p "$OUT"

# The agent wrappers exit on their own before the Harbor timeout, but make
# sure nothing is still editing the repo while we grade.
pkill -KILL -f "gb-agents/" 2>/dev/null || true
pkill -KILL -f "icode run" 2>/dev/null || true
pkill -KILL -f "jiuwenswarm-process" 2>/dev/null || true
sleep 2

cd /work
git log --format='%H%x09%at%x09%s' > "$OUT/git-log.txt" 2>/dev/null || true
cp SUBMISSION.md "$OUT/SUBMISSION.md" 2>/dev/null || true
cp QUESTIONS.md "$OUT/QUESTIONS.md" 2>/dev/null || true

python3 /tests/grade.py /work --roms /work/roms --golden /tests/golden --frozen-dir /tests/frozen \
        -o "$OUT/results.json" > "$OUT/grade-summary.txt" 2> "$OUT/grade-stderr.txt"
GRADE_RC=$?

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
m = {
    "build":              1.0 if build_ok else 0.0,
    "lint_clean":         1.0 if (t0.get("clippy", {}).get("ok") and t0.get("fmt", {}).get("ok")) else 0.0,
    "frozen_unchanged":   1.0 if frozen_ok else 0.0,
    "blargg":             rate(t1.get("blargg", {})),
    "mooneye_acceptance": rate(t1.get("mooneye", {}).get("acceptance_excl_ppu", {})),
    "mooneye_mbc":        (rate(t1.get("mooneye", {}).get("mbc1", {})) + rate(t1.get("mooneye", {}).get("mbc5", {}))) / 2,
    "acid2":              1.0 if t2.get("status") == "pass" else 0.0,
    "games":              rate(t3),
    "determinism":        1.0 if t4.get("determinism", {}).get("ok") else 0.0,
    "save_state":         1.0 if t4.get("save_state_round_trip", {}).get("ok") else 0.0,
}
# Headline reward: weighted by the hidden spec's priorities. Zero if the
# agent touched the frozen harness files (the tests are then meaningless).
w = {"blargg": .25, "acid2": .15, "mooneye_acceptance": .15, "mooneye_mbc": .10, "games": .20, "save_state": .05, "determinism": .05, "lint_clean": .05}
reward = sum(m[k] * w[k] for k in w) if (build_ok and frozen_ok) else 0.0
m["reward"] = round(reward, 4)
json.dump(m, open(sys.argv[2], "w"), indent=2)
print(json.dumps(m))
PY
echo "grade.py exit=$GRADE_RC" >> "$OUT/grade-summary.txt"
exit 0
