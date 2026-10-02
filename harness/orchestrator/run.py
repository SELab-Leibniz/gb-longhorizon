#!/usr/bin/env python3
"""Orchestrator: run one arm of the 48-hour emulator case study.

    run.py --arm full      --agent-cmd "python -m myframework --task TASK.md" [options]
    run.py --arm baseline  --agent-cmd "..."
    run.py --arm ablate-memory --agent-cmd "..."
    run.py --arm full --agent-cmd "python3 /harness/stub_agent.py" --backend local --hours 0.1

Responsibilities
  * create the workspace (docker container from the sandbox image, or a
    local directory for dry runs) and start the agent command in it
  * keep the agent alive for the duration: restart on exit, log each exit
  * run the product-owner agent against /work/QUESTIONS.md
  * every --snapshot-hours: copy the repo out, grade it, record the result
  * at --chaos-hour: kill the agent process hard, wait, restart it cold
  * write runs/<run_id>/events.jsonl (everything time-stamped) and
    runs/<run_id>/snapshots/<n>/{repo.bundle,results.json,trajectory/}

The agent is a black box.  Contract (see harness/README.md):
  env GB_RUN_ID, GB_ARM, GB_DISABLED_MODULES, GB_TRAJECTORY_DIR
  cwd /work, TASK.md present, keep running until killed.

No third-party Python dependencies.
"""
import argparse
import json
import os
import shlex
import shutil
import signal
import subprocess
import sys
import threading
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
HARNESS = HERE.parent
REPO = HARNESS.parent

MODULES = ["clarification", "localization", "memory", "verifier", "compression"]


def now():
    return time.time()


class EventLog:
    def __init__(self, path: Path):
        self.path = path
        self.lock = threading.Lock()
        self.t0 = now()

    def __call__(self, kind: str, **fields):
        rec = {"t": round(now(), 3), "elapsed_h": round((now() - self.t0) / 3600, 4), "kind": kind, **fields}
        with self.lock:
            with self.path.open("a") as f:
                f.write(json.dumps(rec) + "\n")
        print(f"[{rec['elapsed_h']:6.2f}h] {kind} {json.dumps(fields)[:200]}", flush=True)


# --------------------------------------------------------------------------
# Backends: where the agent runs
# --------------------------------------------------------------------------

class LocalBackend:
    """Runs the agent in a scratch copy of the repo on this machine.
    For dry runs and harness testing only — no isolation, has network."""

    def __init__(self, run_dir: Path, image_unused: str):
        self.work = run_dir / "work"
        self.proc = None

    def start(self):
        if self.work.exists():
            shutil.rmtree(self.work)
        shutil.copytree(REPO, self.work, ignore=shutil.ignore_patterns("target", ".git", "harness", "vendor"))
        brief = (HARNESS / "AGENT_BRIEF.md").read_text().split("\n---\n", 1)[1].lstrip()
        (self.work / "TASK.md").write_text(brief)
        shutil.copy(HARNESS / "QUESTIONS_SEED.md", self.work / "QUESTIONS.md")
        subprocess.run(["git", "init", "-q"], cwd=self.work, check=True)
        subprocess.run(["git", "-c", "user.email=s@x", "-c", "user.name=Scaffold", "add", "-A"], cwd=self.work, check=True)
        subprocess.run(["git", "-c", "user.email=s@x", "-c", "user.name=Scaffold", "commit", "-q", "-m", "Initial scaffold"], cwd=self.work, check=True)

    def spawn_agent(self, cmd: str, env: dict):
        self.proc = subprocess.Popen(
            cmd, shell=True, cwd=self.work, env={**os.environ, **env},
            stdout=open(self.work.parent / "agent.stdout.log", "a"),
            stderr=open(self.work.parent / "agent.stderr.log", "a"),
            start_new_session=True,
        )
        return self.proc

    def agent_alive(self):
        return self.proc is not None and self.proc.poll() is None

    def agent_exit_code(self):
        return None if self.proc is None else self.proc.poll()

    def kill_agent(self):
        if self.proc and self.proc.poll() is None:
            os.killpg(os.getpgid(self.proc.pid), signal.SIGKILL)
            self.proc.wait(timeout=30)

    def read_file(self, rel: str) -> str:
        p = self.work / rel
        return p.read_text() if p.exists() else ""

    def write_file(self, rel: str, content: str):
        (self.work / rel).write_text(content)

    def export_repo(self, dest: Path):
        """Copy the working tree (incl. .git) out for grading."""
        shutil.copytree(self.work, dest, ignore=shutil.ignore_patterns("target"))

    def stop(self):
        self.kill_agent()


