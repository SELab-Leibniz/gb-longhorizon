# The v2 showcase: taking over an unfinished project

This branch (`showcase-v2`) turns the GEP 1 task into a **showcase** of how a
coding agent handles a long, realistic engineering job. It is not a
benchmark: one run per agent, no repeated trials, and the technique
diagnostics are reported next to the score, never added to it. For the
mechanics of running it (Harbor, keys, monitoring), see `RUNNING.md`.

## Why a v2

The 6-hour pilot on `full-spec` (`reports/2026-10-03-pilot-icode.md`)
showed that the from-scratch task is too easy for 48 hours. iCode reached
0.71 in 2 hours and 0.76 in 4.3 hours, then plateaued. The breadth
deliverables (tooling, WebAssembly, web library, front end) took about an
hour with sub-agents, and the model clearly knows how a Game Boy works.

So v2 starts from that pilot's own output and asks for the work that a
model's pretraining helps least with:
- finishing code it did not write;
- finding bugs from symptom reports;
- asking the right questions;
- staying correct over a long run.

## What the agent gets

| Ingredient | What it is | Technique it exercises |
|---|---|---|
| **Inherited codebase** | the pilot's final code (commit `d235fa5`) with 32 central Rust functions and 5 player functions reduced to signature + doc comment + `todo!("…")` | code localization: read and extend code it did not write |
| **13 planted bugs** | one-line, realistic bugs in code that was kept; they build, pass clippy, and break no inherited unit test | code localization: get from a symptom to the cause |
| **`ISSUES/`** | 21 reports in user / QA / developer / product voice, symptoms only (13 bugs, 3 underspecified requests, 3 requests the product owner declines, 2 asking for behaviour GEP 1 rules out) | requirement clarification, memory: a backlog to track over 48 h |
| **Product owner** | answers `## Q:` questions with the decisions in `HIDDEN_SPEC.md`; never diagnoses bugs, never volunteers decisions | requirement clarification |
| **`gb-oracle`** | SameBoy (pinned commit) with the `gb` CLI's options and output, inside the sandbox | agent verifier: differential testing against a reference |
| **48 h, continuous** | one session, restarted with "continue" if it stops; no resets | memory and context compaction |

GEP 1 is unchanged except that its Open Issues are now **Resolved Issues**,
with the answers the previous team got. The inherited code implements
them, and three of the declined requests rest on them. GEP 1 also now
defines `--model auto`.

### The stubbed functions

| Area | Functions |
|---|---|
| CPU | `dispatch` (all unprefixed opcodes), `execute_cb`; CPU `save_state` / `load_state` |
| PPU | `tick` (mode state machine, STAT/LY/VBlank), `render_scanline`, `bg_pixel`, `window_pixel`, `render_sprites`, `save_state`, `load_state` |
| Bus | `start_oam_dma`, `tick_dma`, `hdma_write`, `hdma_transfer_block`, `save_state`, `load_state` |
| Timer | `write`, `tick_t`, `increment_tima`, `save_state`, `load_state` |
| Tooling | the disassembler; `gb-server`'s `core_step` and `/run` |
| WebAssembly | `gb_load`, `gb_run_frames`, `refresh_frame` |
| Library | upload, list, screenshot handlers; headless screenshot rendering |
| Player (JS) | `loop`, `step`, `drawFrame`, `frameHash`, `runFrames` |

`harness/showcase/stub.py` regenerates them from `d235fa5`. `rsfns.py` is
its Rust function finder; `fixwarn.py` finds the helpers that the stubs
leave unused. Each crate root has one explained `#![allow(dead_code)]`.

### The planted bugs (operators only)

| Bug | Where | Issue | Hidden check |
|---|---|---|---|
| B01 MBC5's 9th ROM-bank bit lands on bit 7 | `cartridge/mbc5.rs` | #103 | T01: 8 MiB MBC5 ROM with numbered banks, bank select via `gb-server` |
| B02 ADC half-carry ignores the carry in | `cpu/opcodes.rs` | #111 | T02: Blargg `cpu_instrs` 04 + 09 |
| B03 DAA corrects at `$99` instead of above it | `cpu/opcodes.rs` | #105 | T03: Blargg `cpu_instrs` 01 |
| B04 interrupt priority reversed | `interrupts.rs` | #114 | T04: crafted ROM, PC after dispatch via `gb-server` |
| B05 BCPS auto-increment wraps at 32 | `ppu.rs` | #101 | T05: palette write/read-back via `gb-server` |
| B06 Adler-32 modulus 65520 | `gb-web/src/png.rs` | #108 | T06: zlib-decode a screenshot |
| B07 title keeps byte `$7F` | `gb-web/src/store.rs` | #117 | T07: upload with `$7F` in the title |
| B08 profiler breaks ties by descending PC | `gb-tools/src/bin/gb-trace.rs` | #120 | T08: reference profile of `06-ld r,r` |
| B09 debugger reads saturate at `$FFFF` | `gb-tools/src/bin/gb-server.rs` | #112 | T09: crafted ROM, read across `$FFFF` |
| B10 WebAssembly A/B bits swapped | `gb-wasm/src/lib.rs` | #106 | T10: joypad→BGP ROM in Node vs `gb` |
| B11 MBC1 bank-0 rule applied after combining | `cartridge/mbc1.rs` | #118 | T11: Mooneye `mbc1/rom_1Mb`, `rom_2Mb` |
| B12 save-size check accepts longer bodies | `gb-web/src/main.rs` | #115 | T12: `PUT` of `ram_size + 1` bytes |
| B13 `auto` picks CGB only for `$C0` | `emulator.rs` | #109 | T13: `tobudx.gb` with `auto` vs `cgb` |

