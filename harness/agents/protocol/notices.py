#!/usr/bin/env python3
"""Delivery of newly filed issues to the agent — shared by every adapter.

During a run the product-owner sidecar files new issues in waves: it copies
them into /work/ISSUES and writes a notice, NOTICES_DIR/wave-N.json. A file in
the repository is not enough for a headless agent — it may never look again —
so the adapter delivers every notice as the agent's next prompt:

  * between invocations: the next prompt is the notice (instead of the
    continue/recheck prompt);
  * during an invocation: the watcher interrupts it at the first commit made
    after the release (the work is saved), or at the latest `max_wait`
    seconds after the release, and the adapter resumes the same session with
    the notice. An interruption is not counted as a failed invocation.

Each delivery writes NOTICES_DIR/wave-N.delivered.json (when, how, how long
after the release); the sidecar logs it and the verifier scores a wave's
hidden checks only if it was delivered. Stdlib only: it runs under each
agent's own Python.

    notices.py take  --traj DIR --log FILE          print the prompt for pending notices (exit 1: none)
    notices.py watch --traj DIR --log FILE --pidfile FILE --max-wait SEC
    notices.py idle  --log FILE                      the agent reported the work complete
"""
from __future__ import annotations

import argparse
import json
import os
import signal
import subprocess
import time
from pathlib import Path

NOTICES = Path(os.environ.get("GB_NOTICES_DIR", "/notices"))
WORK = Path(os.environ.get("GB_WORK", "/work"))
HERE = Path(__file__).resolve().parent
INTERRUPT_FLAG = "notice_interrupt"          # in the adapter's trajectory dir


def logger(path):
    def log(event, **fields):
        with open(path, "a") as f:
            f.write(json.dumps({"t": round(time.time(), 3), "event": event, **fields}) + "\n")
    return log


def pending():
    out = []
    for f in sorted(NOTICES.glob("wave-*.json")):
        if f.name.endswith(".delivered.json") or (NOTICES / (f.stem + ".delivered.json")).exists():
            continue
        try:
            out.append(json.loads(f.read_text()))
        except (OSError, ValueError):
            pass                                  # being written; picked up next time
    return out


def render(notices):
    items = [f"- #{i['number']} {i['title']} — {i['file']}" for n in notices for i in n["issues"]]
    return (HERE / "new_issues.txt").read_text().replace("{issues}", "\n".join(items)).strip()


def take(traj: Path, log):
    """The prompt announcing every pending wave ("" if none); marks them delivered."""
    notices = pending()
    flag = traj / INTERRUPT_FLAG
    method = flag.read_text().strip() if flag.exists() else "between_invocations"
    flag.unlink(missing_ok=True)
    if not notices:
        return ""
    now = time.time()
    for n in notices:
        rec = {"wave": n["wave"], "released_at": n["released_at"], "delivered_at": round(now, 3),
               "delay_sec": round(now - n["released_at"], 1), "method": method,
               "issues": [i["number"] for i in n["issues"]]}
        (NOTICES / f"wave-{n['wave']}.delivered.json").write_text(json.dumps(rec))
        log("notice.delivered", **rec)
    return render(notices)


def head_commit_time():
    try:
        out = subprocess.run(["git", "-C", str(WORK), "log", "-1", "--format=%ct"],
                             capture_output=True, text=True, timeout=30).stdout.strip()
        return int(out) if out else 0
    except (OSError, ValueError, subprocess.SubprocessError):
        return 0


def descendants(root: int):
    children = {}
    for d in os.listdir("/proc"):
        if d.isdigit():
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


def kill_tree(pid: int):
    for p in list(reversed(descendants(pid))) + [pid]:
        try:
            os.kill(p, signal.SIGKILL)
        except OSError:
            pass


def watch(traj: Path, log, running, kill, max_wait: float, poll: float = 10.0, stop=lambda: False):
    """Interrupt the running invocation so a pending notice is delivered: at the
    first commit after the release, or `max_wait` seconds after it."""
    while not stop():
        time.sleep(poll)
        notices = pending()
        if not notices or not running():
            continue
        released = min(n["released_at"] for n in notices)
        age = time.time() - released
        reason = "after_commit" if head_commit_time() >= released else ("timeout" if age >= max_wait else None)
        if not reason:
            continue
        (traj / INTERRUPT_FLAG).write_text(reason)
        log("notice.interrupt", reason=reason, waves=[n["wave"] for n in notices], age_sec=round(age, 1))
        kill()
        while pending() and not stop():       # one interruption per delivery
            time.sleep(poll)


def idle(log):
    """The agent reported the work complete: the sidecar may file the next wave early."""
    NOTICES.mkdir(parents=True, exist_ok=True)
    t = round(time.time(), 3)
    (NOTICES / f"idle-{int(t)}.json").write_text(json.dumps({"t": t}))
    log("agent.idle")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("cmd", choices=("take", "watch", "idle"))
    ap.add_argument("--traj", type=Path, default=Path("."))
    ap.add_argument("--log", required=True)
    ap.add_argument("--pidfile", type=Path)
    ap.add_argument("--max-wait", type=float, default=1800)
    a = ap.parse_args()
    log = logger(a.log)
    if a.cmd == "take":
        prompt = take(a.traj, log)
        print(prompt)
        return 0 if prompt else 1
    if a.cmd == "idle":
        idle(log)
        return 0

    def pid():
        try:
            return int(a.pidfile.read_text().strip())
        except (OSError, ValueError, AttributeError):
            return 0
    watch(a.traj, log, running=lambda: pid() > 0, kill=lambda: kill_tree(pid()), max_wait=a.max_wait)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
