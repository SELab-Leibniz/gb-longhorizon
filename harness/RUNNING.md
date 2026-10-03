# Running the gb benchmark — step by step

This is the guide for running the case study on your own machine: set up
once, smoke-test, run the 48-hour trials, read the results. Two companion
documents go deeper:

* [`BENCHMARK.md`](BENCHMARK.md) — the protocol (what is measured, fixed
  conditions, validity and re-run rules, how to report). Read it before a
  real campaign.
* [`harbor/README.md`](harbor/README.md) — how the pieces fit together
  (sandbox, egress proxy, product owner, verifier) and what a trial
  directory contains.

**What a run is.** A coding agent is dropped into this repository inside an
offline Docker sandbox and given `TASK.md` (= [`AGENT_BRIEF.md`](AGENT_BRIEF.md))
and the specification [`GEP-0001.md`](../GEP-0001.md): build a Game Boy /
Game Boy Color emulator platform with tooling, embedded and WebAssembly
builds and a web game library. It works unattended for 48 hours; it can ask a
scripted product owner questions through `QUESTIONS.md`. Afterwards a hidden
verifier grades the last commit and writes a score between 0 and 1.

---

## 1. Requirements

| | Minimum | Notes |
|---|---|---|
| Host | macOS (Apple silicon) with Colima, or Linux with Docker Engine | Developed and tested on macOS 26 / Apple M-series with Colima 0.10. Linux should work the same (Harbor drives `docker compose`) but has not been tested. |
| CPU / RAM for Docker | **6 CPUs / 20 GB** for one trial at a time; **10 CPUs / 36 GB** to run two agents side by side | each trial is limited to 4 CPUs / 16 GB; the egress proxy and product-owner sidecars need a little more |
| Disk for Docker | ~60 GB free | task image ≈ 7 GB; Rust build directories and repository volumes add a few GB per trial; trials are kept until you delete them |
| Network | at **build** time: GitHub, astral.sh (uv), PyPI, Debian mirrors, static.rust-lang.org, gitcode.com (jiuwenswarm's engine). At **run** time: only the model API. | the sandbox cannot reach anything else |
| Power | the machine must stay awake for ~50 hours: **on AC power**, sleep disabled | see §9 |
| Software | Docker CLI with the `compose` and `buildx` plugins, git, Python ≥ 3.9 on the host, [`uv`](https://docs.astral.sh/uv/) | |
| Model API | a DeepSeek account and API key (used by both agents and by the product owner) | other OpenAI-compatible providers: §7 |

## 2. One-time setup

**2.1 Clone the repository.** The default branch, `full-spec`, is the
benchmark described here (`main` and `harder-task` hold earlier variants).

```sh
git clone https://github.com/SELab-Leibniz/gb-longhorizon.git
cd gb-longhorizon
git branch --show-current         # full-spec
```

All commands below run from the repository root.

**2.2 Docker.** On macOS with Homebrew:

```sh
brew install colima docker docker-compose docker-buildx
# let the docker CLI find Homebrew's plugins
python3 - <<'EOF'
import json, os; p = os.path.expanduser("~/.docker/config.json")
os.makedirs(os.path.dirname(p), exist_ok=True)
c = json.load(open(p)) if os.path.exists(p) else {}
d = c.setdefault("cliPluginsExtraDirs", [])
"/opt/homebrew/lib/docker/cli-plugins" in d or d.append("/opt/homebrew/lib/docker/cli-plugins")
json.dump(c, open(p, "w"), indent=2)
EOF
colima start --cpu 10 --memory 36 --disk 200     # or --cpu 6 --memory 20 for one trial at a time
docker compose version && docker buildx version   # both must print a version
```

On Linux: install Docker Engine with the compose and buildx plugins and make
sure your user can run `docker` without sudo.

**2.3 Harbor** (the trial runner; pinned to the version the benchmark was built with):

```sh
uv tool install harbor==0.23.0
harbor --version                                  # harbor v0.23.0
```

**2.4 API keys.** Keep them in a file outside the repository, readable only
by you:

```sh
cat > ~/.gb-keys.env <<'EOF'
DEEPSEEK_API_KEY=sk-...        # the agents' key (it goes into the agent container)
GB_PO_API_KEY=sk-...           # the product owner's key (sidecar only; can be the same key)
EOF
chmod 600 ~/.gb-keys.env
```

Check which model IDs your account offers — the jobs use `deepseek-flash`:

```sh
set -a; . ~/.gb-keys.env; set +a
curl -s -H "Authorization: Bearer $DEEPSEEK_API_KEY" https://api.deepseek.com/models
```

**2.5 Per-shell environment.** Every shell that runs `harbor` needs:

```sh
set -a; . ~/.gb-keys.env; set +a                  # keys, never printed
export PYTHONPATH=$PWD/harness/harbor/agents      # Harbor imports the agent classes from here
```

**2.6 Stage the task.**

```sh
harness/harbor/sync.sh
```

The first time, this builds the verifier's private copy of every test ROM in
a throwaway container (`harness/.assets/`, 5–10 minutes, needs network). It
then copies the grader, goldens, product-owner files and the task statement
into `harness/harbor/task/`, and packs the repository's committed `HEAD`
into the image build context. It checks that every graded game has golden
data and stops with a message if not. Re-run it after any change you commit.

## 3. Smoke tests (≈ 45 minutes each, first one includes the image build)

Run both before any real trial. They use the real model, cost about
US $0.15–0.30 each, and exercise everything: image build, egress proxy,
product owner, the agent loop, a forced restart at minute 8, and the full
verifier.

```sh
harbor run -c harness/harbor/jobs/smoke.yaml -y               # iCode, 20 minutes
harbor run -c harness/harbor/jobs/smoke-jiuwenswarm.yaml -y   # jiuwenswarm, 20 minutes
```

The first build of the task image takes 15–30 minutes (it installs both
agents, the Rust toolchain, Chromium and every test ROM); later builds reuse
the cache. Then check, in `jobs/gb-smoke/<trial>/` (and `jobs/gb-smoke-jiuwenswarm/…`):

| Check | Where | Expect |
|---|---|---|
| no infrastructure error | `result.json` | `"exception_info": null` |
| agent ran and resumed | `agent/trajectory/*_adapter.jsonl` | `adapter.start`, a `chaos.kill` at ~8 min, `invocation.end` with `session_recovered`, `adapter.stop` at the end |
| product owner works | `artifacts/po-artifacts/events.jsonl` | `po.answered` lines if the agent asked (on `full-spec`, agents asked the GEP's Open Issues in their first minutes; on `showcase-v2`, look for questions about backlog items) |
| answers in place | `verifier/QUESTIONS.md` | each `## A:` directly under its `## Q:` |
| network boundary | `artifacts/var/log/tinyproxy/tinyproxy.log` | `Established connection to host "api.deepseek.com"` and no other host |
| verifier ran | `verifier/reward.json`, `grade-summary.txt` | all metrics present; after 20 minutes nearly everything is 0 (lint may score 0.03) — that is expected |
| audit trail | `verifier/repo.bundle`, `environment.json` | present; versions listed |
| screenshots | `verifier/screenshots/index.html` | opens in a browser (mostly "panic"/"no frames" after 20 minutes) |

`harness/scripts/summarize.py jobs/gb-smoke jobs/gb-smoke-jiuwenswarm` prints
the same in one table (valid = ✓✓).

## 4. The real runs (≈ 50 hours)

```sh
# terminal 1
set -a; . ~/.gb-keys.env; set +a; export PYTHONPATH=$PWD/harness/harbor/agents
caffeinate -ims -t 190000 &                       # macOS: stay awake for ~53 h (needs AC power)
harbor run -c harness/harbor/jobs/icode.yaml -y

# terminal 2, at the same time (fair: both agents see the same API conditions)
set -a; . ~/.gb-keys.env; set +a; export PYTHONPATH=$PWD/harness/harbor/agents
harbor run -c harness/harbor/jobs/jiuwenswarm.yaml -y
```

Each job runs one trial (`n_attempts: 1`; `BENCHMARK.md` recommends three
per agent when you want statistical claims). A trial is: image build (a few
minutes with a warm cache) → 48 hours of agent work → up to 2.5 hours of
grading. Harbor prints a progress bar; leave the terminals open (or use
`tmux`/`nohup … &`). **Do not intervene during a trial** — no answering
questions, restarting agents or editing the workspace (`BENCHMARK.md` §3).

Running the two agents one after the other instead is also valid; it takes
twice as long and the API conditions may differ between the two halves.

## 5. Monitoring a trial (read-only)

```sh
docker ps --format '{{.Names}}'          # task__<id>__env-main-1, …-po-1, …-egress-1 (one set per trial)
docker exec task__<id>__env-po-1 tail -f /po-artifacts/events.jsonl     # questions answered, 2-hourly snapshots
docker exec task__<id>__env-main-1 git -C /work log --oneline | head    # the agent's commits
docker exec task__<id>__env-main-1 cat /work/QUESTIONS.md               # the Q&A so far
tail -f jobs/gb-icode/<trial>/agent/trajectory/icode_adapter.jsonl      # invocations, restarts
```

Optional, to prove the network boundary of a live trial yourself:

```sh
docker cp harness/harbor/verify_egress.sh task__<id>__env-main-1:/tmp/v.sh
docker exec -e HTTPS_PROXY=http://egress:8888 -e HTTP_PROXY=http://egress:8888 task__<id>__env-main-1 bash /tmp/v.sh
```

## 6. Results

```sh
harness/scripts/summarize.py jobs/gb-icode jobs/gb-jiuwenswarm \
    --price-in 0.14 --price-cached 0.0028 --price-out 0.28      # USD per 1M tokens; check your provider's prices
```

prints one row per trial — reward, the seven phase scores, validity, agent
hours, commits, product-owner questions and which Open Issues were asked
(on `showcase-v2`, also the showcase diagnostics table — `SHOWCASE.md`),
tokens and cost — and per-agent mean / spread / min / max. Per trial you also
have:

* `verifier/reward.json` — `reward` (the headline number) and every
  component; `verifier/results.json` — every individual check.
* `verifier/screenshots/index.html` — every game booted on the agent's
  emulator next to the reference emulator.
* `verifier/SUBMISSION.md`, `QUESTIONS.md`, `git-log.txt`.
* `artifacts/po-artifacts/snapshots/*.bundle` — the repository every
  2 hours, for progress curves (`BENCHMARK.md` §5).
* `verifier/repo.bundle` — the graded repository, for re-grading or review
  (`git clone repo.bundle final`).

How the score is computed (weights, gates, partial credit) is in
`BENCHMARK.md` §5 and in the agent's own `TASK.md`.

## 7. Configuring the agents

Everything about *how an agent is treated* is fixed by the protocol
(`agents/protocol/`): both get `TASK.md` verbatim as the first prompt and the
same continue/recheck prompts. What you may change, per campaign, applying
the same change to every agent:

| What | Where |
|---|---|
| model | `model_name:` in `harness/harbor/jobs/<agent>.yaml` (`deepseek/<model-id>`) |
| run length | `run_seconds:` (agent wall clock) and `override_timeout_sec:` in the job YAML; `timeout_sec` under `[agent]` in `task/task.toml` |
| number of trials | `n_attempts:` in the job YAML |
| agent versions | `ICODE_COMMIT`, `JW_COMMIT` in `harness/harbor/task/environment/Dockerfile` (re-run `sync.sh`; the image rebuilds) |
| product-owner model | `GB_PO_MODEL`, `GB_PO_BASE_URL` (environment, or defaults in `task/task.toml`) |
| forced interruption | `chaos_after_sec:` (0 = off, the benchmark setting) |

**Another OpenAI-compatible model provider** needs three changes:
1. the provider's API host in `harness/harbor/task/environment/egress/allowlist`
   (one regex per line) — otherwise the sandbox cannot reach it — and in
   `ALLOWED_HOSTS` in `harness/scripts/summarize.py`;
2. the endpoint and key for each agent, through the job YAML's `env:`
   block: iCode reads `ICODE_PROVIDER` (`deepseek-openai` or `openai`),
   `ICODE_BASE_URL`, `DEEPSEEK_API_KEY`/`OPENAI_API_KEY`; jiuwenswarm reads
   `API_BASE`, `API_KEY`, `MODEL_PROVIDER`, `ENDPOINT_PROFILE`
   (see `harness/harbor/agents/gb_agents.py`);
3. `GB_PO_BASE_URL` / `GB_PO_MODEL` / `GB_PO_API_KEY` for the product owner.

Run both smoke tests again after any change.

**Adding a third agent.** An agent is any program that can run headless in
the container. You need:
1. an install step in the Dockerfile, *before* the `COPY scaffold.tar` line
   (so the layer is cached), into `/opt/<agent>`;
2. an adapter `harness/agents/<agent>/launch.*` that loops the agent until
   it is killed: first invocation with the contents of `TASK.md`, later ones
   in the same session with `harness/agents/protocol/continue.txt` (or
   `recheck.txt` after the agent reported completion), writing its logs to
   `$GB_TRAJECTORY_DIR` — copy the iCode or jiuwenswarm adapter as a
   template and read `harness/agents/protocol/README.md`;
3. a class in `harness/harbor/agents/gb_agents.py` (like `ICodeAgent`), the
   adapter directory added to the Dockerfile's `cp -r … /opt/gb-agents/`
   line, and a job YAML;
4. configuration only for headless, offline operation (auto-approve tools,
   disable web tools); nothing task-specific in its system prompt.

## 8. What to expect

| | |
|---|---|
| Time | first image build 15–30 min; each smoke test 20 min + 2–5 min grading; each real trial 48 h + up to 2.5 h grading |
| Cost (DeepSeek `deepseek-flash`, list prices) | smoke: about $0.15–0.30 per agent; 48 h trial: roughly $20–30 per agent (estimated from 20-minute runs; over 90 % of input tokens are cache hits) |
| Disk | ~7 GB task image + a few GB per trial (kept: `delete: false`) |
| Early behaviour seen so far | both agents asked all six Open Issues in their first minutes, then planned and started building; after 20 minutes neither had graded progress committed |
| Scores | a reward is the weighted share of the GEP's acceptance checks the last commit passes (0–1); a perfect score is not expected — some PPU tests are beyond even mature emulators |

## 9. Housekeeping and troubleshooting

**Keep the machine awake.** On macOS: AC power, `caffeinate -ims -t 190000 &`
in the shell that starts the run, and either keep the lid open or use an
external display. On battery, macOS sleeps regardless of `caffeinate -s`; a
sleeping host pauses the containers and the model connections, and the trial
should be treated as invalid (`BENCHMARK.md` §4).

**Clean up** after you have collected the results (trials keep their
containers and volumes for inspection):

```sh
docker ps -a --format '{{.Names}}' | grep '^task__' | xargs -r docker rm -f
docker volume ls --format '{{.Name}}' | grep '^task__' | xargs -r docker volume rm
docker image prune -f
```

| Symptom | Cause / fix |
|---|---|
| `docker: 'compose' is not a docker command` | plugins not found: run the `cliPluginsExtraDirs` snippet in §2.2 |
| `egress` container unhealthy, trial never starts | the proxy cannot reach the model API from your network (corporate proxy/firewall?): check `curl https://api.deepseek.com` from the host |
| image build fails cloning or installing an agent | build-time network (GitHub, PyPI, gitcode.com); retry, or build behind a network that reaches them |
| `sync.sh: games: ROMs without goldens …` | the verifier assets are stale: `harness/scripts/build_assets.sh`, then `sync.sh` again |
| `ModuleNotFoundError: gb_agents` | `export PYTHONPATH=$PWD/harness/harbor/agents` in that shell |
| agent makes no progress, adapter log shows API errors | key, model ID or rate limit — check with the `curl …/models` call in §2.4; an outage longer than 30 min makes the trial invalid |
| Harbor was interrupted (Ctrl-C, reboot) | the trial cannot be resumed; clean up and run it again |
| `warning: uncommitted changes are NOT included in scaffold.tar` | `sync.sh` packs the committed `HEAD`; commit (or stash) first if you changed the scaffold |
