# The gb benchmark — operating protocol

How to run the case study so that results for different coding agents are
comparable, repeatable and auditable. `harbor/README.md` covers the
mechanics (setup, commands, trial directory layout); this document is the
protocol around them.

## 1. What is measured

**One task, one environment, one model, each agent as shipped.** Every
agent receives the same task statement (`AGENT_BRIEF.md`, delivered
verbatim as `TASK.md` and as the first prompt), the same repository at the
same commit, the same 48 hours, and the same model. The agents are black
boxes: the harness does not add, enable or test any particular long-horizon
technique (planning, memory, compaction, sub-agents, recovery). Whatever
each agent does to cope with a 48-hour job is part of what is measured.

| | |
|---|---|
| **Primary metric** | `reward` in the verifier's `reward.json`: the weighted score (0–1) of the **last commit at the 48-hour mark**, graded from a clean checkout by hidden suites. Weights are in `AGENT_BRIEF.md` (the agents see them) and in `harbor/task/tests/test.sh`. |
| **Breakdown** | `phase_core`, `phase_cgb`, `phase_ppu`, `phase_tooling`, `phase_portability`, `phase_library`, `phase_front_end` (each 0–1), and every individual check in `results.json`. |
| **Reported, not scored** | product-owner questions and which Open Issues were asked (and whether before the first product commit), tokens and cost, commits, agent wall-clock, `SUBMISSION.md`, screenshots, the progress curve from 2-hourly snapshots. |

## 2. Fixed conditions

