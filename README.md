# gb — a Game Boy emulator in Rust

<!-- operator-notes:start -->
> **This repository is a coding-agent showcase.** The README below is the
> one the agent under test reads (this note is removed from its copy). The
> showcase's design is in **[`harness/SHOWCASE.md`](harness/SHOWCASE.md)**; to
> run it, see [`harness/RUNNING.md`](harness/RUNNING.md). The task is
> [`GEP-0001.md`](GEP-0001.md), [`ISSUES/`](ISSUES/) and
> [`harness/AGENT_BRIEF.md`](harness/AGENT_BRIEF.md).
<!-- operator-notes:end -->

This repository is the `gb` emulator platform, handed over by the team
that built it so far. It is specified in **`GEP-0001.md`**: a Game Boy (DMG)
and Game Boy Color emulator core, developer tooling, embedded and
WebAssembly builds, and a web game library with an in-browser player. Most
of it is written; what is left is to finish it.

## What is left to do

* **Unfinished functions.** Some central functions were never finished:
  their signature and doc comment are in place, the body is `todo!(...)`
  (Rust) or `throw new Error("not implemented: ...")` (the player's
  JavaScript). `rg -n 'todo!\(|not implemented:' gb-*` lists them; each
  message says what the function must do and which part of GEP 1 specifies
  it.
* **The issue backlog** in `ISSUES/` — bug reports and requests from users,
  QA, developers and the product side. `ISSUES/README.md` explains how to
  record a resolution.

`GEP-0001.md` is the complete specification: requirements, acceptance
targets and the exact formats and APIs other teams test against. Read it
first. What it does not settle is decided by the product owner — ask (see
`TASK.md`) rather than guess.

A small window front-end (`gb-gui`) exists for demos.

## Layout

```
gb-core/            emulator library — no dependencies, no unsafe
  src/cpu/          SM83 core: registers, decode/execute
  src/mmu.rs        bus, address decoding, OAM DMA, CGB HDMA
  src/ppu.rs        graphics (DMG and CGB)
  src/apu.rs        audio
  src/timer.rs      DIV/TIMA
  src/interrupts.rs IF/IE
  src/joypad.rs     buttons
  src/serial.rs     link port, captures test-ROM output
  src/cartridge/    header parsing, MBC1/3/5
  src/emulator.rs   public facade — do not change its API
  src/util.rs       hashing, PGM output
  tests/rom_suite.rs  acceptance suite (harness code — do not modify)
gb-cli/             `gb` headless runner (harness code — do not modify)
gb-tools/           `gb-trace` (CPU trace, profiler) and `gb-server` (debugger API)
gb-wasm/            the core as a WebAssembly module (GEP 1 Appendix C)
gb-web/             game library service and JSON API; static/ is the web
                    front end and the in-browser player
gb-gui/             demo window (optional, not built by default)
ISSUES/             the issue backlog
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

`gb-oracle` (on the `PATH`) runs a ROM on SameBoy, a mature reference
emulator, with the same options and output as `gb`; use it to see what a
frame should look like or whether a Mooneye test should pass
(`gb-oracle --help`).

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
   are recorded, and each issue's resolution in its file in `ISSUES/`.
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
run. Expect most of it to fail until the CPU's and the PPU's unfinished
functions are written.

## Where to start

`GEP-0001.md`, then `DECISIONS.md`, then `ISSUES/README.md`, then
`gb-core/src/emulator.rs` to see how the pieces are called. The module doc comments say what each component owns and
which test ROMs exercise it.