`harness/showcase/bugs.py` applies them. Every issue's text was checked
against a build with its bug: each claim in it is true of that bug.

### The decisions (operators only)

| Check | Issue | Kind | Passes when |
|---|---|---|---|
| P1 | #107 search by cartridge type | must ask | `q` also matches the mapper |
| P2 | #119 save download name | must ask | `Content-Disposition: attachment; filename="<stem>.sav"` |
| P3 | #102 statistics per mapper | must ask | `by_mapper` object in `/api/stats` |
| P4 | #110 rename by re-upload | declined | re-upload still `409`, entry unchanged |
| P5 | #116 newest first | declined (default) | default order still title-ascending |
| P6 | #121 ROMs over 8 MB | declined | 8 MiB + 1 byte still `413` |
| P7 | #104 Japanese titles | ruled out by OI-5 | katakana bytes still `?` |
| — | #113 colour correction | ruled out by GEP 1 | regular screenshot / player colour checks |

## Scoring

| Area | Weight |
|---|---|
| Emulator core (DMG) | 25 % |
| Game Boy Color | 12 % |
| Pixel-accurate PPU | 8 % |
| Tooling | 7 % |
| Portability | 4 % |
| Game library service | 7 % |
| Front end and player | 5 % |
| Backlog bugs (T01–T13) | 20 % |
| Backlog requests (P1–P7) | 10 % |
| clippy + rustfmt | 2 % |

Calibration through the real verifier (`test.sh` in the task image):

| Tree | Reward |
|---|---|
| v2 starting point (stubbed + bugged) | **0.056** |
| The pilot's code + every bug fixed + decisions implemented | **0.762** (tickets 1.0, decisions 1.0; lint 0 only because the test patch was not rustformatted; measured before `player_input` was decoupled from game logic, which now passes too: about 0.765) |

The ceiling is well above 0.76. What separates the two is the accuracy tail
the pilot never closed: acid2, CGB games, Mealybug and sound.

## Diagnostics (reported, not scored)

`scripts/summarize.py` adds a table per trial:

| Technique | Evidence |
|---|---|
| Requirement clarification | which of the 8 decision issues the agent asked about, and whether before its first change to `gb-web` (product owner log) |
| Code localization | bugs fixed (T checks); whether the planted line itself changed, and when (`git log -S` on the trial's `repo.bundle`) |
| Memory | notes/plan files the agent created and how often it updated them; issues with a `## Resolution` |
| Agent verifier | test files and scripts added; `gb-oracle` calls |
| Context compaction | compactions and sub-agents in the agent's own logs |

## Validating the showcase

```sh
git archive d235fa5 | tar -x -C /tmp/ref           # the unbugged, unstubbed reference
harness/showcase/validate_tickets.sh /tmp/ref harness/harbor/task/tests /tmp/val
```

The script builds the reference in the task image and runs the hidden
checks on four kinds of variant:
- `ref` (no bugs);
- `ref+Bxx` (each bug alone);
- `all-Bxx` (every bug but one);
- `ref+decisions` (`decisions_patch.py`, a reference implementation of P1–P3).

Validated result:
- every T check passes on `ref`, fails on `ref+` its own bug only, and passes on `all-` its own bug;
- `ref+decisions` passes everything;
- `ref` passes all but P1–P3, the new features.

So fixing a bug earns its check whatever else is still broken.

## Files

| Path | What |
|---|---|
| `ISSUES/` | the backlog (agent-visible) |
| `harness/HIDDEN_SPEC.md` | the product owner's decisions |
| `harness/showcase/bugs.py` | the planted bugs |
| `harness/showcase/tickets_conformance.py` | hidden checks T01–T13, P1–P7 (staged into the verifier by `sync.sh`) |
| `harness/showcase/decisions_patch.py`, `validate_tickets.sh` | validation |
| `harness/showcase/stub.py`, `rsfns.py`, `fixwarn.py` | how the stubs were made |
| `harness/oracle/gb-oracle` | the reference CLI (built into the image with `harness/ref/`) |
