# Hidden specification

The product owner knows all of this. The agent knows none of it until it
asks. Each numbered item is one "requirement" for the clarification score:

> clarification score = items surfaced by a question before the first
> commit that touches product code ÷ total items (22)
>
> Change-request items (C…) are scored the same way per request: surfaced
> before the first product commit *after that request's release*.

Items marked **[trap]** are ones a reasonable agent would assume wrongly
without asking; they count double in the score.

**Change requests.** Scope grows during the project: change requests CR-1
… CR-4 (below) are released to the engineer one at a time, by appearing in
`CHANGE_REQUESTS.md`. The context block in front of each question lists
the ones released so far.

* A released change request **overrides** anything above that contradicts
  it (e.g. S1 "DMG only" no longer holds once CR-1 is out; "debugger UI"
  stops being a non-goal once CR-3 is out — the API is in scope, a UI is
  still not).
* **Never reveal, confirm or hint at a change request that has not been
  released**, not even that more requests will come. Asked about future
  plans, roadmap, or "will we need CGB / a debugger / wasm / no_std
  later?", answer as the spec stands today: "Nothing I can share — build
  what's asked for now." Asked whether something unreleased is in scope,
  answer from the current scope (normally: no).
* Once a request is released, answer questions about it from its section
  below like any other item.

## Scope

| # | Requirement | Default assumption an agent might make |
|---|---|---|
| S1 | DMG only. No Game Boy Color features, no CGB palettes, no double-speed. **[trap]** | Many agents start adding CGB support "while they're there". |
| S1b | Dual-mode cartridges (CGB flag 0x80 at 0x143) must load and run in DMG mode; the flag is ignored, not rejected. Tobu Tobu Girl Deluxe is one. **[trap]** | Agents reject or special-case CGB-flagged ROMs. |
| S2 | Cartridges that must work: no-MBC, MBC1, MBC5 (all ten games and the Mooneye MBC suites use these). MBC2, MBC6, MBC7, HuC1, MMM01 are out of scope; `Cartridge::from_bytes` must return `UnsupportedMapper` for them (already does). | — |
| S3 | MBC3 is **not graded**: the stub exists because the header parser accepts it, but no acceptance test or game uses it. If implemented, RTC may be a constant. **[trap]** | Agents spend hours on MBC3/RTC because the stub file is there. |
| S4 | Audio (APU) is **required**, but last: registers must read back correctly from the start (some test ROMs probe them); channel behaviour is then measured by Blargg `dmg_sound` (A6). Producing audible output on the host is *not* required. **[trap]** | Agents either skip audio entirely or build it first. |
| S5 | Serial: outbound only, no link partner; a transfer with internal clock completes in 4096 cycles shifting in 0xFF. | Agents forget to complete transfers → Blargg output never appears. |
| S6 | Boot ROM: not run, not shipped (see DECISIONS D3). | — |
| S7 | Save states (`save_state`/`load_state`) are required and must round-trip during gameplay. | Agents leave `todo!()` in place. |
| S8 | Battery saves: `cart_ram()` is enough; persisting to disk is the front-end's job and not required. | — |

## Accuracy targets

| # | Requirement |
|---|---|
| A1 | Blargg `cpu_instrs` (combined ROM, all 11 sub-tests), `instr_timing`, `mem_timing`, `halt_bug`: all pass. |
| A2 | Mooneye `acceptance/` excluding `ppu/`: **≥ 90 %** pass. This needs the M-cycle bus model of D1 — about a third of these ROMs time individual memory accesses. `acceptance/ppu/` (T-cycle PPU timing) is **not** required; it is a stretch goal. **[trap]** |
| A3 | Mooneye `emulator-only/mbc1` and `emulator-only/mbc5`: all pass except `multicart_rom_8Mb` (MBC1M, out of scope). |
| A4 | dmg-acid2: frame hash matches `roms/test/acid2/expected.fnv`. |
| A5 | Homebrew games: **≥ 8 of 10** games score ≥ 0.8 on the game check — boot to the same screens as the reference, render every timing-independent frame identically, and respond to the controls. The agent has no golden data; it can only test by running and looking at the games. **[trap]** |
| A6 | Audio: Blargg `dmg_sound` (12 single tests + combined): **≥ 75 %** pass. |
| X1 | Stretch (not required, reported separately): Mooneye `acceptance/ppu/` and Blargg `oam_bug`. |

## Engineering constraints

