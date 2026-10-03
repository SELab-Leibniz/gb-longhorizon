#!/usr/bin/env bash
# Validate the showcase's planted bugs against their hidden ticket checks.
#
#   validate_tickets.sh REF_TREE TESTS_DIR OUT_DIR
#
# REF_TREE   the unbugged, unstubbed reference code (the pilot's crates: commit
#            d235fa5 of showcase-v2, `git archive d235fa5 | tar -x -C REF_TREE`)
# TESTS_DIR  a staged verifier tests dir (roms/, golden-trace/), e.g.
#            harness/harbor/task/tests after sync.sh
# OUT_DIR    one JSON per variant, plus matrix.txt
#
# Variants: ref (no bugs), ref+Bxx (each bug alone), all-Bxx (every bug but
# one), ref+decisions (the P1-P3 reference patch). A ticket check is valid when
# it passes on ref, fails on ref+Bxx for its own bug, and passes on all-Bxx.
# IMG overrides the toolchain image (default: the task's environment image);
# VARIANTS="ref ref+B01 all-B01" runs only those variants.
set -euo pipefail
REF=$(cd "$1" && pwd); TESTS=$(cd "$2" && pwd); mkdir -p "$3"; OUT=$(cd "$3" && pwd)
HERE=$(cd "$(dirname "$0")" && pwd); SCRIPTS=$(cd "$HERE/../scripts" && pwd)
IMG=${IMG:-task__y3epgjr__env-main:latest}

docker run --rm --cpus "${CPUS:-6}" -v "$REF:/ref:ro" -v "$TESTS:/tests:ro" -v "$HERE:/showcase:ro" \
  -v "$SCRIPTS:/scripts:ro" -v "$OUT:/out" -e VARIANTS="${VARIANTS:-}" --entrypoint bash "$IMG" -c '
set -u
BUGS=$(python3 -c "import sys; sys.path.insert(0, \"/showcase\"); import bugs; print(\" \".join(bugs.BUGS))")
FILES=$(python3 -c "import sys; sys.path.insert(0, \"/showcase\"); import bugs; print(\" \".join(sorted({f for f, _, _ in bugs.BUGS.values()} | {\"gb-web/src/main.rs\"})))")
cp -a /ref /w && cd /w && sed -i "s/\"gb-gui\", //" Cargo.toml && rm -rf gb-gui
restore() { for f in $FILES; do cp "/ref/$f" "/w/$f"; done; }      # plain cp: new mtime, cargo rebuilds
variant() {
  local name=$1; shift
  if [ -n "$VARIANTS" ] && ! echo " $VARIANTS " | grep -qF " $name "; then return; fi
  if ! { cargo build --release --offline --workspace -q && cargo build --release --offline -q -p gb-wasm --target wasm32-unknown-unknown; } > "/out/$name.build" 2>&1; then
    echo "{\"build_failed\": true}" > "/out/$name.json"; echo "$name: BUILD FAILED"; return
  fi
  timeout 1800 python3 /showcase/tickets_conformance.py /w --roms /tests/roms --golden-trace /tests/golden-trace > "/out/$name.json" 2> "/out/$name.err"
  python3 -c "import json,sys; r=json.load(open(\"/out/$name.json\")); print(\"$name:\", \" \".join(k for b in r.values() for k, v in b[\"checks\"].items() if not v[\"ok\"]) or \"all pass\")" || echo "$name: no result"
}
variant ref
python3 /showcase/decisions_patch.py /w > /dev/null && variant ref+decisions; restore
for b in $BUGS; do python3 /showcase/bugs.py /w "$b" > /dev/null && variant "ref+$b"; restore; done
for b in $BUGS; do python3 /showcase/bugs.py /w $(echo $BUGS | tr " " "\n" | grep -vx "$b") > /dev/null && variant "all-$b"; restore; done
' 2>&1 | tee "$OUT/matrix.txt"
