#!/usr/bin/env bash
# Stage files from harness/ into the Harbor task directory. Run before
# `harbor run`. The staged copies are git-ignored; the sources in harness/
# are canonical.
set -euo pipefail
H="$(cd "$(dirname "$0")/.." && pwd)"
T="$H/harbor/task"

# pristine ROMs for the verifier (built once by scripts/build_assets.sh)
[ -d "$H/.assets/roms" ] || "$H/scripts/build_assets.sh"
rm -rf "$T/tests/roms" && cp -R "$H/.assets/roms" "$T/tests/roms"

# verifier
mkdir -p "$T/tests/golden" "$T/tests/frozen/gb-core/tests" "$T/tests/frozen/gb-cli/src"
cp "$H/scripts/grade.py" "$H/scripts/screenshots.py" "$T/tests/"
rm -rf "$T/tests/golden" && cp -R "$H/golden" "$T/tests/golden"
cp "$H/../gb-core/tests/rom_suite.rs" "$T/tests/frozen/gb-core/tests/"
cp "$H/../gb-cli/src/main.rs" "$T/tests/frozen/gb-cli/src/"

# product-owner sidecar
cp "$H/HIDDEN_SPEC.md" "$H/PRODUCT_OWNER.md" "$H/orchestrator/po_agent.py" "$T/environment/po/"

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
