# Agent adapters

The study is black-box: each coding agent receives the same `TASK.md`, the
same product-owner channel and the same 48 hours, in its shipped
configuration. An adapter does only what is needed to run the agent
headlessly in the sandbox and to keep it working for the whole window —
both agents exit when they believe they are done, so each adapter runs an
outer loop that resumes the session with a "continue" prompt. That loop is
part of the measurement: how an agent behaves on its 40th resume is the
long-horizon question.

| | iCode (`chrys`) | jiuwenswarm |
|---|---|---|
| Source | https://github.com/openJiuwen-ai/iCode | https://github.com/openJiuwen-ai/jiuwenswarm |
| Headless entry | `icode run --task TASK.md -a LongRun -m gbmodel00001 -C /work --json`, then `icode run "<continue>" -s <session>` | `jiuwenswarm-process --run-jsonl` via the bundled Python SDK, then the same with `session_id` |
| Model config | `~/.chrys/models/gbmodel00001.yaml` (provider `deepseek-openai`, key from `DEEPSEEK_API_KEY`) | `<data>/config/.env` (`API_BASE`, `API_KEY`, `MODEL_NAME`, `MODEL_PROVIDER=OpenAI`, `ENDPOINT_PROFILE=deepseek`) |
| What the adapter changes from defaults | drops `ask_user` + `doc_converter` (interactive-only); auto-loads `TASK.md`/`QUESTIONS.md` as memory files; appends the long-run protocol to the Code agent's instructions; web tools off | language `en`; web tools removed from code mode (no network); debug trace + OTel file export on; MCP servers empty; long-run protocol passed as agent instructions |
| Mid-run questions | `ask_user` is stripped in headless mode anyway; protocol says use `QUESTIONS.md` | every interaction card is auto-answered (approve / session_allow / "no human available") so plan-approval or confirmation cards never block |
| Where its state goes | `$GB_TRAJECTORY_DIR/{home,chrys}` — sessions, `trajectory/events.jsonl`, compactions | `$GB_TRAJECTORY_DIR/jw` — sessions, checkpoints, `.code/traces/dump-code-*.txt`, `jw_otel/`; plus `jw_events_*.jsonl` (full event stream) |
| Verified here | yes, end to end with the mock provider under the orchestrator (session resume across a chaos kill) | config patching, SDK import and auto-answer policy only — the engine (`openjiuwen`) is fetched from gitcode.com, unreachable from this sandbox |

## Sandbox image additions

Each agent must be installed in the image (both need network at install
time; the run itself is `--network none` apart from the LLM endpoint — see
below).

```dockerfile
# iCode: Python 3.14 via uv
RUN git clone https://github.com/openJiuwen-ai/iCode /opt/icode && cd /opt/icode && uv sync --no-dev
# jiuwenswarm: Python 3.11–3.13; pulls openjiuwen from gitcode.com
RUN git clone https://github.com/openJiuwen-ai/jiuwenswarm /opt/jiuwenswarm && cd /opt/jiuwenswarm \
    && uv venv && uv pip install -e . && uv pip install ./sdks/python
```

**LLM egress.** `--network none` blocks the model endpoint too. Run the
container with a network that allows only the API host (an egress proxy
with an allowlist, or `--network` to a bridge whose iptables permit just
`api.deepseek.com:443`). Everything else — crates.io, GitHub, PyPI — must
stay blocked; that is the anti-copying control.

## Launch

```sh
# iCode
export ICODE_DIR=/opt/icode DEEPSEEK_API_KEY=...
python3 harness/orchestrator/run.py --arm icode --agent-cmd "bash /opt/harness/agents/icode/launch.sh"

# jiuwenswarm
export JW_DIR=/opt/jiuwenswarm API_KEY=...
python3 harness/orchestrator/run.py --arm jiuwenswarm --agent-cmd "python3 /opt/harness/agents/jiuwenswarm/launch.py"
```

(`/opt/harness` = this `harness/` directory mounted into the container;
the orchestrator passes `--agent-cmd` through unchanged.)

## Smoke test before the real run

```sh
# iCode, no API calls, 3 minutes, exercises the resume loop and a chaos kill
ICODE_DIR=/opt/icode ICODE_PROVIDER=mock ICODE_CONTINUE_SLEEP_SEC=20 \
python3 harness/orchestrator/run.py --arm icode --backend local \
  --agent-cmd "bash $PWD/harness/agents/icode/launch.sh" \
  --hours 0.05 --snapshot-hours 0.04 --chaos-hour 0.02 --chaos-downtime-min 0.1 --no-po --grade-tiers 0

# jiuwenswarm, real model, 10 minutes — the first thing to run on a machine that can install it
JW_DIR=/opt/jiuwenswarm API_KEY=... \
python3 harness/orchestrator/run.py --arm jiuwenswarm --backend local \
  --agent-cmd "python3 $PWD/harness/agents/jiuwenswarm/launch.py" \
  --hours 0.17 --snapshot-hours 0.1 --chaos-hour 0.08 --chaos-downtime-min 0.5 --grade-tiers 0
```

Things to look at in the jiuwenswarm smoke test (unverifiable from here):
`jw_adapter.jsonl` should show `invocation.end` with `exit_code: 0` and a
`session` id; `interaction.answered` lines show what cards came up; if the
agent's `skill_retrieval`/`skill_toolkit` tools try to reach the network,
remove them from `modes.code.tools` in `launch.py`.

## Adapter log formats

Both adapters write `<name>_adapter.jsonl` into the trajectory dir with
`adapter.start`, `invocation.end` (rc / status, session, `done`), and
`session.abandoned` (three consecutive failed resumes → fresh session on the
current repo state). Those events, joined with the orchestrator's
`events.jsonl`, give the resume count, the done/not-done oscillation and
the abandoned-session count per run — the stability metrics of the study.
