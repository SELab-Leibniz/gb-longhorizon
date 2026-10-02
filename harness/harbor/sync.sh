#!/usr/bin/env bash
# Stage files from harness/ into the Harbor task directory. Run before
# `harbor run`. The staged copies are git-ignored; the sources in harness/
# are canonical.
set -euo pipefail
H="$(cd "$(dirname "$0")/.." && pwd)"
T="$H/harbor/task"

# verifier
mkdir -p "$T/tests/golden" "$T/tests/frozen/gb-core/tests" "$T/tests/frozen/gb-cli/src"
cp "$H/scripts/grade.py" "$T/tests/grade.py"
cp "$H/golden/"* "$T/tests/golden/"
cp "$H/../gb-core/tests/rom_suite.rs" "$T/tests/frozen/gb-core/tests/"
cp "$H/../gb-cli/src/main.rs" "$T/tests/frozen/gb-cli/src/"

# product-owner sidecar
cp "$H/HIDDEN_SPEC.md" "$H/PRODUCT_OWNER.md" "$H/orchestrator/po_agent.py" "$T/environment/po/"

# instruction = the agent brief body
python3 -c "print(open('$H/AGENT_BRIEF.md').read().split('\n---\n',1)[1].lstrip())" > "$T/instruction.md"

echo "staged into $T"
