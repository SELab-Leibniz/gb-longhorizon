# Running the study with Harbor

The protocol for a benchmark campaign — fixed conditions, number of trials,
validity and re-run rules, reporting — is `../BENCHMARK.md`. This file is
the mechanics.

The case study is packaged as one [Harbor](https://github.com/harbor-framework/harbor)
task plus two Harbor agent classes. Harbor owns the container lifecycle,
the 48-hour timeout, the verifier, and the per-trial results directory.
Three things Harbor's Docker environment does not do are provided by the
task's own compose file:

| Need | How |
|---|---|
| Egress allowlist (only the model API) | `main` sits on an *internal* network; the `egress` sidecar (tinyproxy, `environment/egress/allowlist`) is its only route out. Everything not on the list gets 403. Harbor's `allowlist` network mode only exists for cloud providers, not Docker. |
| Product owner answering `QUESTIONS.md` | `po` sidecar shares the `/work` volume, holds the hidden spec and its own API key, has normal internet. The agent container never sees either. |
| Periodic snapshots | same `po` sidecar bundles the repo every `GB_SNAPSHOT_HOURS`; collected as a trial artifact; graded post hoc with `scripts/grade_snapshots.sh`. |
| Forced interruption (optional) | the adapters can kill the agent once mid-run (`chaos_after_sec`) to exercise resumption. On in the smoke jobs (minute 8) to test the restart path; **off in the benchmark jobs** — the agents are black boxes (see `../BENCHMARK.md`). |

### What the agent builds (branch `full-spec`)

The agent gets one specification up front, `GEP-0001.md` (a PEP-style
proposal at the repository root), plus every test ROM and asset it needs.
There are no mid-run changes. The GEP's **Open Issues** (upload size limit,
duplicates, unsupported cartridges, list order, odd titles, seeding) are
answered only by the product owner (`HIDDEN_SPEC.md`); the
hidden tests check those answers, so asking pays.

| GEP 1 | Deliverable | Graded by (tier) | Weight |
|---|---|---|---|
| §2 | emulator core, DMG (CPU, timing, PPU, MBC, APU, games, save states) | 0–4 | 0.30 |
| §3 | Game Boy Color | 5: Mooneye-CGB, cgb-acid2, 10 CGB games, `cgb_sound` | 0.15 |
| §4 | pixel-accurate PPU | 6: Mealybug Tearoom DMG; Mooneye `ppu/`, `oam_bug` | 0.07 |
| §5 | `gb-trace` (Gameboy Doctor traces + profiler), `gb-server` debugger API | 7: block hashes of 11 reference traces, profile top-20, hidden 36-check API suite | 0.13 |
| §6 | `no_std` core, `gb-wasm` ABI | 8: thumbv7em build; Node runs the wasm and must match native frame hashes (with input) | 0.08 |
| §7 | `gb-web` game library: REST API, uploads + validation, SHA-256 ids, PNG screenshots, saves, stats, persistence, concurrency | 9: hidden 41-check API suite incl. the Open Issue answers | 0.14 |
| §8 | library page + in-browser player (wasm on a canvas, keyboard, `window.gbTest`) | 9: headless-Chromium end-to-end, 21 checks; player frames must match native | 0.10 |
| — | clippy + rustfmt clean | 0 | 0.03 |

`reward.json` also reports a `phase_*` sub-score per row. Every check was
validated against a reference before use: SameBoy (through a `gb` shim)
scores 47/47 Mooneye-CGB, cgb-acid2, 11/11 CGB games under unseen timing
perturbations (dead joypad 0/11) and 10/24 Mealybug; a replayed reference
trace scores 1.0; spec-following stand-ins score 36/36 (debugger), 41/41
(library API) and 21/21 (browser), and each injected bug — duplicates
accepted, no size limit, unsupported cartridges accepted, re-seeding,
case-sensitive order, off-by-one screenshots, titles rendered as HTML, wrong
CGB colours, wrong key mapping, no frame pacing — fails exactly its check.
The verifier's ROMs are pristine copies from `scripts/build_assets.sh`
(`harness/.assets/`, built once in Docker; `sync.sh` runs it if missing),
uploaded to `/tests` only after the agent has stopped.

```
harness/harbor/
  task/
    task.toml                 48 h agent timeout, 150 min verifier, 4 CPU / 16 GB, artifacts from sidecars
    instruction.md            staged from harness/AGENT_BRIEF.md (the brief; points at GEP-0001.md)
    environment/
      Dockerfile              rust + scaffold @ pinned commit + assets + vendored crates + both agents
      docker-compose.yaml     networks, repo volume, egress + po sidecars
      egress/                 tinyproxy image + allowlist
      po/                     sidecar image; GEP-0001/HIDDEN_SPEC/PRODUCT_OWNER/po_agent.py staged in
    tests/
      test.sh                 kills leftover agent processes, runs grade.py, writes reward.json
      grade.py, golden/, frozen/   staged
  agents/gb_agents.py         ICodeAgent, JiuwenSwarmAgent (BaseInstalledAgent)
  jobs/{smoke,icode,jiuwenswarm}.yaml
  sync.sh                     stages the files above from harness/
  verify_egress.sh            run inside main to prove the allowlist holds
```

## Running it

Step-by-step instructions — requirements, setup, keys, smoke tests, the real
runs, monitoring, results, configuring and adding agents, troubleshooting —
are in [`../RUNNING.md`](../RUNNING.md). The rest of this file describes the
design.

The scaffold baked into the image is exactly the committed `HEAD`
(`sync.sh` writes `git archive HEAD` to `environment/scaffold.tar`). The
agents are pinned in `task/environment/Dockerfile` (`ICODE_COMMIT`,
`JW_COMMIT`; jiuwenswarm's default branch is `develop` — its `main` is an
older project).

## What a trial directory contains

```
jobs/<job>/<task>__<id>/
  result.json                       rewards (from reward.json), timings, any exception
  agent/
    icode.txt | jiuwenswarm.txt     the adapter's console output
    trajectory/                     GB_TRAJECTORY_DIR: adapter log, all agent sessions/traces
  verifier/
    reward.json results.json grade-summary.txt git-log.txt SUBMISSION.md QUESTIONS.md
    repo.bundle                     the full repository as graded (re-grade / review)
    environment.json                graded commit, toolchain, Chromium, agent versions
    working-tree.json               does the uncommitted state build, how many uncommitted files
    screenshots/index.html, summary.json, <rom>/agent_*.png, <rom>/compare_*.png
  artifacts/
    po-artifacts/events.jsonl       PO answers, snapshots
    po-artifacts/po_log.jsonl       every Q/A with the Open Issue ids + before-first-commit flag
    po-artifacts/snapshots/*.bundle 2-hourly repo state
    var/log/tinyproxy/tinyproxy.log every egress attempt
```

## Notes

- The agent container has network only at **build** time. Both agents are
  pre-installed in the image for that reason; `install()` just checks.
- The verifier runs in the agent's container (`environment_mode = "shared"`)
  because it needs the vendored toolchain and the ROMs. `test.sh` kills any
  leftover agent process first.
- `orchestrator/run.py --backend local` remains useful for quick adapter
  tests without Docker; it is not used for the study.
- Harbor's `delete: false` keeps the trial's containers and the `/work`
  volume after the run, so the final repo can be inspected or the demo
  (`gb-gui`) built from it.
