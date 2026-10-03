# Testing the emulator

Everything here runs offline. Expect most ROM suites to fail until the
unfinished functions in the CPU and the PPU are written; the point of this
file is that you never have to guess how to check your work.

## 1. Build and lint (must stay clean)

```sh
cargo build --release --workspace                         # -D warnings is on
cargo clippy --release --workspace --all-targets          # must report nothing
cargo fmt --all --check
```

## 2. Unit tests

```sh
cargo test --release --lib -p gb-core
```

## 3. Accuracy test ROMs (the acceptance suite)

```sh
cargo test --release --test rom_suite -- --nocapture            # all families
cargo test --release --test rom_suite -- --nocapture blargg     # CPU / timing (serial "Passed")
cargo test --release --test rom_suite -- --nocapture blargg_mem # audio (dmg_sound) + OAM bug, result in cart RAM
cargo test --release --test rom_suite -- --nocapture mooneye    # timers, interrupts, MBC (LD B,B protocol)
cargo test --release --test rom_suite -- --nocapture acid2      # PPU rendering (frame hash)
cargo test --release --test rom_suite -- --nocapture mooneye_cgb     # Game Boy Color (run with Model::Cgb)
cargo test --release --test rom_suite -- --nocapture blargg_mem_cgb  # cgb_sound
cargo test --release --test rom_suite -- --nocapture cgb_acid2       # CGB rendering (frame hash)
cargo test --release --test rom_suite -- --nocapture mealybug_dmg    # pixel-accurate PPU (frame hash at LD B,B)
```

Each ROM prints `PASS`, `FAIL` or `PANIC` with a reason. A `PANIC` is a
bug in the emulator (an unimplemented stub or an out-of-range index), not
an unsupported ROM. `roms/README.md` explains each family's pass signal.

Running a single ROM by hand, e.g. to see Blargg's serial output:

```sh
cargo run --release -p gb-cli -- --rom roms/test/blargg/cpu_instrs/cpu_instrs.gb --frames 7200 --serial-stdout
cargo run --release -p gb-cli -- --rom roms/test/mooneye/acceptance/timer/div_write.gb --frames 1200 --mooneye
cargo run --release -p gb-cli -- --rom "roms/test/blargg-mem/dmg_sound/rom_singles/01-registers.gb" --frames 7200 --blargg-mem
cargo run --release -p gb-cli -- --rom roms/test/acid2/dmg-acid2.gb --frames 120 --dump-frame acid2.pgm --hash
# compare the printed hash with roms/test/acid2/expected.fnv; open acid2.pgm in any image viewer
```

## 4. Games

```sh
cargo run --release -p gb-cli -- --rom roms/games/tobudx.gb --frames 3600 --dump-every 300 --dump-dir frames/ --hash
```

Look at the dumped frames: does the title screen render, does scripted
input start the game? Input scripts: `gb --help` describes the format.

## 5. Determinism and save states

```sh
# same ROM twice → identical "final" hash
cargo run --release -p gb-cli -- --rom roms/games/2048.gb --frames 1800 --hash
cargo run --release -p gb-cli -- --rom roms/games/2048.gb --frames 1800 --hash
# run 600, save; run 1200 → H1.  load state, run 600 → H2.  H1 == H2
cargo run --release -p gb-cli -- --rom roms/games/2048.gb --frames 600 --save-state s.state
cargo run --release -p gb-cli -- --rom roms/games/2048.gb --frames 1200 --hash
cargo run --release -p gb-cli -- --rom roms/games/2048.gb --frames 600 --load-state s.state --hash
```

## 6. Performance sanity

`cpu_instrs` should finish in well under a minute in release mode. If a
single ROM takes minutes, something is wrong with the main loop.

## 7. Game Boy Color games

```sh
cargo run --release -p gb-cli -- --rom roms/games-cgb/ucity.gbc --model cgb --frames 1800 --dump-every 300 --dump-dir frames/
```

Frames are dumped as PPM (colour). Dual-mode games in `roms/games/` should
also run with `--model cgb`.

## 8. The other deliverables (GEP 1 §5–§8)

There are no ready-made tests for these — the GEP's appendices are the
contract, and you are expected to write your own checks against them:

* **`gb-trace`** — `docs/specs/trace-example-01-special.txt` is the first
  2 000 lines of the reference trace for `cpu_instrs/individual/01-special.gb`
  (`gb-trace --doctor`); diff your output against it.
* **`gb-server`, `gb-web`** — drive them with `curl` or a script.
* **`gb-wasm`** — Node.js 18 is installed; GEP 1 Appendix C has a minimal
  host. Compare its hash with `gb --hash`.
* **Front end** — Chromium is installed: `chromium --headless=new
  --no-sandbox --remote-debugging-port=9222 http://127.0.0.1:8080/` and
  Python's `websocket` module (`import websocket`) let you script it over
  the DevTools protocol; or `--dump-dom` / `--screenshot` for quick looks.
* **`no_std`** — `cargo build -p gb-core --release --no-default-features
  --target thumbv7em-none-eabihf`.

## 9. Comparing with the reference emulator

`gb-oracle` runs the same ROM on SameBoy with `gb`'s options and output, so
a difference between the two is a lead:

```sh
gb-oracle --rom roms/games/tobudx.gb --frames 600 --hash
cargo run --release -p gb-cli -- --rom roms/games/tobudx.gb --frames 600 --hash
gb-oracle --rom roms/games-cgb/ucity.gbc --model cgb --frames 600 --dump-frame ref.ppm
gb-oracle --rom roms/test/mooneye/acceptance/timer/div_write.gb --frames 1200 --mooneye
```

The two count frames differently, so the same picture can be a frame or
two apart: compare `gb --dump-every 1` frames against
`gb-oracle --hashes` over a small window rather than one frame number.

## 10. Issues

Reproduce an issue before changing code for it, and keep the reproduction
as a test or a script so it stays fixed. Record the resolution in the
issue's file (`ISSUES/README.md`).
