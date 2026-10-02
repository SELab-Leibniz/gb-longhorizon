#!/usr/bin/env bash
# Build the SameBoy-based reference runner.
#
#   harness/ref/build.sh            → harness/ref/bin/sameboy_runner
#
# Clones SameBoy at a pinned commit (so goldens are reproducible), builds
# its core as a static library, and links sameboy_runner.c against it with
# the internal headers visible (we read gb.boot_rom_finished, like
# SameBoy's own Tester does).
#
# Optional: if rgbds (rgbasm/rgblink/rgbfix) is installed, SameBoy's free
# DMG boot ROM is also built to harness/ref/bin/dmg_boot.bin and
# make_golden.sh will pass it with --boot. Without rgbds the runner uses
# its embedded post-boot stub, which is sufficient for golden hashes.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
SAMEBOY_COMMIT="${SAMEBOY_COMMIT:-213a12ce93d66b105a113debd9396306066a7cfc}"
SRC="$HERE/sameboy-src"
BIN="$HERE/bin"
mkdir -p "$BIN"

if [[ ! -d "$SRC/.git" ]]; then
  git clone -q https://github.com/LIJI32/SameBoy "$SRC"
fi
git -C "$SRC" fetch -q origin "$SAMEBOY_COMMIT" 2>/dev/null || true
git -C "$SRC" checkout -q "$SAMEBOY_COMMIT"

echo "==> building libsameboy (CONF=release)"
make -C "$SRC" -j"$(nproc)" CONF=release lib >/dev/null

echo "==> building sameboy_runner"
cc -O2 -std=gnu11 -Wall -Wno-unused-parameter -Wno-multichar \
   -DGB_INTERNAL -DGB_VERSION='"ref"' \
   -I"$SRC" -I"$SRC/Core" \
   "$HERE/sameboy_runner.c" "$SRC/build/lib/libsameboy.a" \
   -lm -o "$BIN/sameboy_runner"

if command -v rgbasm >/dev/null 2>&1; then
  echo "==> building SameBoy DMG boot ROM"
  make -C "$SRC" CONF=release build/bin/BootROMs/dmg_boot.bin >/dev/null
  cp "$SRC/build/bin/BootROMs/dmg_boot.bin" "$BIN/dmg_boot.bin"
else
  echo "    (rgbds not found; runner will use the embedded boot stub)"
fi

echo "$SAMEBOY_COMMIT" > "$BIN/SAMEBOY_COMMIT"
echo "ok: $BIN/sameboy_runner"
