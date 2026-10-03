# Pilot report — iCode on GEP 1, 4.3 hours

*3 October 2026 · branch `full-spec` · trial `task__Y3EpGjr`*

**Setup.** One iCode trial under the benchmark protocol: the agent received `TASK.md` and the
repository (GEP 1 specification, Rust skeleton, all test ROMs) in the offline sandbox, with
`deepseek-flash` as the model, the scripted product owner reachable through `QUESTIONS.md`,
no forced interruptions, and repository snapshots every hour. Planned for 6 hours; **stopped
at 4.33 h** by decision, once the difficulty question was answered. The last commit was
then graded by the full hidden verifier. Agent `b5366a0954`, Rust
1.97.0, Chromium 154.0.8037.92.

## Summary

* **Final score 0.758** out of 1.0 after 4.3 hours; **0.47 after one hour, 0.71 after two.**
* Everything that is *breadth* — the CPU-trace tool, profiler, debugger API, `no_std` build,
  WebAssembly build, game-library web service and front end — was done within about
  **1 h 05 min**, mostly by three sub-agents working in parallel while the main agent kept the
  core, and scored 0.95–1.00.
* The *accuracy tail* — Game Boy Color games, rendering tests, audio, pixel-accurate PPU — is
  where progress slowed: 0.71 for over two hours, then a timing fix at 4 h 16 min lifted the
  Mooneye suites from 53 to 73 of 86 (DMG) and from 26 to 45 of 47 (CGB).
* The agent asked all six Open Issues in its first minute, implemented every answer correctly,
  worked in one continuous session for the whole run (never stopped, never claimed to be
  done), and spent **$1.89** of model usage.
* **Verdict:** in its current form the task is **too easy for a 48-hour run** with this agent:
  more than 70 % of the score is reachable in two hours. The long-horizon part is the
  accuracy tail, which should carry most of the weight (see the v2 plan at the end).

## Results by category

Score per category (0–1). The pilot column is the committed state at 4.3 h. The two
20-minute columns come from the earlier smoke runs, graded *as if* their uncommitted work had
been committed (their committed score was 0.03 each); they are shown for context, not as a
like-for-like comparison.

| Category | Weight | iCode, pilot (4.3 h) | iCode, 20 min | jiuwenswarm, 20 min |
|---|---|---|---|---|
| Emulator core (DMG) | 30 % | **0.73** | 0.00 | 0.29 |
| Game Boy Color | 15 % | **0.37** | 0.00 | 0.13 |
| Pixel-accurate PPU | 7 % | **0.10** | 0.00 | 0.05 |
| Tooling (trace, profiler, debugger API) | 13 % | **1.00** | 0.00 | 0.00 |
| Portability (no_std, WebAssembly) | 8 % | **1.00** | 0.00 | 0.00 |
| Game library service | 14 % | **1.00** | 0.00 | 0.00 |
| Web front end + player | 10 % | **0.95** | 0.00 | 0.00 |
| Lint (clippy + rustfmt) | 3 % | 1.00 | 0.00 | 0.00 |
| **Overall reward** | 100 % | **0.758** | 0.000 | 0.112 |

## Checks against the GEP acceptance table

| Check | Result | GEP target | Met |
|---|---|---|---|
| Blargg CPU/timing | 17/18 | all pass | ✗ |
| Mooneye acceptance (excl. ppu/) | 50/54 | ≥ 90 % | ✓ |
| Mooneye MBC1 / MBC5 | 12/12 · 8/8 | all | ✓ |
| dmg-acid2 | fail | pass | ✗ |
| DMG games (score ≥ 0.8) | 8/10 | ≥ 8/10 | ✓ |
| Blargg dmg_sound | 0/13 | ≥ 75 % | ✗ |
| Save states · determinism | pass · pass | pass | ✓ |
| Mooneye CGB | 45/47 | ≥ 90 % | ✓ |
| cgb-acid2 | fail | pass | ✗ |
| CGB games (score ≥ 0.8) | 1/10 | ≥ 8/10 | ✗ |
| Blargg cgb_sound | 1/13 | ≥ 75 % | ✗ |
| Mealybug Tearoom (DMG) | 0/24 | ≥ 10/24 | ✗ |
| Mooneye acceptance/ppu | 3/12 | ≥ 75 % | ✗ |
| CPU trace vs reference (11 ROMs) | 0.9943 (02-interrupts diverges at 94 %) | all exact | ✗ |
| Profiler | 11/11 exact | exact | ✓ |
| Debugger API suite | 36/36 | all | ✓ |
| no_std build (thumbv7em) | builds | builds | ✓ |
| WebAssembly vs native frames | 5/5 identical | identical | ✓ |
| Library API suite (incl. Open Issues) | 41/41 | all | ✓ |
| Front end + player (headless Chromium) | 20/21 | all | ✗ |

