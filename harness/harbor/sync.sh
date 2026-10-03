#!/usr/bin/env bash
# Stage files from harness/ into the Harbor task directory. Run before
# `harbor run`. The staged copies are git-ignored; the sources in harness/
# are canonical.
set -euo pipefail
H="$(cd "$(dirname "$0")/.." && pwd)"
T="$H/harbor/task"

# pristine assets (built once by scripts/build_assets.sh)
[ -d "$H/.assets/roms" ] || "$H/scripts/build_assets.sh"

# every graded game ROM must have goldens and every golden its ROM — a mismatch
# would silently score a game 0 (or skip it) for every agent
python3 - "$H" <<'PYCHECK'
import sys
from pathlib import Path
H = Path(sys.argv[1])
bad = []
for roms, golden, ext in (("games", "golden", "*.gb"), ("games-cgb", "golden-cgb", "*.gbc")):
    have = {p.stem for p in (H / ".assets/roms" / roms).glob(ext)}
    gold = {p.name[: -len(".robust.json")] for p in (H / golden).glob("*.robust.json")}
    if have != gold:
        bad.append(f"{roms}: ROMs without goldens {sorted(have - gold)}, goldens without ROMs {sorted(gold - have)}")
if bad:
    sys.exit("sync.sh: " + "; ".join(bad) + " — rebuild harness/.assets (scripts/build_assets.sh)")
PYCHECK

# verifier
mkdir -p "$T/tests/golden" "$T/tests/frozen/gb-core/tests" "$T/tests/frozen/gb-cli/src"
cp "$H/scripts/grade.py" "$H/scripts/screenshots.py" "$H/scripts/trace_blocks.py" \
   "$H/scripts/api_conformance.py" "$H/scripts/wasm_check.py" "$H/scripts/pngio.py" \
   "$H/scripts/web_conformance.py" "$H/scripts/ui_e2e.py" "$H/scripts/romgen.py" \
   "$H/showcase/tickets_conformance.py" "$T/tests/"
rm -rf "$T/tests/golden" && cp -R "$H/golden" "$T/tests/golden"
rm -rf "$T/tests/golden-cgb" "$T/tests/golden-trace" "$T/tests/roms"
cp -R "$H/golden-cgb" "$T/tests/golden-cgb"
cp -R "$H/golden-trace" "$T/tests/golden-trace"
cp -R "$H/.assets/roms" "$T/tests/roms"
cp "$H/../gb-core/tests/rom_suite.rs" "$T/tests/frozen/gb-core/tests/"
cp "$H/../gb-cli/src/main.rs" "$T/tests/frozen/gb-cli/src/"

# product-owner sidecar: the GEP and the backlog (which the agent also has) +
# the product owner's decisions on backlog items
cp "$H/../GEP-0001.md" "$H/AGENT_BRIEF.md" "$H/HIDDEN_SPEC.md" "$H/PRODUCT_OWNER.md" "$H/orchestrator/po_agent.py" \
   "$T/environment/po/"
rm -rf "$T/environment/po/ISSUES" && cp -R "$H/../ISSUES" "$T/environment/po/ISSUES"
# issues filed during the run (released by the sidecar, harness/showcase/waves)
rm -rf "$T/environment/po/waves" && cp -R "$H/showcase/waves" "$T/environment/po/waves"

# instruction = the agent brief body
python3 -c "print(open('$H/AGENT_BRIEF.md').read().split('\n---\n',1)[1].lstrip())" > "$T/instruction.md"

# scaffold: the exact committed HEAD of this repo goes into the image build
# context (no GitHub clone, so the image can never be stale relative to HEAD)
if [[ -n "$(git -C "$H/.." status --porcelain -- . ':!harness/harbor/task' 2>/dev/null)" ]]; then
  echo "warning: uncommitted changes are NOT included in scaffold.tar (it is built from HEAD)" >&2
fi
git -C "$H/.." archive --format=tar -o "$T/environment/scaffold.tar" HEAD
echo "scaffold.tar = $(git -C "$H/.." rev-parse --short HEAD)"

echo "staged into $T"
