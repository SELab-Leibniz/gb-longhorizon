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

echo "==> Opcode table (gbdev/gb-opcodes; also has per-opcode descriptions)"
git clone --depth 1 https://github.com/gbdev/gb-opcodes "$TMP/gbop"
cp "$TMP/gbop/Opcodes.json" "$ROOT/docs/opcodes.json"
cp "$TMP/gbop/OpcodeDescriptions.json" "$ROOT/docs/opcode_descriptions.json"
(cd "$TMP/gbop" && git rev-parse HEAD) > "$ROOT/docs/OPCODES_COMMIT"

echo "==> Blargg test ROMs (serial-reporting subset)"
git clone --depth 1 https://github.com/retrio/gb-test-roms "$TMP/blargg"
mkdir -p "$ROOT/roms/test/blargg"
cp -r "$TMP/blargg/cpu_instrs"  "$ROOT/roms/test/blargg/"
cp -r "$TMP/blargg/instr_timing" "$ROOT/roms/test/blargg/"
cp -r "$TMP/blargg/mem_timing"   "$ROOT/roms/test/blargg/"
cp    "$TMP/blargg/halt_bug.gb"  "$ROOT/roms/test/blargg/"
# Memory-reporting Blargg suites (status at $A000): audio and the OAM bug.
mkdir -p "$ROOT/roms/test/blargg-mem"
cp -r "$TMP/blargg/dmg_sound" "$ROOT/roms/test/blargg-mem/"
cp -r "$TMP/blargg/oam_bug"   "$ROOT/roms/test/blargg-mem/"
find "$ROOT/roms/test/blargg-mem" -type f ! -name '*.gb' -delete
# Keep only .gb files; drop source, readmes and the individual
# cpu_instrs/individual ROMs are kept because they localise failures.
find "$ROOT/roms/test/blargg" -type f ! -name '*.gb' -delete

echo "==> Mooneye test suite (built from source with wla-dx; both from GitHub)"
git clone --depth 1 https://github.com/vhelin/wla-dx "$TMP/wla"
(cd "$TMP/wla" && mkdir -p build && cd build && cmake -DCMAKE_BUILD_TYPE=Release .. >/dev/null && make -j"$(nproc)" wla-gb wlalink >/dev/null)
git clone --depth 1 https://github.com/Gekkio/mooneye-test-suite "$TMP/mooneye"
(cd "$TMP/mooneye" && PATH="$TMP/wla/build/binaries:$PATH" make -j"$(nproc)" all >/dev/null 2>&1)
MTS_DIR="$TMP/mooneye/build"
rm -rf "$ROOT/roms/test/mooneye"
mkdir -p "$ROOT/roms/test/mooneye/emulator-only"
cp -r "$MTS_DIR/acceptance" "$ROOT/roms/test/mooneye/"
cp -r "$MTS_DIR/emulator-only/mbc1" "$ROOT/roms/test/mooneye/emulator-only/"
cp -r "$MTS_DIR/emulator-only/mbc5" "$ROOT/roms/test/mooneye/emulator-only/"
# Exclusions (see roms/README.md): manual tests, MBC1M multicart, and ROMs
# whose suffix targets another model. We emulate DMG-B, so keep unsuffixed,
# -dmgABC, -dmgABCmgb and -GS (G = DMG); drop SGB/SGB2/DMG0/MGB/CGB/AGB.
rm -rf "$ROOT/roms/test/mooneye/acceptance/manual-only"
rm -f  "$ROOT/roms/test/mooneye/emulator-only/mbc1/multicart_rom_8Mb.gb"
find "$ROOT/roms/test/mooneye" -type f -name '*.gb' \
     \( -name '*-S.gb' -o -name '*-dmg0.gb' -o -name '*-mgb.gb' -o -name '*-sgb.gb' -o -name '*-sgb2.gb' \
        -o -name '*-A.gb' -o -name '*-C.gb' -o -name '*-cgb*.gb' -o -name '*-agb*.gb' \) -delete
find "$ROOT/roms/test/mooneye" -type f ! -name '*.gb' -delete
(cd "$TMP/mooneye" && git rev-parse HEAD) > "$ROOT/roms/test/mooneye/SOURCE"

echo "==> dmg-acid2"
# Pinned release (the GitHub API is not needed; the asset URL is stable).
ACID_URL="https://github.com/mattcurrie/dmg-acid2/releases/download/v1.0/dmg-acid2.gb"
mkdir -p "$ROOT/roms/test/acid2"
curl -fsSL "$ACID_URL" -o "$ROOT/roms/test/acid2/dmg-acid2.gb"
echo "NOTE: roms/test/acid2/expected.fnv is produced by make_golden.sh"

echo "==> Homebrew games (from the Homebrew Hub database, pinned commit)"
# roms/LICENSES.md lists title, author, licence and source for each. The
# hub redistributes these ROMs under their authors' open licences.
HUB_COMMIT="50293559a496a3e20382fbf6a2e84b70ec622f88"
git clone -q --depth 1 --filter=blob:none --sparse https://github.com/gbdev/database "$TMP/hub"
git -C "$TMP/hub" checkout -q "$HUB_COMMIT" 2>/dev/null || echo "   (pinned hub commit not reachable with --depth 1; using HEAD)"
mkdir -p "$ROOT/roms/games"
declare -A GAMES=(
  [tobudx]="tobutobugirldeluxe/tobudx.gb"
  [libbet]="libbet/libbet.gb"
  [carazu]="carazu/carazu.gb"
  [shocklobster]="shock-lobster/shocklobster.gb"
  [renegaderush]="renegade-rush/RenegadeRush.gb"
  [tuff]="tuff/game.gb"
  [big2small]="big2small/big2small.gb"
  [postbot]="postbot/PostBot.gb"
  [maxpirate]="maxpirate/maxpirate.gb"
  [2048]="2048gb/2048.gb"
)
PATTERNS=()
for name in "${!GAMES[@]}"; do PATTERNS+=("/entries/$(dirname "${GAMES[$name]}")/*"); done
git -C "$TMP/hub" sparse-checkout set --no-cone "${PATTERNS[@]}"
for name in "${!GAMES[@]}"; do
  cp "$TMP/hub/entries/${GAMES[$name]}" "$ROOT/roms/games/$name.gb"
  echo "   $name.gb"
done

echo "done. Review roms/LICENSES.md before building the image."
