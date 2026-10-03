# The v2 showcase: taking over an unfinished project

This branch (`showcase-v2`) turns the GEP 1 task into a **showcase** of how a
coding agent handles a long, realistic engineering job. It is not a
benchmark: one run per agent, no repeated trials, and the technique
diagnostics are reported next to the score, never added to it. For the
mechanics of running it (Harbor, keys, monitoring), see `RUNNING.md`.

## Why a v2

The 6-hour pilot on `full-spec` (`reports/2026-10-03-pilot-icode.md`)
showed that the from-scratch task is too easy for 48 hours. iCode reached
0.71 in 2 hours and 0.76 in 4.3 hours, then plateaued; the model clearly
knows how a Game Boy works. v2 therefore starts from that pilot's own
output and asks for the work that pretraining helps least with:
- finishing code it did not write;
- finding bugs from symptom reports;
- asking the right questions;
- keeping track of a backlog that keeps growing;
- staying correct over a long run.

The first smoke test of v2 (`reports/2026-10-03-smoke-v2.md`) showed the
first bug set was found too easily: the bugs sat next to the stubs and gave
themselves away. That led to the current design:
- bugs that the standard test suites do not catch, outside stub files, with
  symptoms far from the cause;
- a backlog filed in waves over the run.

## What the agent gets

| Ingredient | What it is | Technique it exercises |
|---|---|---|
| **Inherited codebase** | the pilot's final code (commit `d235fa5`) with 32 central Rust functions and 5 player functions reduced to signature + doc comment + `todo!("…")` | code localization: read and extend code it did not write |
| **14 planted bugs** | realistic one-line bugs in code that was kept, none in a file that holds a stub; they build, pass clippy, break no inherited unit test, and no comment states the correct behaviour next to them | code localization: get from a symptom to the cause |
| **A backlog filed in waves** | 24 issues in user / QA / developer / product voice, symptoms only. 11 at the start (`ISSUES/`), then 4, 4, 3 and 2 filed at 4, 12, 24 and 36 h (earlier if the agent reports the work complete) | requirement clarification; memory: a backlog that keeps changing over 48 h |
| **Product owner** | answers `## Q:` questions with the decisions in `HIDDEN_SPEC.md`, knows only the waves filed so far, never diagnoses bugs, never volunteers decisions | requirement clarification |
| **`gb-oracle`** | SameBoy (pinned commit) with the `gb` CLI's options and output, inside the sandbox | agent verifier: differential testing against a reference |
| **48 h, continuous** | one session, restarted with "continue" if it stops; no resets | memory and context compaction |

GEP 1 is unchanged except that its Open Issues are now **Resolved Issues**,
with the answers the previous team got. The inherited code implements
them, and four of the backlog requests are declined on their basis. GEP 1
also now defines `--model auto`.

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

### The backlog and the planted bugs (operators only)

Every bug is in the code from the start; its issue is filed in the wave
shown. An agent can find a bug before it is reported.

| Wave | Issue | Kind | Cause (planted) | Hidden check |
|---|---|---|---|---|
| 0 | #101 `--model auto` runs dual-mode games in black and white | bug B13 | `Model::for_rom` picks CGB only for `$C0` | T13: `tobudx.gb` `auto` vs `cgb` |
| 0 | #102 MBC1 reads the wrong ROM bank | bug B11 | bank-0 rule applied after combining the bank bits | T11: Mooneye `mbc1/rom_1Mb`, `rom_2Mb` |
| 0 | #103 8 MB cartridges crash part-way | bug B01 | MBC5's 9th bank bit lands on bit 7 | T01: 8 MiB MBC5 ROM with numbered banks, via `gb-server` |
| 0 | #104 timer and V-blank serviced in the wrong order | bug B04 | interrupt priority reversed | T04: crafted ROM, PC after dispatch |
| 0 | #105 screenshots rejected by image tools | bug B06 | Adler-32 modulus 65520 | T06: zlib-decode a screenshot |
| 0 | #106 a title shows an invisible character | bug B07 | title rule keeps `$7F` | T07: upload with `$7F` in the title |
| 0 | #107 profile lists equal counts in the wrong order | bug B08 | ties sorted by descending PC | T08: reference profile of `06-ld r,r` |
| 0 | #108 search by cartridge type | must ask | — | P1 |
| 0 | #109 rename by uploading again | declined (OI-2) | — | P4 |
| 0 | #110 Japanese titles | ruled out (OI-5) | — | P7 |
| 0 | #111 colour correction | ruled out (GEP 1) | — | regular screenshot/player checks |
| 1 | #112 a "press any key" pause continues only on release | bug B14 | `joypad.rs` raises its interrupt on the release edge | T14: HALT-until-key ROM, frames 5/20/40 |
| 1 | #113 the library page is empty after one upload | bug B15 | `gb-web` JSON escaper drops the `"` case | T15: upload `SAY "HI"`, list must parse |
| 1 | #114 statistics per cartridge type | must ask | — | P3 |
| 1 | #115 newest uploads first | declined (OI-4) | — | P5 |
| 2 | #116 an upload succeeds but the game never shows up | bug B16 | stored metadata split at the last `=` | T16: upload `jam=2024.gb` |
| 2 | #117 a deleted game's save comes back | bug B17 | deleting a game keeps its save file | T17: upload, save, delete, re-upload |
| 2 | #118 save download name | must ask | — | P2 |
| 2 | #119 ROMs over 8 MB | declined (OI-1) | — | P6 |
| 3 | #120 uploads from a slow office are rejected | bug B18 | 750 ms socket read timeout | T18: upload that stalls 2 s mid-body |
| 3 | #121 searching "C++" finds nothing | bug B19 | `%XX` in the last three bytes of a query not decoded | T19: `q=C%2B%2B` |
| 3 | #122 favourite games | must ask | — | P8: `PUT/DELETE /favorite`, field, filter, persistence |
| 4 | #123 screenshots of Color games can't be decoded | bug B20 | 64 KiB stored-deflate blocks (LEN wraps to 0) | T20: raw-inflate a CGB screenshot |
| 4 | #124 export the library catalogue | must ask | — | P9: `GET /api/export` manifest |