**10 of 20 targets met.** The one front-end miss (`player_input`) is a test
coupling rather than a player bug: in this agent's emulator, 2048 does not yet react to START
natively either, so the check cannot see the key press (to be fixed in the grader, see below).

## Progress over time

| Elapsed | Reward | Core | CGB | PPU | Tooling | Portability | Library | Front end | Commits in the hour |
|---|---|---|---|---|---|---|---|---|---|
| 1.0 h | 0.473 | 0.65 | 0.21 | 0.10 | 1.00 | 1.00 | 0.00 | 0.00 | 8 |
| 2.0 h | 0.712 | 0.66 | 0.22 | 0.10 | 1.00 | 1.00 | 1.00 | 0.95 | 4 |
| 2.9 h | 0.712 | 0.66 | 0.22 | 0.10 | 1.00 | 1.00 | 1.00 | 0.95 | 1 |
| 3.9 h | 0.712 | 0.66 | 0.22 | 0.10 | 1.00 | 1.00 | 1.00 | 0.95 | 0 |
| 4.3 h (final) | **0.758** | 0.73 | 0.37 | 0.10 | 1.00 | 1.00 | 1.00 | 0.95 | 1 |

### Commit timeline

| Time | Commit |
|---|---|
| 0:01 | Ask product owner about the six Open Issues |
| 0:06 | Implement MMU, timer, joypad, serial, APU registers, PPU scanline rendering |
| 0:19 | Implement CPU step, full opcode set, MBC1/3/5, fix PPU sprite VRAM indexing and PUSH |
| 0:22 | Add no_std support to gb-core and debugger data-access plumbing |
| 0:24 | Add debugger API to Emulator; scaffold gb-tools, gb-wasm, gb-web crates |
| 0:43 | Implement gb-tools: gb-trace (trace/profile/doctor) and gb-server (full Appendix B API) |
| 0:45 | Implement gb-wasm Appendix C ABI (import-free, DMG+CGB frame parity) |
| 0:46 | Fix clippy lints across gb-core (collapsible_match, needless_range_loop, casts) |
| 1:05 | Implement gb-web service: Appendix D API, store, PNG/SHA256/JSON, front end |
| 1:12 | Allow internal WRAM/HRAM access during OAM DMA |
| 1:35 | Timer reload window and DMG-only I/O register reads |
| 1:41 | PPU: disable window when BG disabled; sprite X-priority order |
| 2:08 | MBC1/3/5: allocate standard 8 KiB RAM when header declares none |
| 4:15 | Fix OAM DMA bus masking/restart timing and timer reload model |

## How the agent worked

**Clarification first.** Its first commit, one minute in, was the six Open Issue questions
from the GEP; the product owner answered all six within seconds and the agent implemented
every answer as decided (the hidden library suite checks them: 41/41). It asked nothing else
during the run.

**Core first, built for what comes next.** It went straight to the emulator core in two large
commits — memory bus, timer, joypad, serial and a scanline PPU at 7 minutes; the full CPU and
cartridge controllers at 19 minutes — and immediately added the `no_std` split and the
debugger plumbing the tooling would need (22–25 minutes), before any tool existed.

**Delegation for breadth.** It used 7 sub-agents, each with a sharply scoped brief
and an instruction not to commit (the main agent reviewed and committed):