class DockerBackend:
    """Runs the agent inside the sandbox image with no network."""

    def __init__(self, run_dir: Path, image: str):
        self.name = f"gb-{run_dir.name}"
        self.image = image
        self.run_dir = run_dir
        self.agent_pid = None

    def _docker(self, *args, check=True, capture=True, timeout=None):
        return subprocess.run(["docker", *args], check=check, capture_output=capture, text=True, timeout=timeout)

    def start(self):
        self._docker("rm", "-f", self.name, check=False)
        self._docker("run", "-d", "--name", self.name, "--network", "none",
                     "--cpus", os.environ.get("GB_CPUS", "4"), "--memory", os.environ.get("GB_MEM", "16g"),
                     "-w", "/work", self.image, "sleep", "infinity")
        self._docker("exec", self.name, "sh", "-c", "mkdir -p /trajectory")

    def spawn_agent(self, cmd: str, env: dict):
        env_args = []
        for k, v in env.items():
            env_args += ["-e", f"{k}={v}"]
        # Run detached inside the container; record its PID.
        wrapped = f"cd /work && nohup sh -c {shlex.quote(cmd)} >> /agent.stdout.log 2>> /agent.stderr.log & echo $!"
        r = self._docker("exec", *env_args, self.name, "sh", "-c", wrapped)
        self.agent_pid = int(r.stdout.strip().splitlines()[-1])
        return self.agent_pid

    def agent_alive(self):
        if self.agent_pid is None:
            return False
        r = self._docker("exec", self.name, "sh", "-c", f"kill -0 {self.agent_pid} 2>/dev/null && echo yes || echo no")
        return r.stdout.strip() == "yes"

    def agent_exit_code(self):
        return None  # not recoverable for a detached process; exit is logged as 'unknown'

    def kill_agent(self):
        if self.agent_pid is not None:
            self._docker("exec", self.name, "sh", "-c", f"pkill -KILL -P {self.agent_pid}; kill -KILL {self.agent_pid}", check=False)
            self.agent_pid = None

    def read_file(self, rel: str) -> str:
        r = self._docker("exec", self.name, "sh", "-c", f"cat /work/{shlex.quote(rel)} 2>/dev/null || true")
        return r.stdout

    def write_file(self, rel: str, content: str):
        subprocess.run(
            ["docker", "exec", "-i", self.name, "sh", "-c", f"cat > /work/{shlex.quote(rel)}"],
            input=content, text=True, check=True)

    def export_repo(self, dest: Path):
        dest.parent.mkdir(parents=True, exist_ok=True)
        self._docker("exec", self.name, "sh", "-c", "cd /work && tar --exclude=target -cf /tmp/repo.tar .")
        self._docker("cp", f"{self.name}:/tmp/repo.tar", str(dest.parent / "repo.tar"), capture=False)
        dest.mkdir()
        subprocess.run(["tar", "-xf", str(dest.parent / "repo.tar"), "-C", str(dest)], check=True)
        (dest.parent / "repo.tar").unlink()
        traj = dest.parent / "trajectory"
        self._docker("cp", f"{self.name}:/trajectory", str(traj), check=False, capture=False)
        for log in ("agent.stdout.log", "agent.stderr.log"):
            self._docker("cp", f"{self.name}:/{log}", str(dest.parent / log), check=False, capture=False)

    def stop(self):
        self._docker("rm", "-f", self.name, check=False)


# --------------------------------------------------------------------------
# Snapshot + grade
# --------------------------------------------------------------------------

def snapshot(backend, run_dir: Path, n: int, log: EventLog, golden: Path, grade_tiers: str):
    snap = run_dir / "snapshots" / f"{n:03d}"
    snap.mkdir(parents=True, exist_ok=True)
    repo = snap / "repo"
    t = now()
    backend.export_repo(repo)
    # Commit count and last commit time are cheap stability signals.
    gl = subprocess.run(["git", "log", "--format=%at", "--reverse"], cwd=repo, capture_output=True, text=True)
    commits = [int(x) for x in gl.stdout.split()] if gl.returncode == 0 else []
    subprocess.run(["git", "bundle", "create", str(snap / "repo.bundle"), "--all"], cwd=repo, capture_output=True)
    log("snapshot.exported", n=n, commits=len(commits), export_secs=round(now() - t, 1))

    cmd = [sys.executable, str(HARNESS / "scripts" / "grade.py"), str(repo), "--golden", str(golden), "-o", str(snap / "results.json")]
    if grade_tiers:
        cmd += ["--tier", grade_tiers]
    t = now()
    r = subprocess.run(cmd, capture_output=True, text=True, timeout=3 * 3600)
    summary = r.stdout.strip().splitlines()[-1] if r.stdout.strip() else r.stderr[-300:]
    log("snapshot.graded", n=n, grade_secs=round(now() - t, 1), ok=r.returncode == 0, summary=summary)
    shutil.rmtree(repo, ignore_errors=True)  # bundle is enough; save disk
    return summary


# --------------------------------------------------------------------------
# Main loop
# --------------------------------------------------------------------------

