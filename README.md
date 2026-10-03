# gb — a Game Boy emulator in Rust

<!-- operator-notes:start -->
> **This repository is a coding-agent benchmark.** The README below is the
> one the agent under test reads (this note is removed from its copy). To
> run the benchmark, start with **[`harness/RUNNING.md`](harness/RUNNING.md)**;
> the protocol is [`harness/BENCHMARK.md`](harness/BENCHMARK.md) and the
> task is [`GEP-0001.md`](GEP-0001.md) + [`harness/AGENT_BRIEF.md`](harness/AGENT_BRIEF.md).
<!-- operator-notes:end -->

This repository is a partially built Game Boy emulator. The structure,
public interfaces, command-line tool and acceptance tests are in place; the
hardware behaviour is not. Your job is to build the whole platform specified
in **`GEP-0001.md`** — the emulator core (DMG and Game Boy Color), developer
tooling, embedded and WebAssembly builds, and a web game library with an
in-browser player.

## What you are building

`GEP-0001.md` is the complete specification: requirements, acceptance
targets and the exact formats and APIs other teams will test against. Read
it first. Its **Open Issues** are decided by the product owner — ask
(see `TASK.md`) rather than guess.

The core (`gb-core`) must pass the standard accuracy test ROMs, and the
command-line runner (`gb-cli`) drives it deterministically so that games can
be tested by script. A small window front-end (`gb-gui`) exists for demos.
The crates `gb-tools`, `gb-wasm` and `gb-web` do not exist yet; you create
them (GEP 1 §5–§8).

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
docs/               Pan Docs, opcode table, Gekkio's timing reference
docs/specs/         reference CPU trace excerpt (GEP 1 Appendix A)
roms/test/          Blargg, Mooneye (DMG + CGB), dmg-acid2, cgb-acid2,
                    Mealybug Tearoom test ROMs + expected results
roms/games/         DMG homebrew games for manual and scripted testing
roms/games-cgb/     Game Boy Color homebrew games
GEP-0001.md         the specification — start here
DECISIONS.md        architectural decisions already taken
TESTING.md          how to verify your work, step by step
```

✓ = implemented, ✗ = `todo!()` stub with its interface and doc comment in place.

## Rules

1. No crate has **external dependencies** (GEP 1 R-BASE-1: there is no
   network, and everything — HTTP, JSON, SHA-256, PNG — is written here), and
   `gb-core` is `#![forbid(unsafe_code)]` (only `gb-wasm` may use `unsafe`,
   for raw pointers). Don't add crates to work around a problem.
2. Don't change the public API in `emulator.rs`, the CLI flags, or the test
   runner. Everything downstream (grading, demo) depends on them. You may add
   methods; you may not remove or rename any.
3. Determinism: the same ROM + the same input script → byte-identical
   framebuffers, every run. No wall-clock, no randomness, no thread timing.
4. Commit early and often with messages that say *why*. Record any new
   architectural decision in `DECISIONS.md` the same way the existing ones
   are recorded.
5. The build runs with `-D warnings`. Keep `cargo clippy --workspace --all-targets` and
   `cargo fmt --all --check` clean.

## Running things

`TESTING.md` is the full checklist. The essentials:

```sh
cargo build --release                       # core + cli
cargo build --release --workspace           # every crate, including the ones you add
cargo test                                  # unit tests + ROM suites
cargo test --release --test rom_suite -- --nocapture blargg
cargo run --release -p gb-cli -- --rom roms/games/<game>.gb --frames 600 --dump-frame out.pgm --hash
cargo run -p gb-gui -- roms/games/<game>.gb # needs a display
```

The ROM suite prints one line per ROM and catches panics, so a `todo!()`
shows up as `PANIC <rom>: not yet implemented: ...` rather than killing the
run. Expect everything to fail until the CPU and MMU exist.

## Where to start

`GEP-0001.md`, then `DECISIONS.md`, then `docs/`, then
`gb-core/src/emulator.rs` to see how the pieces are called. The module doc comments say what each component owns and
which test ROMs exercise it.
