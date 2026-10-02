# Reference emulator

`sameboy_runner.c` drives a [SameBoy](https://github.com/LIJI32/SameBoy)
DMG-B core headlessly and emits, for every frame, the FNV-1a-64 hash of the
160×144 2-bit shade buffer — the same number `gb --hash` prints — plus
optional PGM dumps. `make_golden.sh` uses it to produce the Tier 2 and
Tier 3 expectations.

```sh
harness/ref/build.sh                       # clones SameBoy @ pinned commit, builds
harness/ref/bin/sameboy_runner ROM FRAMES INPUT_SCRIPT OUT_DIR [--boot dmg_boot.bin] [--dump-every N]
```

SameBoy's commit is pinned in `build.sh` (`SAMEBOY_COMMIT`) so goldens are
reproducible; `make_golden.sh` records it next to the goldens.

**Boot ROM.** SameBoy needs one. With rgbds installed, `build.sh` builds
SameBoy's own free `dmg_boot.bin` and `make_golden.sh` passes it. Without
rgbds the runner falls back to an embedded 64-byte stub that writes the
documented post-boot register/I/O state and hands over to `0x0100` — the
same starting point as `gb-core` (DECISIONS.md D3). Verified on dmg-acid2:
both paths produce the reference image.

**Frame alignment.** SameBoy counts vblanks; `gb-core` counts 70 224-cycle
blocks from `PC=0100`. They agree to within ±1–2 frames, so `grade.py`
accepts an agent frame if its hash appears in the golden window
`[n−2, n+2]`. Input scripts should hold buttons for ≥ 10 frames so a
1–2 frame offset in when a press lands cannot change game state.

The SameBoy source tree and the built binary live under this directory
but are git-ignored.
