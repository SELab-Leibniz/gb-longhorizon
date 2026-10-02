#!/usr/bin/env bash
# Produce the golden data the grader needs, using the SameBoy reference
# runner (build it first: harness/ref/build.sh):
#
#   roms/test/acid2/expected.fnv   — committed; the agent sees it
#   harness/golden/<game>.fnv      — one line per frame "<n> <hash>"; hidden
#   harness/golden/<game>.input    — the input script (you write these)
#
# Both emulators hash the 160×144 2-bit shade buffer with FNV-1a-64, so the
# numbers are directly comparable to `gb --hash` output. The reference runs
# with power-on RAM zeroed (deterministic); grade.py only grades the
# timing-robust frames listed in <game>.robust.json, within a ±2 frame window.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
GOLDEN="$ROOT/harness/golden"
RUNNER="$ROOT/harness/ref/bin/sameboy_runner"
BOOT="$ROOT/harness/ref/bin/dmg_boot.bin"
mkdir -p "$GOLDEN"

[[ -x "$RUNNER" ]] || { echo "build the reference runner first: harness/ref/build.sh" >&2; exit 1; }
BOOT_ARGS=()
[[ -f "$BOOT" ]] && BOOT_ARGS=(--boot "$BOOT")

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

echo "==> dmg-acid2"
"$RUNNER" "$ROOT/roms/test/acid2/dmg-acid2.gb" 120 /dev/null "$TMP/acid2" "${BOOT_ARGS[@]}" --dump-every 0 2>/dev/null
tail -1 "$TMP/acid2/hashes.txt" | cut -d' ' -f2 > "$ROOT/roms/test/acid2/expected.fnv"
echo "    expected.fnv = $(cat "$ROOT/roms/test/acid2/expected.fnv")"

cp "$ROOT/harness/ref/bin/SAMEBOY_COMMIT" "$GOLDEN/SAMEBOY_COMMIT" 2>/dev/null || true

echo "==> homebrew games: base hashes + timing-robust frame sets"
# Runs each game under several boot-phase / input-timing perturbations and
# keeps only the sample frames all of them agree on (see the script header).
python3 "$ROOT/harness/scripts/make_game_goldens.py" --runner "$RUNNER" --roms "$ROOT/roms/games" --golden "$GOLDEN"

echo "==> reference screenshots for the manual-review page"
rm -rf "$GOLDEN/screens"
python3 "$ROOT/harness/scripts/screenshots.py" --reference "$RUNNER" --roms "$ROOT/roms" --golden "$GOLDEN"
echo "done -> $GOLDEN"
