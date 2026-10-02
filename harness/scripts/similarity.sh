#!/usr/bin/env bash
# Contamination check: compare the agent's gb-core against well-known Rust
# Game Boy emulators. Requires JPlag (https://github.com/jplag/JPlag),
# which supports Rust: download the release jar and set JPLAG_JAR.
#
#   harness/scripts/similarity.sh /path/to/agent/checkout
#
# Output: a JPlag result zip plus a short text summary of the highest
# pairwise similarities. Interpret with care — a shared hardware spec and
# idiomatic Rust produce real overlap in opcode dispatch tables. The signal
# is structural identity across *several* modules, or identical unusual
# identifiers.
set -euo pipefail

CHECKOUT="${1:?usage: similarity.sh CHECKOUT}"
: "${JPLAG_JAR:?set JPLAG_JAR to the JPlag jar path}"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# Reference set. Add/remove freely; record what you used in the write-up.
REFS=(
  "https://github.com/Gekkio/mooneye-gb"      # author of the Mooneye test suite; Rust
  "https://github.com/mvdnes/rboy"
  "https://github.com/mohanson/gameboy"
  "https://github.com/joamag/boytacean"
  "https://github.com/alexcrichton/jba"
  "https://github.com/nicholasbishop/gb-emu"
  "https://github.com/p4ddy1/gbemulator"
  "https://github.com/ArmanKolozyan/gameboy-emulator"
  # The agent's own scaffold: expected to match the stub signatures. Included
  # so that scaffold overlap is visible as a baseline, not mistaken for copying.
  "SCAFFOLD"
)

mkdir -p "$WORK/subs"
cp -r "$CHECKOUT/gb-core/src" "$WORK/subs/agent"

SCAFFOLD_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
for ref in "${REFS[@]}"; do
  if [[ "$ref" == "SCAFFOLD" ]]; then
    cp -r "$SCAFFOLD_ROOT/gb-core/src" "$WORK/subs/scaffold"
    continue
  fi
  name="$(basename "$ref")"
  if git clone --depth 1 -q "$ref" "$WORK/clone_$name" 2>/dev/null; then
    mkdir -p "$WORK/subs/$name"
    find "$WORK/clone_$name" -name '*.rs' -not -path '*/target/*' -exec cp --parents {} "$WORK/subs/$name/" \; 2>/dev/null || true
  else
    echo "skip $ref (clone failed)" >&2
  fi
done

java -jar "$JPLAG_JAR" -l rust -r "$WORK/result" --min-tokens 12 "$WORK/subs" > "$WORK/jplag.log" 2>&1 || true

OUT="${CHECKOUT}/../similarity-$(date +%Y%m%d-%H%M).zip"
cp "$WORK/result.zip" "$OUT" 2>/dev/null || cp "$WORK/result"*.zip "$OUT"
echo "JPlag result: $OUT"
echo "Top comparisons involving 'agent':"
grep -i "agent" "$WORK/jplag.log" | sort -t' ' -k3 -rn | head -10 || true
echo
echo "Open the zip in https://jplag.github.io/JPlag/ to inspect matches."