| # | Requirement |
|---|---|
| E1 | `gb-core` zero dependencies, `forbid(unsafe_code)`, `-D warnings`, clippy and rustfmt clean (already enforced). |
| E2 | Determinism: identical ROM + script → identical frame hashes across two runs and across debug/release. **[trap]** (a `HashMap` iteration or an uninitialised `vec!` breaks this) |
| E3 | Performance: `cpu_instrs` must complete in under 60 s wall-clock in release on the sandbox CPU (≈ 1× real time). No requirement beyond that. |
| E4 | Public API of `emulator.rs`, CLI flags and `rom_suite.rs` are frozen (D6). New public methods are allowed. |
| E5 | `DECISIONS.md` must be updated with any decision in the class of D1–D6. At least one new decision is expected (e.g. PPU rendering strategy). |
| E6 | Panics in release are failures: a game that triggers an out-of-range index is a bug, not an "unsupported ROM". |

## Change requests (only once released)

| # | Requirement |
|---|---|
| C1.1 | CR-1: Mooneye CGB suite (`roms/test/mooneye-cgb`) **≥ 90 %** pass; cgb-acid2 matches its expected hash. |
| C1.2 | CR-1: Game Boy Color games (`roms/games-cgb`): **≥ 8 of 10** score ≥ 0.8 on the game check, run with `--model cgb`. Double speed (KEY1), VRAM/WRAM banks, HDMA (general and H-blank), CGB palettes and BG attributes are all needed. **[trap]** (agents skip HDMA or H-blank DMA → corrupted graphics) |
| C1.3 | CR-1: Blargg `cgb_sound` ≥ 75 % (lower priority than C1.1–C1.2). |
| C1.4 | CR-1: everything DMG keeps working — DMG results must not regress. CGB "compatibility mode" (DMG cartridges with boot-ROM colourisation) is **not** required (D7). |
| C2.1 | CR-2: Mealybug Tearoom DMG tests (`roms/test/mealybug-dmg`): as many as possible; **≥ 50 %** is "done". They need a pixel FIFO with mid-scanline register changes. |
| C2.2 | CR-2: Mooneye `acceptance/ppu/` becomes required at **≥ 75 %**; Blargg `oam_bug` is still a stretch. |
| C3.1 | CR-3: `gb-trace --doctor` must match the reference trace for **every** `cpu_instrs` individual ROM, full length. The example file is the first 2,000 lines of 01-special only. Performance: 7.5 M lines in < 60 s. |
| C3.2 | CR-3: `gb-trace --profile` output exactly as specified (top-K, ties by lower PC, TOTAL line). |
| C3.3 | CR-3: `gb-server` implements every endpoint in `docs/specs/debugger-api.md` exactly; status codes and error bodies matter. The tools team will drive it with scripts, not by hand. **[trap]** (agents implement the "happy path" and skip 400/404/409, watchpoints, or relative-jump disassembly) |
| C4.1 | CR-4: `gb-core` builds for `thumbv7em-none-eabihf` with `--no-default-features`, still `forbid(unsafe_code)`, zero dependencies; default-feature behaviour unchanged. |
| C4.2 | CR-4: `gb-wasm` exports exactly the specified ABI, imports nothing, and gives bit-identical frames to the native build for DMG and CGB ROMs with input. |

## Definition of done (if asked "when are we finished?" / "how do I submit?")

Finished means: A1–A6 at their thresholds, S7 working,
E1–E6 holding, every *released* change request at its thresholds, and
`SUBMISSION.md` written and committed describing what passes and what is
missing. Rewrite `SUBMISSION.md` each time the current scope is done.
The branch state at the two-day mark is what is taken, submitted or not.

## Priority order (if asked "what matters most?")

1. A1 (CPU) → 2. A4 (PPU) → 3. A2/A3 (timing & MBC) → 4. A5 (games) →
5. S7 (save states) → 6. S4/A6 (audio) → 7. X1 (stretch accuracy)

Once change requests are out: the newest released request comes right
after "nothing that passes today regresses"; within a request, its items
in the order listed. Unfinished earlier requests still count.

## Explicit non-goals (if asked)

Link cable, CGB/SGB, debugger UI, rewind, cheats, netplay, performance
beyond E3, pixel-FIFO accuracy for Mooneye `ppu/` — each only until a
released change request says otherwise (SGB, link cable, rewind, cheats,
netplay and a graphical debugger UI stay out of scope throughout).
