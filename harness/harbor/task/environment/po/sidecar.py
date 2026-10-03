"""Product-owner + snapshot sidecar for the Harbor task.

Runs for the life of the trial:
  * every GB_PO_POLL_SEC: answer pending "## Q:" headings in /work/QUESTIONS.md
    (po_agent.ProductOwner, with a file-based backend on the shared volume)
  * issue waves (/po/waves/schedule.json): wave N is filed into /work/ISSUES
    at its time (`after_hours` x GB_WAVE_TIME_SCALE), or earlier if the agent
    reported the work complete since the previous wave. Filing writes a notice
    to /notices/wave-N.json, which the agent adapter delivers as the agent's
    next prompt (harness/agents/protocol/notices.py) and acknowledges with
    /notices/wave-N.delivered.json; filed issue files that disappear from
    /work/ISSUES are put back. The product owner learns a wave when it is filed.
  * every GB_SNAPSHOT_HOURS: `git bundle` the repo into
    /po-artifacts/snapshots/NNN_<unixtime>.bundle (graded post hoc on the
    host with harness/scripts/grade_snapshots.sh)
Everything it logs goes to /po-artifacts, which Harbor collects as an
artifact of the trial.
"""
import json
import os
import shutil
import subprocess
import threading
import time
from pathlib import Path

WORK = Path(os.environ.get("GB_WORK", "/work"))
ART = Path(os.environ.get("GB_PO_ARTIFACTS", "/po-artifacts"))
ART.mkdir(parents=True, exist_ok=True)
(ART / "snapshots").mkdir(exist_ok=True)


class FileBackend:
    """Adapts po_agent.ProductOwner to a plain directory (the shared volume)."""

    def __init__(self, work: Path):
        self.work = work  # attribute name matters: po_agent checks `hasattr(backend, "work")`

    def read_file(self, rel):
        p = self.work / rel
        return p.read_text() if p.exists() else ""

    def write_file(self, rel, content):
        tmp = self.work / (rel + ".tmp")
        tmp.write_text(content)
        tmp.replace(self.work / rel)


class EventLog:
    def __init__(self, path):
        self.path = path
        self.t0 = time.time()
        self.lock = threading.Lock()

    def __call__(self, kind, **fields):
        rec = {"t": round(time.time(), 3), "elapsed_h": round((time.time() - self.t0) / 3600, 4), "kind": kind, **fields}
        with self.lock, open(self.path, "a") as f:
            f.write(json.dumps(rec) + "\n")
        print(f"[{rec['elapsed_h']:6.2f}h] {kind} {json.dumps(fields)[:200]}", flush=True)


def snapshots(log, every_hours):
    n = 0
    while True:
        time.sleep(every_hours * 3600)
        n += 1
        out = ART / "snapshots" / f"{n:03d}_{int(time.time())}.bundle"
        r = subprocess.run(["git", "-C", str(WORK), "bundle", "create", str(out), "--all"], capture_output=True, text=True)
        commits = subprocess.run(["git", "-C", str(WORK), "rev-list", "--count", "HEAD"], capture_output=True, text=True).stdout.strip()
        has_submission = (WORK / "SUBMISSION.md").exists()
        log("snapshot", n=n, ok=r.returncode == 0, commits=commits, submission=has_submission, err=r.stderr[-200:] if r.returncode else "")


WAVES = Path("/po/waves")
NOTICES = Path(os.environ.get("GB_NOTICES_DIR", "/notices"))


def issue_info(path: Path):
    """Number, title and reporter of an issue file ("# 112 — Title", "**Reporter:** user · ...")."""
    text = path.read_text()
    head = text.splitlines()[0].lstrip("# ").strip()
    number, _, title = head.partition(" — ")
    reporter = next((l.split("**Reporter:**", 1)[1].split("·")[0].strip() for l in text.splitlines()
                     if l.startswith("**Reporter:**")), "")
    return {"number": number.strip(), "title": title.strip(), "file": f"ISSUES/{path.name}", "reporter": reporter}


def write_atomic(path: Path, text: str):
    tmp = path.with_name(path.name + ".tmp")
    tmp.write_text(text)
    tmp.replace(path)