`harness/showcase/bugs.py` applies the bugs; its `DOC_EDITS` removed the
comments and unit tests that gave planted bugs away. Every issue's claims
were checked against a build with its bug (and, for the front end, in
headless Chromium).

## How new issues reach the agent

A file appearing in `ISSUES/` is not enough: a headless agent only takes in
text at the start of an invocation, and may never look at the directory
again. So a wave is delivered through the agent's prompt
(`harness/agents/protocol/notices.py`, identical for every adapter):

1. **Filing.** The product-owner sidecar files the wave: it copies the
   issue files into `/work/ISSUES`, adds rows to the backlog table, writes
   `/notices/wave-N.json` (a volume outside the repository), and from then
   on knows the wave's issues and decisions.
2. **Delivery.** The adapter delivers the notice as the agent's next prompt
   (`protocol/new_issues.txt`: the new issue numbers, titles and files):
   - at once if the agent is between invocations;
   - otherwise it interrupts the running invocation at the first commit
     made after the filing (the work is saved), or at the latest
     `notice_max_wait_sec` after it (default 30 min). It then resumes the
     same session with the notice. An interruption does not count as a
     failed invocation.
3. **Acknowledgement.** Each delivery writes `/notices/wave-N.delivered.json`
   (when, how, how long after the filing). The sidecar logs it, and the
   verifier reads it.
4. **Scoring.** A wave's hidden checks count only if the wave was
   delivered, so an issue that never reached the agent can never be scored
   as a failure.
5. **Repair.** Filed issue files that disappear from `/work/ISSUES` are put
   back by the sidecar (and logged).

`TASK.md` tells the agent upfront that the backlog grows and how it will
hear about new issues.

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
| Backlog bugs (T checks of delivered waves) | 20 % |
| Backlog requests (P checks of delivered waves) | 10 % |
| clippy + rustfmt | 2 % |

Calibration through the real verifier (`test.sh` in the task image, every
wave counted as delivered):

| Tree | Reward |
|---|---|
| v2 starting point (stubbed + 14 bugs) | **0.056** — no hidden backlog check passes |
| The pilot's code with every bug fixed and every decision implemented | **0.764** — all 23 hidden checks pass; lint 0 only because the test patch was not rustformatted |

The rest of the headroom is the accuracy tail the pilot never closed:
`dmg-acid2`, `cgb-acid2`, Color games, Mealybug and sound.

## Diagnostics (reported, not scored)

`scripts/summarize.py` adds a table per trial:

| Technique | Evidence |
|---|---|
| Requirement clarification | which of the 10 decision issues the agent asked about, and whether before its first change to `gb-web` (product owner log) |
| Code localization | bugs fixed (T checks); whether the planted line itself changed, and when (`git log -S` on the trial's `repo.bundle`) |
| Memory | waves delivered and handled; notes/plan files the agent created and how often it updated them; issues with a `## Resolution` |
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
- `ref+decisions` (`decisions_patch.py`, a reference implementation of the
  requests that add behaviour: P1–P3, P8, P9).

Validated result for all 14 bugs:
- every T check passes on `ref`, fails on `ref+` its own bug only, and passes on `all-` its own bug;
- `ref+decisions` passes everything;
- `ref` passes all but P1–P3, P8 and P9, the new features.

So fixing a bug earns its check whatever else is still broken.

## Files

| Path | What |
|---|---|
| `ISSUES/` | wave 0 of the backlog (agent-visible) |
| `harness/showcase/waves/` | waves 1–4 and `schedule.json` (staged into the sidecar only) |
| `harness/HIDDEN_SPEC.md` | the product owner's decisions, by wave |
| `harness/agents/protocol/notices.py`, `new_issues.txt` | delivery of filed waves as prompts |
| `harness/harbor/task/environment/po/sidecar.py` | filing waves, restoring files, logging deliveries |
| `harness/showcase/bugs.py` | the planted bugs and the doc/test edits |
| `harness/showcase/tickets_conformance.py` | hidden checks T01–T20, P1–P9 (staged into the verifier by `sync.sh`) |
| `harness/showcase/decisions_patch.py`, `validate_tickets.sh` | validation |
| `harness/showcase/stub.py`, `rsfns.py`, `fixwarn.py` | how the stubs were made |
| `harness/oracle/gb-oracle` | the reference CLI (built into the image with `harness/ref/`) |
