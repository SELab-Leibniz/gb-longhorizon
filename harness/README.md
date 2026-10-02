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
  orchestrator/           run.py (controller), po_agent.py, stub_agent.py
  ref/                    SameBoy reference runner
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

## Running an arm: the orchestrator

`orchestrator/run.py` runs one 48-hour arm end to end: creates the
workspace (sandbox container, or a local directory for dry runs), starts
the agent, restarts it when it exits, runs the product-owner agent against
`QUESTIONS.md`, snapshots and grades the repo every 2 h, kills the agent
hard at hour 20 (the chaos test), and writes everything to
`harness/runs/<run_id>/`.

```sh
export GB_PO_API_KEY=...                       # DeepSeek / any OpenAI-compatible endpoint
export GB_PO_BASE_URL=https://api.deepseek.com/v1

# dry run, 6 minutes, no container, stub agent
python3 harness/orchestrator/run.py --arm full --backend local \
    --agent-cmd "python3 $PWD/harness/orchestrator/stub_agent.py" \
    --hours 0.1 --snapshot-hours 0.033 --chaos-hour 0.05 --chaos-downtime-min 0.2

# real run
python3 harness/orchestrator/run.py --arm full     --agent-cmd "<your framework's launch command>"
python3 harness/orchestrator/run.py --arm baseline --agent-cmd "<same command>"
python3 harness/orchestrator/run.py --arm ablate-memory --agent-cmd "<same command>"
```

### Contract with the agent framework

The agent is a black box launched by `--agent-cmd` inside the workspace
(`/work` in the container), with `TASK.md` and an empty `QUESTIONS.md`
present. It must keep running until killed. Environment:

| Variable | Meaning |
|---|---|
| `GB_RUN_ID` | unique run id |
| `GB_ARM` | `full`, `baseline`, or `ablate-<module>` |
| `GB_DISABLED_MODULES` | comma-separated subset of `clarification,localization,memory,verifier,compression`; the agent must switch these off |
| `GB_TRAJECTORY_DIR` | directory the agent should write its trajectory/tool log to; collected with every snapshot |

Questions to the product owner: append `## Q: …` to `QUESTIONS.md`; the
answer appears as `## A: …` underneath within a couple of minutes.

### Outputs per run

```
harness/runs/<run_id>/
  config.json            the exact arguments used
  events.jsonl           time-stamped: agent start/exit, chaos kill, PO answers, snapshots
  po_log.jsonl           every Q/A with spec items drawn on and whether the agent had committed yet
  agent.std{out,err}.log
  snapshots/NNN/
    repo.bundle          `git clone repo.bundle` to inspect the state at that hour
    results.json         grade.py output
    trajectory/          the agent's own log at that time
```

### Metrics to add from your framework's side

`events.jsonl` covers process-level stability (exits, restarts, chaos
recovery time, commit cadence, tier-pass curve). What only the framework
can log, and should write into `GB_TRAJECTORY_DIR`: tokens in/out per
hour, tool calls and error streaks, context compressions and their size,
verifier decisions (what it rejected and why).

## Contamination check

`similarity.sh` clones a fixed list of Rust Game Boy emulators (list inside)
and runs a token-level similarity tool over `gb-core/src`. Report the
maximum pairwise score and the top three matching files. Idiomatic Rust
plus a shared hardware spec will produce *some* similarity in the opcode
dispatch; what you are looking for is structural identity across several
modules.
