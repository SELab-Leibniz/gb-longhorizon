# Harness — NOT for the agent

Everything in this directory is removed from the sandbox image. It holds
the hidden acceptance spec, the product-owner script, the grading scripts
and the Tier-3 golden hashes. If any of this leaks into the agent's
workspace, the requirement-clarification measurement is void for that run.

```
harness/
  HIDDEN_SPEC.md          the full requirements the product owner knows
  PRODUCT_OWNER.md        how the PO agent answers, and what it logs
  AGENT_BRIEF.md          the only task statement the agent receives
  golden/                 Tier-3 frame hashes + input scripts (generated)
  scripts/
    fetch_assets.sh       download Pan Docs, opcode table, test ROMs, homebrew
    vendor.sh             vendor crates and flip cargo to offline
    make_golden.sh        run the reference emulator over the game scripts
    grade.py              run all tiers against a repo checkout → JSON
    similarity.sh         compare gb-core against known Rust GB emulators
  Dockerfile              sandbox image: toolchain, vendored crates, assets, no network
```

## Build the sandbox image

```sh
harness/scripts/fetch_assets.sh          # needs network; populates docs/ roms/
harness/scripts/vendor.sh                # cargo vendor + offline config
harness/scripts/make_golden.sh           # needs SameBoy on PATH
docker build -f harness/Dockerfile -t gb-longhorizon-sandbox .
```

The Dockerfile copies the repo, deletes `harness/` and `.git/`, re-inits
git with a single "initial scaffold" commit, and sets `--network none` at
run time. The agent's repo history therefore starts clean, which also makes
the trajectory analysis simpler.

## Grading a run

```sh
python3 harness/scripts/grade.py /path/to/agent/checkout --golden harness/golden -o results.json
```

`grade.py` builds the checkout in release mode, runs each tier, and writes
a JSON report with per-ROM outcomes, time per ROM, panics, and the Tier-3
match rate. It also records `git log` so the tier-pass curve can be
replayed against commit timestamps. Run it on each 2-hour snapshot to
produce the time-series the write-up needs.

## Metrics the orchestrator should log (outside this harness)

- wall-clock per tool call; tool-call error streaks
- context resets / compressions and their size
- product-owner questions asked (timestamp, question, answer) — from the PO log
- unexpected process exits of the agent runtime
- the chaos-kill event (hour ~20) and time-to-first-useful-action after resume
- tokens in / out per hour

## Contamination check

`similarity.sh` clones a fixed list of Rust Game Boy emulators (list inside)
and runs a token-level similarity tool over `gb-core/src`. Report the
maximum pairwise score and the top three matching files. Idiomatic Rust
plus a shared hardware spec will produce *some* similarity in the opcode
dispatch; what you are looking for is structural identity across several
modules.