def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--arm", required=True, help="full | baseline | ablate-<module>")
    ap.add_argument("--agent-cmd", required=True)
    ap.add_argument("--run-id", default=None)
    ap.add_argument("--backend", choices=["docker", "local"], default="docker")
    ap.add_argument("--image", default="gb-longhorizon-sandbox")
    ap.add_argument("--hours", type=float, default=48.0)
    ap.add_argument("--snapshot-hours", type=float, default=2.0)
    ap.add_argument("--chaos-hour", type=float, default=20.0, help="kill the agent hard at this hour; <0 disables")
    ap.add_argument("--chaos-downtime-min", type=float, default=5.0)
    ap.add_argument("--restart-delay-sec", type=float, default=30.0)
    ap.add_argument("--max-restarts", type=int, default=50)
    ap.add_argument("--golden", type=Path, default=HARNESS / "golden")
    ap.add_argument("--po-model", default=os.environ.get("GB_PO_MODEL", "deepseek-v4.1-flash"))
    ap.add_argument("--po-poll-sec", type=float, default=20.0)
    ap.add_argument("--no-po", action="store_true")
    ap.add_argument("--grade-tiers", default="")
    a = ap.parse_args()

    if a.arm == "full":
        disabled = []
    elif a.arm == "baseline":
        disabled = MODULES[:]
    elif a.arm.startswith("ablate-") and a.arm[7:] in MODULES:
        disabled = [a.arm[7:]]
    else:
        sys.exit(f"bad --arm {a.arm}; want full | baseline | ablate-{{{','.join(MODULES)}}}")

    run_id = a.run_id or f"{a.arm}-{time.strftime('%Y%m%d-%H%M%S')}"
    run_dir = HARNESS / "runs" / run_id
    run_dir.mkdir(parents=True, exist_ok=True)
    log = EventLog(run_dir / "events.jsonl")
    (run_dir / "config.json").write_text(json.dumps(vars(a), indent=2, default=str))
    log("run.start", arm=a.arm, disabled=disabled, backend=a.backend, hours=a.hours)

    backend = (LocalBackend if a.backend == "local" else DockerBackend)(run_dir, a.image)
    backend.start()
    log("workspace.ready")

    env = {
        "GB_RUN_ID": run_id,
        "GB_ARM": a.arm,
        "GB_DISABLED_MODULES": ",".join(disabled),
        "GB_TRAJECTORY_DIR": "/trajectory" if a.backend == "docker" else str(run_dir / "trajectory"),
    }
    if a.backend == "local":
        Path(env["GB_TRAJECTORY_DIR"]).mkdir(exist_ok=True)

    # Product owner runs on the host and talks to the workspace through the backend.
    po_stop = threading.Event()
    if not a.no_po:
        from po_agent import ProductOwner
        po = ProductOwner(backend, run_dir, log, model=a.po_model, poll_sec=a.po_poll_sec)
        threading.Thread(target=po.run, args=(po_stop,), daemon=True).start()

    deadline = log.t0 + a.hours * 3600
    next_snapshot = log.t0 + a.snapshot_hours * 3600
    chaos_at = log.t0 + a.chaos_hour * 3600 if a.chaos_hour >= 0 else None
    restarts = 0
    snap_n = 0

    def start_agent(reason):
        backend.spawn_agent(a.agent_cmd, env)
        log("agent.start", reason=reason, restarts=restarts)

    start_agent("initial")
    try:
        while now() < deadline:
            time.sleep(10)

            if not backend.agent_alive():
                log("agent.exit", code=backend.agent_exit_code(), restarts=restarts)
                if restarts >= a.max_restarts:
                    log("agent.gave_up", restarts=restarts)
                    break
                restarts += 1
                time.sleep(a.restart_delay_sec)
                start_agent("crash")

            if chaos_at and now() >= chaos_at:
                log("chaos.kill")
                backend.kill_agent()
                time.sleep(a.chaos_downtime_min * 60)
                restarts += 1
                start_agent("chaos")
                chaos_at = None

            if now() >= next_snapshot:
                snap_n += 1
                try:
                    snapshot(backend, run_dir, snap_n, log, a.golden, a.grade_tiers)
                except Exception as e:  # never let grading kill the run
                    log("snapshot.error", n=snap_n, error=repr(e))
                next_snapshot += a.snapshot_hours * 3600
    finally:
        log("run.deadline" if now() >= deadline else "run.aborted")
        po_stop.set()
        backend.kill_agent()
        snap_n += 1
        try:
            snapshot(backend, run_dir, snap_n, log, a.golden, a.grade_tiers)
        except Exception as e:
            log("snapshot.error", n=snap_n, error=repr(e))
        backend.stop()
        log("run.end", restarts=restarts, snapshots=snap_n)


if __name__ == "__main__":
    main()
