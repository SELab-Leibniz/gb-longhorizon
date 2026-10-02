#!/usr/bin/env bash
# Produce the golden data the grader needs, using a reference emulator:
#   roms/test/acid2/expected.fnv       — committed to the repo (agent sees it)
#   harness/golden/<game>.fnv           — kept out of the sandbox
#   harness/golden/<game>.input         — the input script used
#
# The reference emulator must be able to run headless and dump frames.
# SameBoy's `sameboy_tester` (built from the SameBoy repo with `make tester`)
# can run a ROM for N frames and write a .bmp; `--input` scripts are not
# supported there, so for Tier 3 the scripts are replayed by a small driver
# that uses SameBoy's libretro core or an equivalent headless build.
#
# Whatever reference you use, the contract is:
#   * run ROM for exactly F frames from post-boot state, no boot ROM
#   * apply the input script with the same semantics as gb-cli
#   * at every 60th frame, take the 160×144 2-bit framebuffer (0 = lightest)
#     and compute FNV-1a-64 over its bytes (see gb-core/src/util.rs)
#
# The hash is over shades, not RGB, so the reference's palette is irrelevant.
# `pgm_to_fnv.py` below converts a reference's grayscale dump into the same
# hash gb-cli prints, as long as the four shades are distinct.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
GOLDEN="$ROOT/harness/golden"
mkdir -p "$GOLDEN"

: "${REF_EMU:?set REF_EMU to a command that runs: REF_EMU ROM FRAMES INPUT_SCRIPT OUT_DIR  and writes frame_XXXXXX.pgm every 60 frames}"

echo "==> dmg-acid2"
mkdir -p "$ROOT/tmp_ref"
$REF_EMU "$ROOT/roms/test/acid2/dmg-acid2.gb" 120 /dev/null "$ROOT/tmp_ref"
python3 "$ROOT/harness/scripts/pgm_to_fnv.py" "$ROOT/tmp_ref/frame_000120.pgm" > "$ROOT/roms/test/acid2/expected.fnv"
rm -rf "$ROOT/tmp_ref"
cat "$ROOT/roms/test/acid2/expected.fnv"

echo "==> homebrew games"
for rom in "$ROOT"/roms/games/*.gb; do
  name="$(basename "$rom" .gb)"
  script="$GOLDEN/$name.input"
  if [[ ! -f "$script" ]]; then
    echo "   no input script for $name — write $script first (see gb-cli for the format)"
    continue
  fi
  frames="$(awk '!/^#/ && NF {f=$1} END {print f+600}' "$script")"
  out="$ROOT/tmp_ref_$name"; mkdir -p "$out"
  $REF_EMU "$rom" "$frames" "$script" "$out"
  : > "$GOLDEN/$name.fnv"
  for pgm in "$out"/frame_*.pgm; do
    n="$(basename "$pgm" .pgm | sed 's/frame_0*//')"
    printf '%s %s\n' "$n" "$(python3 "$ROOT/harness/scripts/pgm_to_fnv.py" "$pgm")" >> "$GOLDEN/$name.fnv"
  done
  rm -rf "$out"
  echo "   $name: $(wc -l < "$GOLDEN/$name.fnv") frames"
done
