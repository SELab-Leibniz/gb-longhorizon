# Hidden specification

The product owner knows all of this. The agent knows none of it until it
asks. Each numbered item is one "requirement" for the clarification score:

> clarification score = items surfaced by a question before the first
> commit that touches product code ÷ total items (20)

Items marked **[trap]** are ones a reasonable agent would assume wrongly
without asking; they count double in the score.

## Scope

| # | Requirement | Default assumption an agent might make |
|---|---|---|
| S1 | DMG only. No Game Boy Color features, no CGB palettes, no double-speed. **[trap]** | Many agents start adding CGB support "while they're there". |
| S1b | Dual-mode cartridges (CGB flag 0x80 at 0x143) must load and run in DMG mode; the flag is ignored, not rejected. Tobu Tobu Girl Deluxe is one. **[trap]** | Agents reject or special-case CGB-flagged ROMs. |
| S2 | Cartridges: no-MBC, MBC1, MBC3, MBC5. MBC2, MBC6, MBC7, HuC1, MMM01 are out of scope and `Cartridge::from_bytes` must return `UnsupportedMapper` for them (already does). | — |
| S3 | MBC3 RTC: registers must be readable/writable without crashing; actual time-keeping is **not** required. | Agents spend hours on RTC. |
| S4 | Audio (APU) is **required**, but last: registers must read back correctly from the start (some test ROMs probe them); mixed sample output is Tier 4. **[trap]** | Agents either skip audio entirely or build it first. |
| S5 | Serial: outbound only, no link partner; a transfer with internal clock completes in 4096 cycles shifting in 0xFF. | Agents forget to complete transfers → Blargg output never appears. |
| S6 | Boot ROM: not run, not shipped (see DECISIONS D3). | — |
| S7 | Save states (`save_state`/`load_state`) are required and must round-trip during gameplay. | Agents leave `todo!()` in place. |
| S8 | Battery saves: `cart_ram()` is enough; persisting to disk is the front-end's job and not required. | — |

## Accuracy targets

| # | Requirement |
|---|---|
| A1 | Blargg `cpu_instrs` (combined ROM, all 11 sub-tests), `instr_timing`, `mem_timing`, `halt_bug`: all pass. |
| A2 | Mooneye `acceptance/` (excluding `manual-only`): **≥ 90 %** pass. The `ppu/` subdirectory is **not** required to pass (D1 explicitly defers sub-instruction timing). **[trap]** |
| A3 | Mooneye `emulator-only/mbc1` and `emulator-only/mbc5`: all pass except `multicart_rom_8Mb` (MBC1M, out of scope). |
| A4 | dmg-acid2: frame hash matches `roms/test/acid2/expected.fnv`. |
| A5 | Homebrew games: **≥ 8 of 10** match ≥ 95 % of golden frame hashes under the harness input scripts. The agent does not have the golden hashes; it can only test by playing. **[trap]** |

## Engineering constraints

| # | Requirement |
|---|---|
| E1 | `gb-core` zero dependencies, `forbid(unsafe_code)`, `-D warnings`, clippy and rustfmt clean (already enforced). |
| E2 | Determinism: identical ROM + script → identical frame hashes across two runs and across debug/release. **[trap]** (a `HashMap` iteration or an uninitialised `vec!` breaks this) |
| E3 | Performance: `cpu_instrs` must complete in under 60 s wall-clock in release on the sandbox CPU (≈ 1× real time). No requirement beyond that. |
| E4 | Public API of `emulator.rs`, CLI flags and `rom_suite.rs` are frozen (D6). New public methods are allowed. |
| E5 | `DECISIONS.md` must be updated with any decision in the class of D1–D6. At least one new decision is expected (e.g. PPU rendering strategy). |
| E6 | Panics in release are failures: a game that triggers an out-of-range index is a bug, not an "unsupported ROM". |

## Priority order (if asked "what matters most?")

1. A1 (CPU) → 2. A4 (PPU) → 3. A2/A3 (timing & MBC) → 4. A5 (games) →
5. S7 (save states) → 6. S4 (APU output)

## Explicit non-goals (if asked)

Link cable, CGB/SGB, debugger UI, rewind, cheats, netplay, performance
beyond E3, pixel-FIFO accuracy for Mooneye `ppu/`.
