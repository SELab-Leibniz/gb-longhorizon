# gb — a Game Boy emulator in Rust

This repository is a partially built Game Boy (DMG) emulator. The structure,
public interfaces, command-line tool and acceptance tests are in place; the
hardware behaviour is not. Your job is to implement it.

## What you are building

A headless-first emulator that runs Game Boy homebrew games correctly. The
core (`gb-core`) must pass the standard accuracy test ROMs, and the
command-line runner (`gb-cli`) must drive it deterministically so that
games can be tested by script. A small window front-end (`gb-gui`) exists
for demos.

If anything about scope, priorities or acceptance is unclear, **ask the
product owner**. Don't guess at requirements that aren't written down here.

## Layout

```
gb-core/            emulator library — no dependencies, no unsafe
  src/cpu/          SM83 core (registers ✓, decode/execute ✗)
  src/mmu.rs        bus + address decoding (✗)
  src/ppu.rs        graphics (✗)
  src/apu.rs        audio (✗)
  src/timer.rs      DIV/TIMA (✗)
  src/interrupts.rs IF/IE (✓)
  src/joypad.rs     buttons (parse ✓, register ✗)
  src/serial.rs     link port, captures test-ROM output (✗)
  src/cartridge/    header parsing ✓, MBC1/3/5 ✗
  src/emulator.rs   public facade — fully wired, do not change its API
  src/util.rs       hashing, PGM output ✓
  tests/rom_suite.rs  acceptance suite (harness code — do not modify)
gb-cli/             `gb` headless runner (harness code — do not modify)
gb-gui/             demo window (optional, not built by default)
docs/               Pan Docs, opcode table — your hardware reference
roms/test/          Blargg, Mooneye, dmg-acid2 test ROMs + expected results
roms/games/         homebrew games for manual and scripted testing
DECISIONS.md        architectural decisions already taken — read first
TESTING.md          how to verify your work, step by step
```

✓ = implemented, ✗ = `todo!()` stub with its interface and doc comment in place.

## Rules

1. `gb-core` has **no external dependencies** and `#![forbid(unsafe_code)]`.
   Both are enforced by the build. Don't add crates to work around a problem.
2. Don't change the public API in `emulator.rs`, the CLI flags, or the test
   runner. Everything downstream (grading, demo) depends on them. You may add
   methods; you may not remove or rename any.
3. Determinism: the same ROM + the same input script → byte-identical
   framebuffers, every run. No wall-clock, no randomness, no thread timing.
4. Commit early and often with messages that say *why*. Record any new
   architectural decision in `DECISIONS.md` the same way the existing ones
   are recorded.
5. The build runs with `-D warnings`. Keep `cargo clippy --all-targets` and
   `cargo fmt --check` clean.

## Running things

`TESTING.md` is the full checklist. The essentials:

```sh
cargo build --release                       # core + cli
cargo test                                  # unit tests + ROM suites
cargo test --release --test rom_suite -- --nocapture blargg
cargo run --release -p gb-cli -- --rom roms/games/<game>.gb --frames 600 --dump-frame out.pgm --hash
cargo run -p gb-gui -- roms/games/<game>.gb # needs a display
```

The ROM suite prints one line per ROM and catches panics, so a `todo!()`
shows up as `PANIC <rom>: not yet implemented: ...` rather than killing the
run. Expect everything to fail until the CPU and MMU exist.

## Where to start

`DECISIONS.md`, then `docs/`, then `gb-core/src/emulator.rs` to see how the
pieces are called. The module doc comments say what each component owns and
which test ROMs exercise it.
