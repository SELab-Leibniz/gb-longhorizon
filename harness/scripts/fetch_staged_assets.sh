#!/usr/bin/env bash
# Build the test assets that change requests deliver into the agent's repo.
#
#   fetch_staged_assets.sh CR_DIR OUT
#
# CR_DIR is harness/change_requests (lists, pins, expected hashes).
# OUT/staged/CR-1/... and OUT/staged/CR-2/... mirror repo-relative paths; the
# product-owner sidecar copies them into /work when it releases a request.
# Runs inside the product-owner image at build time (the only place with
# network that the agent cannot see), so the assets are invisible to the
# agent until their change request is released.
set -euo pipefail
CR="$(cd "$1" && pwd)"
OUT="$2"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
S1="$OUT/staged/CR-1"; S2="$OUT/staged/CR-2"
mkdir -p "$S1" "$S2"

echo "==> Mooneye (built from source with wla-dx) -> CR-1 mooneye-cgb"
git clone -q https://github.com/vhelin/wla-dx "$TMP/wla"
(cd "$TMP/wla" && mkdir -p build && cd build && cmake -DCMAKE_BUILD_TYPE=Release .. >/dev/null && make -j"$(nproc)" wla-gb wlalink >/dev/null)
git clone -q https://github.com/Gekkio/mooneye-test-suite "$TMP/mts"
git -C "$TMP/mts" checkout -q "$(cat "$CR/MOONEYE_COMMIT")"
(cd "$TMP/mts" && PATH="$TMP/wla/build/binaries:$PATH" make -j"$(nproc)" all >/dev/null 2>&1)
while IFS= read -r rel; do
  [[ -z "$rel" ]] && continue
  mkdir -p "$S1/roms/test/mooneye-cgb/$(dirname "$rel")"
  cp "$TMP/mts/build/$rel" "$S1/roms/test/mooneye-cgb/$rel"
done < "$CR/mooneye-cgb.txt"

echo "==> Blargg cgb_sound -> CR-1 blargg-mem-cgb"
git clone -q --depth 1 https://github.com/retrio/gb-test-roms "$TMP/blargg"
mkdir -p "$S1/roms/test/blargg-mem-cgb"
cp -r "$TMP/blargg/cgb_sound" "$S1/roms/test/blargg-mem-cgb/"
find "$S1/roms/test/blargg-mem-cgb" -type f ! -name '*.gb' -delete

echo "==> cgb-acid2 -> CR-1"
mkdir -p "$S1/roms/test/cgb-acid2"
curl -fsSL https://github.com/mattcurrie/cgb-acid2/releases/download/v1.1/cgb-acid2.gbc -o "$S1/roms/test/cgb-acid2/cgb-acid2.gbc"
echo "78ce869d9b004a6f" > "$S1/roms/test/cgb-acid2/cgb-acid2.fnv"   # = the project's reference.png, pixel-exact

echo "==> Game Boy Color games (Homebrew Hub, pinned) -> CR-1 roms/games-cgb"
git clone -q --filter=blob:none --sparse https://github.com/gbdev/database "$TMP/hub"
git -C "$TMP/hub" checkout -q "$(cat "$CR/HUB_COMMIT")"
declare -A G=(
  [aevilia]="aevilia/aevilia.gbc" [europa-rescue]="europa-rescue/Europa Rescue.gbc"
  [gbhack]="gbhack/gbhack.gbc"
  [ucity]="ucity/ucity.gbc" [labirinth]="labirinth/Labirinth.gbc" [a-slime-travel]="a-slime-travel/aslimetravel.gbc"
  [tobudx]="tobutobugirldeluxe/tobudx.gb" [libbet]="libbet/libbet.gb" [tuff]="tuff/game.gb" [big2small]="big2small/big2small.gb"
)
P=(); for k in "${!G[@]}"; do P+=("/entries/$(dirname "${G[$k]}")/*"); done
git -C "$TMP/hub" sparse-checkout set --no-cone "${P[@]}"
mkdir -p "$S1/roms/games-cgb"
for k in "${!G[@]}"; do cp "$TMP/hub/entries/${G[$k]}" "$S1/roms/games-cgb/$k.gbc"; done
cp "$CR/GAMES_CGB_LICENSES.md" "$S1/roms/games-cgb/LICENSES.md"

echo "==> Mealybug Tearoom (DMG) -> CR-2 mealybug-dmg"
git clone -q https://github.com/mattcurrie/mealybug-tearoom-tests "$TMP/mbt"
git -C "$TMP/mbt" checkout -q "$(cat "$CR/MEALYBUG_COMMIT")"
mkdir -p "$TMP/mbroms" "$S2/roms/test/mealybug-dmg"
(cd "$TMP/mbroms" && unzip -q "$TMP/mbt/mealybug-tearoom-tests.zip")
python3 - "$CR/mealybug-dmg.json" "$TMP/mbroms" "$S2/roms/test/mealybug-dmg" <<'PY'
import json, shutil, sys, glob, os
exp, src, dst = json.load(open(sys.argv[1])), sys.argv[2], sys.argv[3]
roms = {os.path.splitext(os.path.basename(p))[0]: p for p in glob.glob(f"{src}/**/*.gb", recursive=True)}
for name, e in exp.items():
    shutil.copy(roms[name], f"{dst}/{name}.gb")
    open(f"{dst}/{name}.fnv", "w").write(e["fnv"] + "\n")
print(f"   {len(exp)} mealybug ROMs + expected hashes")
PY
cp "$TMP/mbt/LICENSE" "$S2/roms/test/mealybug-dmg/LICENSE"

for d in "$S1" "$S2"; do echo "   $(basename "$d"): $(find "$d" -type f | wc -l) files"; done