Record these for every campaign (most are captured automatically in each
trial's `verifier/environment.json` and `config.json`).

| Condition | Value | Where it is fixed |
|---|---|---|
| Task + scaffold | `GEP-0001.md` + the repository at one commit of branch `full-spec` | `sync.sh` bakes `git archive HEAD` into the image; it prints the commit |
| Agents | iCode `ICODE_COMMIT`, jiuwenswarm `JW_COMMIT` | `harbor/task/environment/Dockerfile` |
| Agent protocol | first prompt = `TASK.md`; identical continue/recheck prompts; no system-prompt or memory additions | `agents/protocol/` (see its README for the only per-agent settings: headless approval, no web tools, telemetry off) |
| Model | `deepseek-flash`, one API account for all agents | job YAMLs (`model_name`), keys in `~/.gb-keys.env` |
| Product owner | `deepseek-flash`, temperature 0.2, answers from GEP 1 + `HIDDEN_SPEC.md` | `orchestrator/po_agent.py`, `task.toml` |
| Time | 48 h agent (`run_seconds` 172 500 + Harbor margin), ≤ 150 min verifier | job YAMLs, `task.toml` |
| Resources | 4 CPU, 16 GB RAM, 40 GB disk per trial | `task.toml` |
| Network | only `api.deepseek.com` (tinyproxy allowlist); no network at all during grading | `environment/egress/` |
| Toolchain | Rust 1.97.0 + targets; Node 18; Chromium; Python 3 + websocket | Dockerfile (Debian bookworm packages) |
| Harness | Harbor 0.23.0 | `uv tool install harbor` version |
| Interruptions | none (`chaos_after_sec: 0`) | job YAMLs |

**Build the image once per campaign and reuse it.** The Debian packages
(Chromium, Node) are not version-pinned, so a rebuild weeks later can change
them. After the first build, record the image id
(`docker image inspect --format '{{.Id}}' <task image>`); `environment.json`
in every trial records the versions actually used — compare them across
trials.

## 3. Procedure

1. **Set up** as in `harbor/README.md` (Docker, keys, `PYTHONPATH`), then
   `harness/harbor/sync.sh` (builds the verifier's pristine assets once,
   stages the task). Commit nothing after this point for the campaign.
2. **Smoke-test each agent** (`jobs/smoke.yaml`, `jobs/smoke-jiuwenswarm.yaml`,
   20 minutes each) and check the list in `harbor/README.md`: the image
   builds, the egress log shows only the model host, the product owner
   answers, the verifier writes `reward.json` with all phases.
3. **Run the trials**: `jobs/icode.yaml` and `jobs/jiuwenswarm.yaml`, each
   with `n_attempts: 3` (one after another within a job). Prefer running the
   two jobs **at the same time** on a host with ≥ 8 CPUs and ≥ 32 GB of RAM
   available to Docker, so both agents see the same model-API conditions;
   otherwise alternate them (`n_attempts: 1`, icode, jiuwenswarm, icode, …).
   Never run trials of one agent while the other agent's trials compete for a
   host that cannot give both their full 4 CPU / 16 GB.
4. **Do not intervene** during a trial: no answering questions (the product
   owner does that), no restarting agents, no edits to the workspace, no
   extra prompts. Passive monitoring only (`docker logs`, the sidecar's
   `events.jsonl`).
5. **After each trial** the verifier grades automatically. Collect the
   results with `harness/scripts/summarize.py jobs/gb-icode jobs/gb-jiuwenswarm`
   (add `--price-in/--price-cached/--price-out` for cost).

## 4. Validity and re-runs

A trial is **invalid** — excluded from the statistics and re-run — only for
an infrastructure fault, decided from the evidence below *before looking at
the score*:

| Fault | Evidence |
|---|---|
| Harbor or the verifier failed | `exception_info` in `result.json`, or no `verifier/reward.json` (`summarize.py`: valid ✗) |
| Sandbox breach | a connection established to any host but the model API (`summarize.py`: egress ✗) — impossible by design; investigate before continuing |
| Model API outage | the API failed for more than 30 minutes in a row (HTTP errors in the agent trajectory / adapter log, or the provider's status page) |
| Host failure | container OOM-killed by the host, disk full, Docker daemon restart, machine sleep |
| Product owner wrong | an answer in `po-artifacts/po_log.jsonl` contradicts `HIDDEN_SPEC.md` on a point a hidden check depends on |

Everything the agent does is **valid and counts**: crashes and restarts of
the agent itself, stalls, loops, stopping early, a broken build, editing a
frozen file (reward 0), not asking the product owner, not writing
`SUBMISSION.md`. **Never re-run a trial because of its score.** Report every
trial, including invalid ones with the reason.

## 5. Scoring

`test.sh` grades the last commit (`git archive HEAD`, pristine ROMs from
`/tests`, uploaded only after the agent stops):

| Tier | Grader | Feeds |
|---|---|---|
| 0 | build of `gb-core` + `gb-cli` (gate), frozen files (gate), clippy/rustfmt over the workspace | 0 if a gate fails; `lint_clean` |
| 1–4 | Blargg, Mooneye, dmg-acid2, DMG games (timing-robust goldens), save states, determinism | `phase_core` |
| 5 | Mooneye CGB, cgb-acid2, CGB games, `cgb_sound` | `phase_cgb` |
| 6 | Mealybug Tearoom DMG (+ Mooneye `ppu/`, `oam_bug` from tier 1) | `phase_ppu` |
| 7 | `gb-trace` vs reference traces, profile, `gb-server` (36-check suite) | `phase_tooling` |
| 8 | `no_std` build, `gb-wasm` in Node vs native hashes | `phase_portability` |
| 9 | `gb-web` (41-check API suite, incl. Open Issues), front end + player (21 checks, headless Chromium) | `phase_library`, `phase_front_end` |

Graders are deterministic (fixed ROMs, inputs and frame counts; hashes, not
fuzzy image matching) except the player's real-time pacing check, which
accepts 50–70 fps. Every check was validated against a reference
implementation and against injected bugs (`harbor/README.md`). Hung
emulators cost one timeout per ROM and a family is skipped after three
hangs; every verifier step is capped, so `reward.json` is always written.

**Re-grading.** Each trial keeps `verifier/repo.bundle` (the full
repository as graded) and `verifier/environment.json`. To re-grade, clone
the bundle and run `grade.py` inside the task image (it needs Rust, Node and
Chromium) with the same `/tests` content (`harbor/task/tests/`). The 2-hourly
snapshots (`artifacts/po-artifacts/snapshots/*.bundle`) can be graded the
same way for progress curves (`scripts/grade_snapshots.sh` does this for the
emulator tiers on a host with cargo).

## 6. Reporting

* Per agent: number of valid trials, reward mean, standard deviation, min
  and max, and the mean of each phase — `summarize.py` prints exactly this.
* Per trial: reward, phases, validity, agent hours, commits, product-owner
  questions and Open Issues asked, tokens / cost.
* Qualitative: `SUBMISSION.md`, `verifier/screenshots/index.html`, the
  product-owner log, the progress curve.
* With three trials per agent, a difference smaller than the trials' spread
  is not a finding; say so.

## 7. Integrity

* The hidden suites, goldens, Open Issue answers and grader never enter the
  agent's container (`harness/` is deleted from the image; `/tests` is
  uploaded after the agent stops). Grading uses a clean export of the last
  commit and pristine ROMs, so editing tests or ROMs in the workspace cannot
  change the score.
* Network: only the model API is reachable, so nothing can be fetched or
  copied from the internet during the run.
* Training-data contamination cannot be excluded: the test ROMs, the
  community documentation and open-source emulators are public. Run
  `scripts/similarity.sh` (JPlag against well-known Rust emulators) on each
  final repository and report anything with structural identity across
  several modules.

## 8. Known limitations

* LLM sampling makes every trial different; three trials per agent bound,
  but do not remove, that variance.
* Results are for agent + model pairs; another model can change the order.
* Wall-clock time includes model-API latency, which varies with load; run
  the agents concurrently (§3) to share conditions.
* Mealybug Tearoom is not fully reachable even for a mature emulator
  (SameBoy matches 10 of 24); the PPU phase gives partial credit.
* The product owner is an LLM; its answers are logged and auditable (§4).
