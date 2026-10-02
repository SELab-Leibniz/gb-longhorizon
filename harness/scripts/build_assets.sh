#!/usr/bin/env bash
# Build the verifier's pristine copy of every test ROM, once, on the host.
#
#   harness/scripts/build_assets.sh            -> harness/.assets/roms
#
# Runs fetch_assets.sh inside a throwaway container, so the host needs only
# Docker. sync.sh copies the result into the Harbor task as tests/roms, which
# Harbor uploads only after the agent has stopped: the agent can edit or
# delete /work/roms, but never the ROMs it is graded on.
set -euo pipefail
H="$(cd "$(dirname "$0")/.." && pwd)"
REPO="$(cd "$H/.." && pwd)"
OUT="$H/.assets"
rm -rf "$OUT" && mkdir -p "$OUT"
docker run --rm -v "$REPO:/src:ro" -v "$OUT:/out" rust:1.97.0-bookworm bash -euc '
  apt-get update -qq >/dev/null && apt-get install -y -qq cmake unzip python3 >/dev/null 2>&1
  mkdir -p /tmp/w && cd /tmp/w && cp -r /src/harness /src/roms /src/docs . 2>/dev/null || true
  mkdir -p roms/test roms/games docs
  harness/scripts/fetch_assets.sh >/dev/null
  cp -r roms /out/roms
  echo "roms: $(find /out/roms -name "*.gb" | wc -l) ROMs"
'
