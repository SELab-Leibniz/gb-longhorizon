"""Product-owner + snapshot sidecar for the Harbor task.

Runs for the life of the trial:
  * every GB_PO_POLL_SEC: answer pending "## Q:" headings in /work/QUESTIONS.md
    (po_agent.ProductOwner, with a file-based backend on the shared volume)
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
    po.run(threading.Event())  # never set: runs until the container is stopped


if __name__ == "__main__":
    main()
