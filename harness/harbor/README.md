# Running the study with Harbor

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
| Chaos kill at hour 20 | inside the agent adapters (`GB_CHAOS_AFTER_SEC`), since Harbor has no mid-run hook. |

```
harness/harbor/
  task/
    task.toml                 48 h agent timeout, 90 min verifier, 4 CPU / 16 GB, artifacts from sidecars
    instruction.md            staged from harness/AGENT_BRIEF.md (the thin brief)
    environment/
      Dockerfile              rust + scaffold @ pinned commit + assets + vendored crates + both agents
      docker-compose.yaml     networks, repo volume, egress + po sidecars
      egress/                 tinyproxy image + allowlist
      po/                     sidecar image; HIDDEN_SPEC/PRODUCT_OWNER/po_agent.py staged in
    tests/
      test.sh                 kills leftover agent processes, runs grade.py, writes reward.json
      grade.py, golden/, frozen/   staged
  agents/gb_agents.py         ICodeAgent, JiuwenSwarmAgent (BaseInstalledAgent)
  jobs/{smoke,icode,jiuwenswarm}.yaml
  sync.sh                     stages the files above from harness/
  verify_egress.sh            run inside main to prove the allowlist holds
```

## Setup (once, on the machine that runs the study)

```sh
uv tool install harbor                     # or use your fork: uv tool install /path/to/harbor
export DEEPSEEK_API_KEY=...                # the agents' key (goes into the agent container)
export GB_PO_API_KEY=...                   # the product owner's key (po sidecar only; can be the same key)
export PYTHONPATH=$PWD/harness/harbor/agents   # so --agent-import-path gb_agents:... resolves
harness/harbor/sync.sh                     # stage spec, goldens, grader, instruction into the task dir
```

Pin the revisions you are evaluating in `task/environment/Dockerfile`
(`GB_COMMIT`, `ICODE_COMMIT`, `JW_COMMIT`) so the study is reproducible.

## Smoke test first (≈ 20 min + image build)

```sh
harbor run -c harness/harbor/jobs/smoke.yaml -y
```

Checks, in `jobs/gb-smoke/<trial>/`:
- `agent/icode.txt` and `agent/trajectory/icode_adapter.jsonl` — invocations with `rc: 0`, a `chaos.kill` line at ~8 min and a resume after it
- `artifacts/po-artifacts/events.jsonl` — `po.answered` lines if the agent asked anything; `snapshot` lines
- `artifacts/var/log/tinyproxy/tinyproxy.log` — only `api.deepseek.com` CONNECTs succeed; anything else shows as filtered
- `verifier/reward.json` — all metrics present (mostly 0 after 20 minutes, that's fine), `verifier/grade-summary.txt`
- `result.json` — no `exception_info`

To prove the network boundary independently, run inside the main container
of a live trial: `docker exec <main> bash /tests/../verify_egress.sh` (or
copy `harness/harbor/verify_egress.sh` in).

## The real runs

```sh
harbor run -c harness/harbor/jobs/icode.yaml -y
harbor run -c harness/harbor/jobs/jiuwenswarm.yaml -y
```

Each takes ~49 h (48 h agent + build + verify). Run them concurrently on
separate hosts, or set `n_concurrent_trials: 2` on one big host.
Afterwards:

```sh
harness/scripts/grade_snapshots.sh jobs/gb-icode/<trial>        # tier-pass curve over time
harbor view jobs/gb-icode                                        # browse the agent log
```

## What a trial directory contains

```
jobs/<job>/<task>__<id>/
  result.json                       rewards (from reward.json), timings, any exception
  agent/
    icode.txt | jiuwenswarm.txt     the adapter's console output
    trajectory/                     GB_TRAJECTORY_DIR: adapter log, all agent sessions/traces
  verifier/
    reward.json results.json grade-summary.txt git-log.txt SUBMISSION.md QUESTIONS.md
  artifacts/
    po-artifacts/events.jsonl       PO answers, snapshots
    po-artifacts/po_log.jsonl       every Q/A with hidden-spec items + before-first-commit flag
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
