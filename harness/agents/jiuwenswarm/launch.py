#!/usr/bin/env python3
"""jiuwenswarm adapter for the orchestrator (black-box run).

    --agent-cmd "python3 /path/to/harness/agents/jiuwenswarm/launch.py"

Runs in the workspace (cwd = repo with TASK.md). Keeps a jiuwenswarm code
agent working until killed:
  * first invocation: `jiuwenswarm-process --run-jsonl` with TASK.md as input
  * later invocations: same session_id with a "continue" prompt
  * every interaction card (plan approval, confirmations, ask_user) is
    answered automatically — approve / proceed / "no human available", so the
    agent never blocks; requirement questions go through QUESTIONS.md
  * the full JSONL event stream is written to GB_TRAJECTORY_DIR

Required env:
  JW_DIR            checkout of https://github.com/openJiuwen-ai/jiuwenswarm, installed
                    (uv venv && uv pip install -e .) — its .venv/bin/jiuwenswarm-process is used
  API_KEY           model API key
Optional:
  API_BASE          default https://api.deepseek.com
  MODEL_NAME        default deepseek-flash
  MODEL_PROVIDER    default OpenAI        ENDPOINT_PROFILE default deepseek
  JW_CONTINUE_SLEEP_SEC   pause after a run that reports completion (default 600)

Setup is idempotent: $GB_TRAJECTORY_DIR/jw holds JIUWENSWARM_HOME/DATA_DIR,
so all of jiuwenswarm's state (sessions, checkpoints, traces, logs) travels
with the snapshots.
"""
import asyncio
import json
import os
import signal
import subprocess
import sys
import time
from pathlib import Path

WORK = Path.cwd()
TRAJ = Path(os.environ.get("GB_TRAJECTORY_DIR", ".trajectory")).resolve()
JW_DIR = Path(os.environ["JW_DIR"]).resolve()
VENV = JW_DIR / ".venv" / "bin"
JW_HOME = TRAJ / "jw"
DATA = JW_HOME / ".jiuwenswarm"
LOG = TRAJ / "jw_adapter.jsonl"
STATE = TRAJ / "jw_state"

sys.path.insert(0, str(JW_DIR / "sdks" / "python" / "src"))
from jiuwenswarm_sdk.client import Client  # noqa: E402

CONTINUE_PROMPT = (
    "Continue working on the task in TASK.md. First check QUESTIONS.md for new answers "
    "from the product owner, "
    "and git log / test results for the current state. Keep going until everything is "
    "complete and verified; say DONE only then."
)
RECHECK_PROMPT = (
    "Re-verify the project against TASK.md and GEP-0001.md: look for new product-owner "
    "answers, run every check in TESTING.md and the GEP acceptance table, fix any regressions, "
    "and close remaining gaps. If everything passes and nothing is left, say DONE."
)
PROTOCOL = """
Long-running task protocol: there is no human watching. You will be re-invoked with
"continue" prompts; each time re-read TASK.md, check QUESTIONS.md for new product-owner
answers, and carry on from the repository's current state — never start over.
Requirement questions: append "## Q: ..." to QUESTIONS.md and keep working on something
else while the answer arrives. Commit after each coherent piece of work. Say DONE only
when everything is complete and verified.
"""


def log(event, **f):
    with LOG.open("a") as fh:
        fh.write(json.dumps({"t": time.time(), "event": event, **f}) + "\n")


def env_for_child():
    e = dict(os.environ)
    e.update({
        "JIUWENSWARM_HOME": str(JW_HOME),
        "JIUWENSWARM_DATA_DIR": str(DATA),
        "HOME": str(JW_HOME),
        "PATH": f"{VENV}:{e.get('PATH', '')}",
    })
    return e