| Started | Brief (abridged) | Outcome |
|---|---|---|
| 0:25 | Build the `gb-tools` crate (`gb-trace`, `gb-server`) — do not touch `gb-core` | committed 0:43; trace, profiler and debugger API essentially complete |
| 0:46 | Build the `gb-web` crate and its front end — only files under `gb-web/` | committed 1:05 together with the next one |
| 0:51 | Rewrite the two front-end pages (`index.html`, `player.html`) | library API 41/41, front end 20/21 |
| 2:20 | (explore agent) Condense GEP 1 into the complete list of machine-checkable acceptance criteria | an audit of the gaps, once the breadth work was done |
| 2:29 | Make the Mooneye acceptance *timing* tests pass | landed 4:16: +20 DMG and +19 CGB Mooneye passes |
| 4:16 | two further sub-agents (briefs not in the saved session) | still running when the trial was stopped |

The main agent kept the core and integration to itself while sub-agents built the tools
(done at 43 min) and the web service with its front end (65 min) in parallel; WebAssembly
followed at 46 min.

**Testing and verification.** Of 1290 shell commands recovered from the trajectory:
212 builds, 47 ROM-suite runs,
93 runs of the CLI on individual ROMs, 20 trace comparisons against the
reference excerpt, 6 Node.js runs of the WebAssembly host, 21 lint runs and
89 git operations; 104 commands read the specification or hardware documentation.
It did not drive Chromium itself, yet the front end passed 20 of 21 browser checks. Lint was
clean at the end.

**The accuracy tail.** After two hours it moved to correctness: small fixes (OAM DMA bus
access, timer reload, window/sprite priority, cartridge RAM), an audit of the GEP's
acceptance criteria against the code (2:20), and a long debugging campaign with a dozen
throw-away diagnostic programs (`gb-tools/examples/`). For about two hours no committed change
moved the score; then the timing sub-agent started at 2:29 delivered at 4:16, adding 20 DMG
and 19 CGB Mooneye passes. Rendering (both acid2 tests), audio,
CGB games and the pixel FIFO were still untouched or failing when the run stopped.

**Effort and context.**

| | Hour 1 | Hour 2 | Hour 3 | Hour 4 | Last 20 min | Total |
|---|---|---|---|---|---|---|
| Model calls | 496 | 446 | 361 | 352 | 168 | **1823** |
| Tool calls | 605 | 486 | 416 | 403 | 181 | **2091** |
| Context compactions | 13 | 14 | 13 | 13 | 3 | **56** |
| Sub-agents started | 3 | 0 | 2 | 0 | 2 | **7** |
| Input tokens (M) | 25.6 | 24.0 | 19.5 | 18.5 | 8.8 | **96.4** |
| Output tokens (M) | 0.77 | 0.70 | 0.70 | 0.73 | 0.23 | **3.13** |

94.4 % of input tokens were cache hits. At DeepSeek list prices (US $0.14 / 0.0028 / 0.28 per
million uncached-input / cached-input / output tokens) the run cost **$1.89** — about
$0.44 per hour, so a 48-hour run would cost roughly $21. The context was compacted
56 times (about every 4.6 minutes) and the agent stayed in a single session
throughout: no restarts, no "done" claims, no `SUBMISSION.md`.

## What this means for the case study

1. **Breadth is too cheap.** Tooling, portability, library and front end are 45 % of the score
   and were finished in about an hour; they do not distinguish a strong agent after hour 1.
2. **The tail is the long-horizon test.** Timing, audio, Color and pixel accuracy needed hours
   of debugging and a dedicated sub-agent (1 h 47 min) to move at all. That is where memory, verification
   and context management matter, and it should carry most of the score.
3. **Next: v2 showcase** — start from this pilot's own codebase with the main features
   reduced to signatures, symptom-only bug tickets with planted root causes, ambiguities hidden
   in the tickets instead of a labelled Open Issues list, a reference oracle in the sandbox,
   and scoring weighted toward accuracy and tickets.

### Grader notes from this run

* `player_input` couples a front-end check to core accuracy (see above); v2 replaces 2048
  with a crafted joypad-to-palette ROM.
* The CPU trace for `02-interrupts` diverges at 94 % of its length — a genuine emulator
  difference (interrupt timing), not a grader issue.

## Caveats

One trial of one agent, stopped at 4.3 of 6 planned hours; model sampling makes every run
different. jiuwenswarm was not part of this pilot (20-minute data only).

*Data: `jobs/gb-pilot-6h/task__Y3EpGjr/` (verifier results, trajectory, product-owner log,
hourly snapshots), graded snapshots and `report_data.py` output.*

