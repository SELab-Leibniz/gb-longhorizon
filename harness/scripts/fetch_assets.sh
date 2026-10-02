#!/usr/bin/env bash
# Download hardware docs, test ROMs and homebrew games into docs/ and roms/.
# Run once, with network, before building the sandbox image.
#
# Every source below is a public repository or release page. Verify the
# URLs and licences on first run — upstream layouts change — and record
# the exact commits/tags you used in roms/LICENSES.md.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

echo "==> Pan Docs (Markdown source)"
git clone --depth 1 https://github.com/gbdev/pandocs "$TMP/pandocs"
mkdir -p "$ROOT/docs/pandocs"
cp -r "$TMP/pandocs/src/"*.md "$ROOT/docs/pandocs/"
(cd "$TMP/pandocs" && git rev-parse HEAD) > "$ROOT/docs/pandocs/COMMIT"

echo "==> Opcode table"
curl -fsSL https://gbdev.io/gb-opcodes/Opcodes.json -o "$ROOT/docs/opcodes.json"

echo "==> Blargg test ROMs (serial-reporting subset)"
git clone --depth 1 https://github.com/retrio/gb-test-roms "$TMP/blargg"
mkdir -p "$ROOT/roms/test/blargg"
cp -r "$TMP/blargg/cpu_instrs"  "$ROOT/roms/test/blargg/"
cp -r "$TMP/blargg/instr_timing" "$ROOT/roms/test/blargg/"
cp -r "$TMP/blargg/mem_timing"   "$ROOT/roms/test/blargg/"
cp    "$TMP/blargg/halt_bug.gb"  "$ROOT/roms/test/blargg/"
# Keep only .gb files; drop source, readmes and the individual
# cpu_instrs/individual ROMs are kept because they localise failures.
find "$ROOT/roms/test/blargg" -type f ! -name '*.gb' -delete

echo "==> Mooneye test suite"
git clone --depth 1 https://github.com/Gekkio/mooneye-test-suite "$TMP/mooneye"
# The suite ships prebuilt ROMs on the GitHub releases page; building
# from source needs wla-dx. Prefer the release zip:
MOONEYE_RELEASE_URL="$(curl -fsSL https://api.github.com/repos/Gekkio/mooneye-test-suite/releases/latest \
  | grep -o 'https://[^"]*mts-[^"]*\.zip' | head -1)"
curl -fsSL "$MOONEYE_RELEASE_URL" -o "$TMP/mts.zip"
mkdir -p "$TMP/mts" && (cd "$TMP/mts" && unzip -q ../mts.zip)
MTS_DIR="$(find "$TMP/mts" -maxdepth 2 -type d -name 'acceptance' -exec dirname {} \; | head -1)"
mkdir -p "$ROOT/roms/test/mooneye"
cp -r "$MTS_DIR/acceptance" "$ROOT/roms/test/mooneye/"
mkdir -p "$ROOT/roms/test/mooneye/emulator-only"
cp -r "$MTS_DIR/emulator-only/mbc1" "$ROOT/roms/test/mooneye/emulator-only/"
cp -r "$MTS_DIR/emulator-only/mbc5" "$ROOT/roms/test/mooneye/emulator-only/"
# Exclusions per roms/README.md
rm -rf "$ROOT/roms/test/mooneye/acceptance/manual-only"
rm -f  "$ROOT/roms/test/mooneye/emulator-only/mbc1/multicart_rom_8Mb.gb"
find "$ROOT/roms/test/mooneye" -type f ! -name '*.gb' -delete
echo "$MOONEYE_RELEASE_URL" > "$ROOT/roms/test/mooneye/SOURCE"

echo "==> dmg-acid2"
ACID_URL="$(curl -fsSL https://api.github.com/repos/mattcurrie/dmg-acid2/releases/latest \
  | grep -o 'https://[^"]*dmg-acid2\.gb' | head -1)"
mkdir -p "$ROOT/roms/test/acid2"
curl -fsSL "$ACID_URL" -o "$ROOT/roms/test/acid2/dmg-acid2.gb"
echo "NOTE: roms/test/acid2/expected.fnv is produced by make_golden.sh"

echo "==> Homebrew games"
# Fill this list after checking each licence. Candidates with source and
# permissive licences, covering the MBC matrix:
#   - 2048-gb          (no MBC)          https://github.com/Sanqui/2048-gb
#   - Tobu Tobu Girl   (MBC1)            https://github.com/SimonLarsen/tobutobugirl
#   - µCity            (MBC5, big)       https://github.com/AntonioND/ucity
#   - GB-Snake / Flappy Boy / Petris etc. — see awesome-gbdev for more.
# Download the release .gb into roms/games/ and append name, URL, licence,
# and commit to roms/LICENSES.md.
mkdir -p "$ROOT/roms/games"
echo "TODO: populate roms/games/ manually from the list in this script" >&2

echo "done. Review roms/LICENSES.md before building the image."