def setup():
    """Init the data dir once; write .env and patch config.yaml."""
    import yaml  # provided by jiuwenswarm's venv when run with it; fall back to system

    JW_HOME.mkdir(parents=True, exist_ok=True)
    cfg_dir = DATA / "config"
    if not (cfg_dir / "config.yaml").exists():
        subprocess.run([str(VENV / "jiuwenswarm-init")], env=env_for_child(), stdin=subprocess.DEVNULL,
                       check=False, capture_output=True, text=True)
    if not (cfg_dir / "config.yaml").exists():
        sys.exit("jiuwenswarm-init did not create config/config.yaml")

    # .env is loaded with override=True by jiuwenswarm, so write the real values here.
    (cfg_dir / ".env").write_text(
        f'API_BASE="{os.environ.get("API_BASE", "https://api.deepseek.com")}"\n'
        f'API_KEY="{os.environ["API_KEY"]}"\n'
        f'MODEL_NAME="{os.environ.get("MODEL_NAME", "deepseek-flash")}"\n'
        f'MODEL_PROVIDER={os.environ.get("MODEL_PROVIDER", "OpenAI")}\n'
        f'ENDPOINT_PROFILE={os.environ.get("ENDPOINT_PROFILE", "deepseek")}\n'
    )

    p = cfg_dir / "config.yaml"
    c = yaml.safe_load(p.read_text())

    def setk(path, value):
        cur = c
        keys = path.split(".")
        for k in keys[:-1]:
            cur = cur.setdefault(k, {})
        cur[keys[-1]] = value

    # Only what a headless, offline, English, logged run needs. Agent behaviour
    # (memory, task loop, context engine, sub-agents) stays at defaults.
    setk("preferred_language", "en")
    setk("telemetry.enabled", False)
    setk("debug_trace.code.enabled", True)
    setk("debug_trace.limits.tool_result_max_chars", 20000)
    setk("debug_trace.limits.tool_args_max_chars", 8000)
    setk("agent_observability.enabled", True)
    setk("agent_observability.exporter", "file")
    setk("agent_observability.traces_dir", str(TRAJ / "jw_otel"))
    setk("react.context_engine_config.enable_tiktoken_counter", False)
    setk("mcp.servers", [])
    # No network in the sandbox: drop the web tools; keep the local ones.
    tools = c.get("modes", {}).get("code", {}).get("tools", []) or []
    setk("modes.code.tools", [t for t in tools if not t.startswith("web_")])
    p.write_text(yaml.safe_dump(c, sort_keys=False, allow_unicode=True))
    log("setup.done", config=str(p))


async def answer_interaction(record):
    """Auto-answer any card so the run never blocks on a human."""
    payload = record.get("payload", {}) or {}
    card = payload.get("interaction", {}) or {}
    questions = card.get("questions") or []
    if not questions and card.get("question"):
        questions = [card]
    answers = []
    for q in questions:
        options = q.get("options") or []
        values = []
        for o in options:
            v = o.get("value") if isinstance(o, dict) else o
            label = o.get("label") if isinstance(o, dict) else o
            values.append((str(v or label), str(label or v)))
        pick = None
        for pref in ("session_allow", "approve", "allow_once", "accept", "proceed", "execute"):
            for v, lab in values:
                if pref in v.lower() or pref in lab.lower():
                    pick = v
                    break
            if pick:
                break
        if pick is None and values:
            pick = values[0][0]
        ans = {
            "question": q.get("question", ""),
            "selected_options": [pick] if pick else [],
            "custom_input": "" if pick else
            "No human is available. Proceed with your best judgement; put requirement questions in QUESTIONS.md as '## Q:' headings.",
        }
        if q.get("card_id"):
            ans["card_id"] = q["card_id"]
        answers.append(ans)
    log("interaction.answered", kind=card.get("type") or card.get("kind"), n=len(answers),
        picks=[a["selected_options"] for a in answers])
    return answers


# Session id of the run in progress, learned from the event stream as soon as
# the runtime announces it, so a crash or chaos kill can still be resumed.
CURRENT = {"session": ""}
STREAMING_EVENTS = {"chat.reasoning", "chat.delta"}
STREAM_COUNTS = {}


async def one_run(session_id, prompt, events_path):
    # The SDK appends --run-jsonl itself; passing it here too makes the process
    # reject the run ("cannot be combined with legacy execution arguments").
    client = Client([str(VENV / "jiuwenswarm-process")], cwd=str(WORK), env=env_for_child())
    req = {
        "input": prompt,
        "mode": "agent.code.normal",
        "workspace": {"cwd": str(WORK), "project_dir": str(WORK), "trusted_dirs": [str(WORK)]},
        "agent": {"name": "coder", "instructions": PROTOCOL.strip()},
    }
    if session_id:
        req["session_id"] = session_id
    with events_path.open("a") as fh:
        async def on_event(rec):
            sid = rec.get("session_id")
            if sid and sid != CURRENT["session"]:
                CURRENT["session"] = sid
                STATE.write_text(sid)
            et = rec.get("event_type")
            # Streaming deltas arrive one token at a time (~8k/min of reasoning);
            # count them instead of logging each one, or a 48 h log runs to GBs.
            if et in STREAMING_EVENTS:
                STREAM_COUNTS[et] = STREAM_COUNTS.get(et, 0) + 1
                return
            fh.write(json.dumps(rec) + "\n")
            fh.flush()
        result = await client.run(req, on_event=on_event, on_interaction=answer_interaction)
    return result


