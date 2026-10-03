# Harness — NOT for the agent

Everything in this directory is removed from the sandbox image. It holds
the product owner's answers to the specification's Open Issues, the
product-owner script, the grading scripts and the golden data. (The
specification itself, `GEP-0001.md`, is at the repository root: the agent
has it.) If any of this leaks into the agent's
workspace, the requirement-clarification measurement is void for that run.

```
harness/
  HIDDEN_SPEC.md          the product owner's answers to GEP 1's Open Issues (OI-1…OI-7)
  PRODUCT_OWNER.md        how the PO agent answers, and what it logs
  AGENT_BRIEF.md          the task statement the agent receives (points at GEP-0001.md)
  golden/                 DMG game frame hashes + input scripts (generated)
  golden-cgb/             the same for the Game Boy Color games, + reference screens
  golden-trace/           block hashes and profiles of the Gameboy Doctor reference traces
  extra_assets/           pins and lists for the CGB / Mealybug assets
  harbor/                 THE WAY TO RUN THE STUDY: Harbor task + agents + jobs (see harbor/README.md)
  agents/                 adapter loops for icode/ and jiuwenswarm/ (used by harbor/agents)
  orchestrator/           po_agent.py (used by the PO sidecar), run.py + stub_agent.py for local dry runs
  ref/                    SameBoy reference runner
  scripts/
    fetch_assets.sh       download Pan Docs, opcode table, test ROMs, homebrew
    vendor.sh             vendor crates and flip cargo to offline
    make_golden.sh        run the reference emulator over the game scripts
    fetch_extra_assets.sh CGB and Mealybug test ROMs, CGB games (called by fetch_assets.sh)
    build_assets.sh       pristine ROM copy for the verifier (harness/.assets/)
    grade.py              run all tiers against a repo checkout → JSON
    api_conformance.py    hidden gb-server (debugger API) suite
    web_conformance.py    hidden gb-web (library API) suite
    ui_e2e.py             hidden front-end / player suite (headless Chromium)
    wasm_check.py         gb-wasm vs native frame hashes (Node.js)
    similarity.sh         compare gb-core against known Rust GB emulators
  Dockerfile              sandbox image: toolchain, vendored crates, assets, no network
```

## Build the sandbox image

```sh
harness/scripts/fetch_assets.sh          # needs network; populates docs/ roms/
harness/scripts/vendor.sh                # cargo vendor + offline config
harness/ref/build.sh                     # SameBoy reference runner
harness/scripts/make_golden.sh           # goldens (already committed; re-run only if ROMs/scripts change)
docker build -f harness/Dockerfile -t gb-longhorizon-sandbox .
```

`harness/golden/` is committed: per-frame reference hashes and the input
script for each of the ten games in `roms/LICENSES.md`, plus the SameBoy
commit they came from.

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

## Running the study

**Use Harbor** — see `harbor/README.md`. The task, both agents, the egress
allowlist, the product-owner sidecar and the snapshots are all packaged
there; `harbor run -c harness/harbor/jobs/<agent>.yaml` runs one 48-hour
trial. The orchestrator below predates that packaging and is kept for
Docker-free dry runs of an adapter.

## Local dry runs: the orchestrator

The study is black-box: each coding agent gets the same task, the same
product-owner channel and the same 48 hours in its shipped configuration.
`orchestrator/run.py` runs one agent end to end: creates the workspace
(sandbox container, or a local directory for dry runs), starts the agent
through its adapter (`agents/`), restarts it when it exits, runs the
product-owner agent against `QUESTIONS.md`, snapshots and grades the repo
every 2 h, kills the agent hard at hour 20 (the chaos test), and writes
everything to `harness/runs/<run_id>/`.

```sh
export GB_PO_API_KEY=...                       # DeepSeek / any OpenAI-compatible endpoint
export GB_PO_BASE_URL=https://api.deepseek.com/v1

# dry run, 6 minutes, no container, stub agent
python3 harness/orchestrator/run.py --arm stub --backend local \
    --agent-cmd "python3 $PWD/harness/orchestrator/stub_agent.py" \
    --hours 0.1 --snapshot-hours 0.033 --chaos-hour 0.05 --chaos-downtime-min 0.2

# real runs (see agents/README.md for the env each adapter needs)
python3 harness/orchestrator/run.py --arm icode       --agent-cmd "bash /opt/harness/agents/icode/launch.sh"
python3 harness/orchestrator/run.py --arm jiuwenswarm --agent-cmd "python3 /opt/harness/agents/jiuwenswarm/launch.py"
```

### Contract with an agent adapter

The agent is a black box launched by `--agent-cmd` inside the workspace
(`/work` in the container), with `TASK.md` and `QUESTIONS.md` present. It
must keep running until killed. Environment:

| Variable | Meaning |
|---|---|
| `GB_RUN_ID` | unique run id |
| `GB_ARM` | the label given with `--arm` |
| `GB_TRAJECTORY_DIR` | directory for the agent's own logs; collected with every snapshot |

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
