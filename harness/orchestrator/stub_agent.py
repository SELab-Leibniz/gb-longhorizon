#!/usr/bin/env python3
"""Stub agent: exercises the orchestrator without a real framework.

Behaviour, in order:
  1. append a question to QUESTIONS.md, wait for an answer (up to 5 min)
  2. implement a trivial change and commit
  3. every 30 s: touch a file, commit, write a trajectory line
  4. on its 4th iteration, exit with code 3 (simulated crash)
  5. after restart (detected via a marker file) keep looping forever

Reads GB_ARM / GB_DISABLED_MODULES / GB_TRAJECTORY_DIR like a real agent.
"""
import json
import os
import subprocess
import sys
import time
from pathlib import Path

WORK = Path.cwd()
TRAJ = Path(os.environ.get("GB_TRAJECTORY_DIR", "/tmp")) / "stub.jsonl"
ARM = os.environ.get("GB_ARM", "?")
DISABLED = os.environ.get("GB_DISABLED_MODULES", "")
MARKER = WORK / ".stub_restarted"


def git(*args):
    subprocess.run(["git", "-c", "user.email=stub@x", "-c", "user.name=Stub", *args], cwd=WORK, check=True,
                   capture_output=True)


def traj(**fields):
    with TRAJ.open("a") as f:
        f.write(json.dumps({"t": time.time(), **fields}) + "\n")


def ask(q):
    qf = WORK / "QUESTIONS.md"
    qf.write_text(qf.read_text().rstrip("\n") + f"\n\n## Q: {q}\n")
    traj(event="ask", q=q)
    for _ in range(30):
        time.sleep(10)
        text = qf.read_text()
        if text.rstrip().endswith("\n## A:") or "## A:" in text.split(f"## Q: {q}")[-1]:
            ans = text.split(f"## Q: {q}")[-1].split("## A:")[-1].strip()
            traj(event="answer", a=ans[:200])
            return ans
    traj(event="answer_timeout")
    return None


def main():
    traj(event="start", arm=ARM, disabled=DISABLED, restarted=MARKER.exists())
    first_run = not MARKER.exists()
    if first_run and "clarification" not in DISABLED:
        ask("Do we need to support Game Boy Color, or is the original Game Boy enough?")

    i = 0
    while True:
        i += 1
        p = WORK / "gb-core" / "src" / "util.rs"
        p.write_text(p.read_text() + f"\n// stub edit {i} ({ARM})\n")
        git("add", "-A")
        git("commit", "-q", "-m", f"stub: edit {i}")
        traj(event="commit", i=i)
        if first_run and i == 4:
            MARKER.touch()
            traj(event="simulated_crash")
            sys.exit(3)
        time.sleep(30)


if __name__ == "__main__":
    main()
