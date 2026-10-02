# Testing the emulator

Everything here runs offline. Expect all ROM suites to fail until the CPU
and MMU exist; the point of this file is that you never have to guess how
to check your work.

## 1. Build and lint (must stay clean)

```sh
cargo build --release          # -D warnings is on
cargo clippy --all-targets     # must report nothing
cargo fmt --check
```

## 2. Unit tests

```sh
cargo test --release --lib -p gb-core
```

## 3. Accuracy test ROMs (the acceptance suite)

```sh
cargo test --release --test rom_suite -- --nocapture            # all three families
cargo test --release --test rom_suite -- --nocapture blargg     # CPU / timing (serial "Passed")
cargo test --release --test rom_suite -- --nocapture mooneye    # timers, interrupts, MBC (LD B,B protocol)
cargo test --release --test rom_suite -- --nocapture acid2      # PPU rendering (frame hash)
```

Each ROM prints `PASS`, `FAIL` or `PANIC` with a reason. A `PANIC` is a
bug in the emulator (an unimplemented stub or an out-of-range index), not
an unsupported ROM. `roms/README.md` explains each family's pass signal.

Running a single ROM by hand, e.g. to see Blargg's serial output:

```sh
cargo run --release -p gb-cli -- --rom roms/test/blargg/cpu_instrs/cpu_instrs.gb --frames 7200 --serial-stdout
cargo run --release -p gb-cli -- --rom roms/test/mooneye/acceptance/timer/div_write.gb --frames 1200 --mooneye
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
