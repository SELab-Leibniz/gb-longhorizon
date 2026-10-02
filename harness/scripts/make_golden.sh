#!/usr/bin/env bash
# Produce the golden data the grader needs, using the SameBoy reference
# runner (build it first: harness/ref/build.sh):
#
#   roms/test/acid2/expected.fnv   — committed; the agent sees it
#   harness/golden/<game>.fnv      — one line per frame "<n> <hash>"; hidden
#   harness/golden/<game>.input    — the input script (you write these)
#
# Both emulators hash the 160×144 2-bit shade buffer with FNV-1a-64, so the
# numbers are directly comparable to `gb --hash` output. Frame alignment
# between emulators is ±1–2 frames; grade.py matches within a window.
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

echo "==> homebrew games"
shopt -s nullglob
for rom in "$ROOT"/roms/games/*.gb; do
  name="$(basename "$rom" .gb)"
  script="$GOLDEN/$name.input"
  if [[ ! -f "$script" ]]; then
    echo "    $name: no input script at $script — skipping (write one; see gb-cli for the format)"
    continue
  fi
  # Run 10 s past the last scripted input.
  frames="$(awk '!/^#/ && NF {f=$1} END {print f+600}' "$script")"
  "$RUNNER" "$rom" "$frames" "$script" "$TMP/$name" "${BOOT_ARGS[@]}" --dump-every 0 2>/dev/null
  cp "$TMP/$name/hashes.txt" "$GOLDEN/$name.fnv"
  echo "    $name: $frames frames"
done

cp "$ROOT/harness/ref/bin/SAMEBOY_COMMIT" "$GOLDEN/SAMEBOY_COMMIT" 2>/dev/null || true
echo "done -> $GOLDEN"