def file_wave(wave, trigger, log, po):
    files = sorted((WAVES / f"w{wave}").glob("[0-9]*.md"))
    (WORK / "ISSUES").mkdir(exist_ok=True)
    infos = []
    for f in files:
        shutil.copyfile(f, WORK / "ISSUES" / f.name)
        infos.append(issue_info(f))
    readme = WORK / "ISSUES" / "README.md"
    if readme.exists():                         # the backlog table is the last thing in the file
        body = readme.read_text().rstrip("\n") + "\n"
        body += "".join(f"| {i['number']} | {i['title']} | {i['reporter']} |\n" for i in infos)
        write_atomic(readme, body)
    now = time.time()
    NOTICES.mkdir(parents=True, exist_ok=True)
    write_atomic(NOTICES / f"wave-{wave}.json",
                 json.dumps({"wave": wave, "released_at": round(now, 3), "trigger": trigger, "issues": infos}))
    po.release_wave(wave)
    log("wave.released", wave=wave, trigger=trigger, issues=[i["number"] for i in infos])
    return now


def waves(log, po, start):
    sched = json.loads((WAVES / "schedule.json").read_text())["waves"]
    scale = float(os.environ.get("GB_WAVE_TIME_SCALE", "1"))
    state_p = ART / "wave_state.json"
    state = json.loads(state_p.read_text()) if state_p.exists() else {"released": []}
    for r in state["released"]:                 # sidecar restarted: the product owner knows them again
        po.release_wave(r["wave"])
    delivered = set()
    log("waves.start", schedule=[(w["wave"], w["after_hours"]) for w in sched], scale=scale)
    while True:
        # put back released issue files that were removed from the repository
        for r in state["released"]:
            for f in sorted((WAVES / f"w{r['wave']}").glob("[0-9]*.md")):
                dst = WORK / "ISSUES" / f.name
                if not dst.exists():
                    (WORK / "ISSUES").mkdir(exist_ok=True)
                    shutil.copyfile(f, dst)
                    log("wave.file_restored", wave=r["wave"], file=f.name)
        # deliveries recorded by the agent adapter
        for f in NOTICES.glob("wave-*.delivered.json"):
            if f.name not in delivered:
                delivered.add(f.name)
                try:
                    log("wave.delivered", **json.loads(f.read_text()))
                except (OSError, ValueError):
                    delivered.discard(f.name)
        # the next wave: at its time, or early when the agent reported the work complete
        if len(state["released"]) < len(sched):
            nxt = sched[len(state["released"])]
            last = state["released"][-1]["t"] if state["released"] else start
            idle = any(float(json.loads(f.read_text()).get("t", 0)) > last for f in NOTICES.glob("idle-*.json"))
            sub = WORK / "SUBMISSION.md"
            submitted = sub.exists() and sub.stat().st_mtime > max(last, start)
            due = time.time() >= start + nxt["after_hours"] * 3600 * scale
            trigger = "schedule" if due else ("agent_idle" if idle else ("submission" if submitted else None))
            if trigger:
                t = file_wave(nxt["wave"], trigger, log, po)
                state["released"].append({"wave": nxt["wave"], "t": t, "trigger": trigger})
                write_atomic(state_p, json.dumps(state, indent=1))
        time.sleep(15)


def main():
    # Wait for the agent container to populate the shared volume.
    for _ in range(120):
        if (WORK / "QUESTIONS.md").exists():
            break
        time.sleep(5)
    log = EventLog(ART / "events.jsonl")
    log("sidecar.start", model=os.environ.get("GB_PO_MODEL"), snapshot_hours=os.environ.get("GB_SNAPSHOT_HOURS"))

    import sys
    sys.path.insert(0, "/po")
    import po_agent
    po_agent.HARNESS = Path("/po")  # HIDDEN_SPEC.md / PRODUCT_OWNER.md live here
    po = po_agent.ProductOwner(FileBackend(WORK), ART, log,
                               model=os.environ.get("GB_PO_MODEL", "deepseek-flash"),
                               poll_sec=float(os.environ.get("GB_PO_POLL_SEC", "20")))
    threading.Thread(target=snapshots, args=(log, float(os.environ.get("GB_SNAPSHOT_HOURS", "2"))), daemon=True).start()
    if (WAVES / "schedule.json").exists():
        threading.Thread(target=waves, args=(log, po, time.time()), daemon=True).start()
    po.run(threading.Event())  # never set: runs until the container is stopped


if __name__ == "__main__":
    main()