# ---- process control --------------------------------------------------------
# Never match agent processes by name (they may rename themselves): act on the
# process tree below this adapter instead.
def descendants(root):
    children = {}
    for d in os.listdir("/proc"):
        if not d.isdigit():
            continue
        try:
            with open(f"/proc/{d}/stat") as f:
                ppid = int(f.read().rsplit(")", 1)[1].split()[1])
            children.setdefault(ppid, []).append(int(d))
        except (OSError, ValueError, IndexError):
            continue
    out, stack = [], [root]
    while stack:
        for c in children.get(stack.pop(), []):
            out.append(c)
            stack.append(c)
    return out


def kill_descendants():
    pids = descendants(os.getpid())
    for pid in reversed(pids):          # leaves first
        try:
            os.kill(pid, signal.SIGKILL)
        except OSError:
            pass
    return pids


def on_term(signum, frame):
    log("adapter.stop", signal=signum)
    kill_descendants()
    os._exit(143)


def chaos_watchdog():
    """Once, GB_CHAOS_AFTER_SEC after start, SIGKILL the running agent (its whole tree)."""
    after = int(os.environ.get("GB_CHAOS_AFTER_SEC", "0") or 0)
    marker = TRAJ / "chaos_done"
    if after <= 0 or marker.exists():
        return
    time.sleep(after)
    marker.touch()
    log("chaos.kill", pids=kill_descendants())


def main():
    TRAJ.mkdir(parents=True, exist_ok=True)
    signal.signal(signal.SIGTERM, on_term)
    signal.signal(signal.SIGINT, on_term)
    setup()
    import threading
    threading.Thread(target=chaos_watchdog, daemon=True).start()
    session = STATE.read_text().strip() if STATE.exists() else ""
    log("adapter.start", arm=os.environ.get("GB_ARM"), resume_session=session)
    fail_streak = 0
    last_done = False
    while True:
        if not session:
            prompt = (WORK / "TASK.md").read_text() + "\n" + PROTOCOL
        else:
            prompt = RECHECK_PROMPT if last_done else CONTINUE_PROMPT
        events_path = TRAJ / f"jw_events_{int(time.time())}.jsonl"
        CURRENT["session"] = session
        STREAM_COUNTS.clear()
        sub = WORK / "SUBMISSION.md"
        sub_before = sub.stat().st_mtime if sub.exists() else 0.0
        try:
            result = asyncio.run(one_run(session, prompt, events_path))
        except Exception as e:  # transport/protocol errors, or the child was killed
            result = {"status": "adapter_error", "exit_code": -1, "error": repr(e)[:500]}
        sid = result.get("session_id") or CURRENT["session"] or session
        recovered = bool(sid) and not result.get("session_id")
        if sid:
            session = sid
            STATE.write_text(session)
        output = str(result.get("output") or "")
        # "Done" = DONE in the answer, or SUBMISSION.md (re)written during THIS
        # invocation; one left over from an earlier phase does not count.
        sub_after = sub.stat().st_mtime if sub.exists() else 0.0
        last_done = "DONE" in output or sub_after != sub_before
        ok = result.get("exit_code") == 0
        log("invocation.end", status=result.get("status"), exit_code=result.get("exit_code"),
            session=session, session_recovered=recovered, done=last_done, usage=result.get("usage"),
            error=result.get("error"), events=events_path.name, streamed=dict(STREAM_COUNTS))
        if not ok:
            fail_streak += 1
            if fail_streak >= 3:
                log("session.abandoned", after_failures=fail_streak, session=session)
                session = ""
                STATE.unlink(missing_ok=True)
                fail_streak = 0
                time.sleep(30)
            else:
                time.sleep(10 * fail_streak)
            continue
        fail_streak = 0
        time.sleep(int(os.environ.get("JW_CONTINUE_SLEEP_SEC", "180")) if last_done else 5)


if __name__ == "__main__":
    main()
