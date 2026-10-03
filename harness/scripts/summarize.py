#!/usr/bin/env python3
"""Aggregate Harbor trials of the gb benchmark into one report.

    summarize.py JOBS_DIR [JOBS_DIR ...] [--json out.json] [--price-in P --price-cached P --price-out P]

For every trial (a directory with result.json) it reads the verifier's
reward.json, the trial's artifacts and the agent's trajectory, and prints a
Markdown report: one row per trial, then mean / spread / min / max per agent.
Prices are USD per million tokens (optional; omit to report tokens only).

A second table holds the showcase diagnostics (reported, never scored): the
backlog's hidden checks, which planted bugs were fixed at the planted line and
when, which product decisions the agent asked about and whether before its
first change to the library, notes files, tests added, gb-oracle use,
context compactions and sub-agents.

Validity (a trial is reported but flagged, never silently dropped):
  infra     Harbor recorded no exception and the verifier wrote reward.json
  egress    no connection was established to any host but the model API
A trial that fails either is an infrastructure fault: it is excluded from the
per-agent statistics and re-run (BENCHMARK.md). Editing a frozen file is the
agent's own doing: the trial counts, with reward 0, and is flagged "frozen".
"""
from __future__ import annotations

import argparse
import json
import re
import statistics
import subprocess
import sys
import tempfile
from datetime import datetime
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "showcase"))
try:
    from bugs import BUGS          # planted bugs: id -> (file, original, planted)
except ImportError:
    BUGS = {}

ALLOWED_HOSTS = {"api.deepseek.com"}
PHASES = ["core", "cgb", "ppu", "tooling", "portability", "library", "front_end", "tickets", "decisions"]
# backlog items that need (or rule on) a product decision -> their hidden check (None: covered by regular checks)
DECISION_ISSUES = {"108": "P1", "109": "P4", "110": "P7", "111": None, "114": "P3", "115": "P5",
                   "118": "P2", "119": "P6", "122": "P8", "124": "P9"}


def load(p: Path, default=None):
    try:
        return json.loads(p.read_text())
    except Exception:
        return default


def hours(a, b):
    try:
        f = lambda s: datetime.fromisoformat(s.replace("Z", "+00:00"))
        return round((f(b) - f(a)).total_seconds() / 3600, 2)
    except Exception:
        return None


def egress(trial: Path):
    log = trial / "artifacts/var/log/tinyproxy/tinyproxy.log"
    if not log.exists():
        return {"checked": False}
    text = log.read_text(errors="replace")
    requested = set(re.findall(r"Request \(file descriptor \d+\): CONNECT ([^:\s]+):\d+", text))
    requested |= set(re.findall(r"Request \(file descriptor \d+\): [A-Z]+ https?://([^/:\s]+)", text))
    established = set(re.findall(r'Established connection to host "([^"]+)"', text))
    return {"checked": True, "blocked_attempts": sorted(requested - ALLOWED_HOSTS),
            "violations": sorted(established - ALLOWED_HOSTS)}


def usage_icode(traj: Path):
    tot = {"input": 0, "cached": 0, "output": 0, "calls": 0}
    for f in traj.glob("chrys/sessions/*/trajectory/events.jsonl"):
        for line in f.open(errors="replace"):
            if '"usage"' not in line:
                continue
            try:
                stack = [json.loads(line)]
            except Exception:
                continue
            while stack:
                o = stack.pop()
                if isinstance(o, dict):
                    u = o.get("usage")
                    if isinstance(u, dict) and isinstance(u.get("normalized"), dict):
                        n = u["normalized"]
                        tot["input"] += n.get("input_total", 0) or 0
                        tot["cached"] += n.get("cache_read", 0) or 0
                        tot["output"] += n.get("output_total", 0) or 0
                        tot["calls"] += 1
                    stack.extend(o.values())
                elif isinstance(o, list):
                    stack.extend(o)
    return tot if tot["calls"] else None


def usage_jiuwenswarm(traj: Path):
    tot = {"input": 0, "cached": 0, "output": 0, "calls": 0}
    for f in traj.glob("jw_otel/*.jsonl"):
        s = f.read_text(errors="replace")
        for key, field in (("input_tokens", "input"), ("cache_read.input_tokens", "cached"), ("output_tokens", "output")):
            vals = re.findall(r'"gen_ai\.usage\.%s", "value": \{"intValue": "(\d+)"\}' % re.escape(key), s)
            tot[field] += sum(int(v) for v in vals)
            if field == "input":
                tot["calls"] += len(vals)
    return tot if tot["calls"] else None


def git(repo: Path, *args):
    return subprocess.run(["git", "-C", str(repo), *args], capture_output=True, text=True, errors="replace").stdout


def bug_marker(old: str, new: str) -> str:
    """The longest line of the planted text that the original does not have."""
    lines = [l.strip() for l in new.splitlines() if l.strip() and l.strip() not in old]
    return max(lines, key=len) if lines else new.strip()


def repo_history(trial: Path):
    """Localization, memory and verification evidence from the trial's repo.bundle."""
    bundle = trial / "verifier/repo.bundle"
    if not bundle.exists():
        return None
    with tempfile.TemporaryDirectory() as td:
        repo = Path(td) / "repo"
        subprocess.run(["git", "clone", "-q", str(bundle), str(repo)], capture_output=True)
        if not (repo / ".git").exists():
            return None
        log = [l.split("\t") for l in git(repo, "log", "--reverse", "--format=%H\t%at\t%an").splitlines()]
        start = int(log[0][1]) if log else 0
        scaffold = {h for h, _, an in log if an == "Scaffold"}
        bugs = {}
        for bid, (rel, old, new) in BUGS.items():
            marker = bug_marker(old, new)
            head = (repo / rel).read_text(errors="replace") if (repo / rel).exists() else ""
            removed = [l.split("\t") for l in git(repo, "log", "--format=%H\t%at", "-S", marker, "--", rel).splitlines()]
            removed = [(h, int(t)) for h, t in removed if h not in scaffold]
            bugs["T" + bid[1:]] = {"planted_line_present": marker in head,
                                  "changed_at_h": round((removed[0][1] - start) / 3600, 2) if removed and marker not in head else None}
        added = git(repo, "log", "--diff-filter=A", "--name-only", "--format=", "--author=Coding Agent").split()
        notes = sorted({f for f in added if f.lower().endswith((".md", ".txt"))
                        and not f.startswith("ISSUES/") and f not in ("SUBMISSION.md", "QUESTIONS.md")})
        tests = sorted({f for f in added if re.search(r"(^|/)tests?/|_test\.|test_|\.(py|sh|mjs|js)$", f)
                        and not f.startswith("gb-web/static/")})
        web = [int(t) for t in git(repo, "log", "--format=%at", "--author=Coding Agent", "--", "gb-web").split()]
        return {"start": start, "bugs": bugs, "notes_files": notes,
                "notes_commits": {f: len(git(repo, "log", "--format=%H", "--", f).split()) for f in notes},
                "tests_added": tests, "first_library_commit": min(web) if web else None}


def agent_activity(traj: Path, agent: str):
    """Context compactions, sub-agents and gb-oracle calls from the agent's own logs."""
    out = {"compactions": 0, "sub_agents": 0, "oracle_calls": 0}
    files = (list(traj.glob("chrys/sessions/*/trajectory/events.jsonl")) if agent == "icode"
             else list(traj.glob("jw_events_*.jsonl")))
    for f in files:
        for line in f.open(errors="replace"):
            if agent == "icode":
                if '"compaction.finished"' in line:
                    out["compactions"] += 1
                elif '"sub_agent.started"' in line:
                    out["sub_agents"] += 1
            elif '"context.compression_state"' in line and '"status": "completed"' in line:
                out["compactions"] += 1
            elif '"chat.tool_call"' in line and "gb-oracle" in line:
                out["oracle_calls"] += 1
    if agent == "icode":
        # iCode's event log carries argument fingerprints only; the calls themselves are in the
        # recovered conversation of the main session and of each sub-agent
        def calls(o, acc):
            if isinstance(o, dict):
                if "name" in o and ("arguments" in o or "input" in o):
                    acc.append(json.dumps(o.get("arguments", o.get("input"))))
                for v in o.values():
                    calls(v, acc)
            elif isinstance(o, list):
                for v in o:
                    calls(v, acc)
        for f in list(traj.glob("chrys/sessions/*/session.recovery.json")) + list(traj.glob("chrys/sessions/*/sub_agents/sessions/*.json")):
            acc = []
            calls(load(f, {}), acc)
            out["oracle_calls"] += sum("gb-oracle" in a for a in acc)
    return out


def product_owner(trial: Path):
    art = trial / "artifacts/po-artifacts"
    qs = []
    for line in (art / "po_log.jsonl").open() if (art / "po_log.jsonl").exists() else []:
        try:
            qs.append(json.loads(line))
        except Exception:
            pass
    asked = set()
    early = set()
    first_ask = {}
    for q in qs:
        ids = {i.upper() for i in q.get("items", []) if re.fullmatch(r"(?i)OI-\d", i)}
        ids |= set(re.findall(r"\bOI-\d\b", q.get("question", "").upper()))
        asked |= ids
        if not q.get("agent_had_committed_product_code"):
            early |= ids
        # backlog items, by the answer's [items: #NNN] tag or a number in the question
        nums = {i.lstrip("#") for i in q.get("items", []) if re.fullmatch(r"#?\d{3}", i.strip())}
        nums |= set(re.findall(r"#?\b(1[0-2]\d)\b", q.get("question", "")))
        for n in nums & set(DECISION_ISSUES):
            first_ask.setdefault(n, q.get("t"))
    return {"questions": len(qs), "open_issues_asked": sorted(asked), "asked_before_first_product_commit": sorted(early),
            "decision_issues_asked": dict(sorted(first_ask.items()))}


def trial_row(trial: Path):
    res = load(trial / "result.json", {})
    agent = (res.get("agent_info") or {}).get("name") or "?"
    rw = load(trial / "verifier/reward.json")
    traj = trial / "agent/trajectory"
    use = usage_icode(traj) if agent == "icode" else usage_jiuwenswarm(traj) if agent == "jiuwenswarm" else None
    gitlog = trial / "verifier/git-log.txt"
    commits = len(gitlog.read_text().splitlines()) if gitlog.exists() else None
    eg = egress(trial)
    ae = res.get("agent_execution") or {}
    return {
        "trial": trial.name, "job": trial.parent.name, "agent": agent,
        "model": ((res.get("agent_info") or {}).get("model_info") or {}).get("name"),
        "reward": rw.get("reward") if rw else None,
        "phases": {p: (rw or {}).get(f"phase_{p}") for p in PHASES},
        "valid": {
            "infra": res.get("exception_info") is None and rw is not None,
            "egress": eg.get("checked", False) and not eg.get("violations"),
        },
        "frozen_edited": bool(rw) and rw.get("frozen_unchanged") != 1.0,
        "egress": eg,
        "agent_hours": hours(ae.get("started_at"), ae.get("finished_at")),
        "commits": commits - 2 if commits else commits,      # minus the scaffold + task-brief commits
        "submission": (trial / "verifier/SUBMISSION.md").exists(),
        "tokens": use,
        "product_owner": product_owner(trial),
        "showcase": showcase(trial, agent, traj),
    }


def showcase(trial: Path, agent: str, traj: Path):
    tk = load(trial / "verifier/tickets.json", {}) or {}
    iss = load(trial / "verifier/issues.json", {}) or {}
    waves = load(trial / "verifier/waves.json", {}) or {}
    hist = repo_history(trial)
    po = product_owner(trial)
    checks = {k: v["ok"] for block in tk.values() for k, v in block.get("checks", {}).items()}
    counted = {0} | set(waves.get("delivered", []))
    wave_of = {k: v.get("wave", 0) for block in tk.values() for k, v in block.get("checks", {}).items()}
    checks = {k: ok for k, ok in checks.items() if wave_of.get(k, 0) in counted}   # only issues the agent was told about
    asked = po["decision_issues_asked"]
    first_web = (hist or {}).get("first_library_commit")
    before = sorted(n for n, t in asked.items()
                    if t is not None and (first_web is None or (t if t > 1e9 else 0) <= first_web))
    fixed_at_site = sorted(t for t, b in ((hist or {}).get("bugs") or {}).items() if not b["planted_line_present"])
    fix_hours = sorted(b["changed_at_h"] for b in ((hist or {}).get("bugs") or {}).values() if b["changed_at_h"] is not None)
    return {
        "tickets": checks, "issues_resolved": iss.get("resolved"), "issues_total": iss.get("total"),
        "bugs_total": sum(1 for k in checks if k.startswith("T")),
        "decisions_total": sum(1 for k in checks if k.startswith("P")),
        "waves": waves,
        "bugs_fixed": sorted(k for k, ok in checks.items() if k.startswith("T") and ok),
        "bugs_changed_at_planted_line": fixed_at_site,
        "bug_change_hours": fix_hours,
        "decisions_passed": sorted(k for k, ok in checks.items() if k.startswith("P") and ok),
        "decision_issues_asked": sorted(asked), "asked_before_first_library_commit": before,
        "notes_files": (hist or {}).get("notes_files"), "notes_commits": (hist or {}).get("notes_commits"),
        "tests_added": len((hist or {}).get("tests_added") or []),
        **agent_activity(traj, agent),
    }


def fmt(x, nd=3):
    return "–" if x is None else (f"{x:.{nd}f}" if isinstance(x, float) else str(x))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("jobs", nargs="+", type=Path)
    ap.add_argument("--json", type=Path)
    ap.add_argument("--price-in", type=float, help="USD per 1M uncached input tokens")
    ap.add_argument("--price-cached", type=float, help="USD per 1M cached input tokens")
    ap.add_argument("--price-out", type=float, help="USD per 1M output tokens")
    a = ap.parse_args()
    rows = [trial_row(r.parent) for j in a.jobs for r in sorted(j.rglob("result.json")) if r.parent != j]
    if a.price_in is not None:
        for r in rows:
            t = r["tokens"]
            if t:
                t["usd"] = round(((t["input"] - t["cached"]) * a.price_in + t["cached"] * (a.price_cached or 0)
                                  + t["output"] * (a.price_out or 0)) / 1e6, 2)
    print("# gb benchmark results\n")
    print("| trial | agent | reward | " + " | ".join(PHASES) + " | valid | flags | agent h | commits | PO Qs | OIs asked | tokens in / out (M) |")
    print("|" + "---|" * (len(PHASES) + 10))
    for r in rows:
        v = "".join("✓" if ok else "✗" for ok in r["valid"].values())
        t = r["tokens"]
        tok = f"{t['input'] / 1e6:.1f} / {t['output'] / 1e6:.2f}" + (f" (${t['usd']})" if "usd" in t else "") if t else "–"
        flags = "frozen-edited" if r["frozen_edited"] else ""
        print(f"| {r['job']}/{r['trial']} | {r['agent']} | {fmt(r['reward'])} | "
              + " | ".join(fmt(r["phases"][p], 2) for p in PHASES)
              + f" | {v} | {flags} | {fmt(r['agent_hours'])} | {fmt(r['commits'])} | {r['product_owner']['questions']} | "
              + f"{','.join(r['product_owner']['open_issues_asked']) or '–'} | {tok} |")
    print("\nvalid = infra / egress. Invalid trials are listed but excluded below and re-run (BENCHMARK.md).\n")
    print("## Per agent (valid trials only)\n")
    print("| agent | n | reward mean | sd | min | max | " + " | ".join(PHASES) + " |")
    print("|" + "---|" * (len(PHASES) + 6))
    for agent in sorted({r["agent"] for r in rows}):
        rs = [r for r in rows if r["agent"] == agent and all(r["valid"].values()) and r["reward"] is not None]
        if not rs:
            print(f"| {agent} | 0 |" + " – |" * (len(PHASES) + 4))
            continue
        rew = [r["reward"] for r in rs]
        sd = statistics.stdev(rew) if len(rew) > 1 else 0.0
        means = [statistics.mean(r["phases"][p] or 0.0 for r in rs) for p in PHASES]
        print(f"| {agent} | {len(rs)} | {statistics.mean(rew):.3f} | {sd:.3f} | {min(rew):.3f} | {max(rew):.3f} | "
              + " | ".join(f"{m:.2f}" for m in means) + " |")
    print("\n## Showcase diagnostics (reported, not scored)\n")
    print("| trial | waves delivered | bugs fixed | at planted line | hours to change (median) | decisions | "
          "decision issues asked | asked before 1st library commit | issues resolved | notes files | "
          "tests added | gb-oracle calls | compactions | sub-agents |")
    print("|" + "---|" * 14)
    for r in rows:
        d = r["showcase"]
        hrs = d["bug_change_hours"]
        if not d["tickets"]:      # not a showcase trial (no planted bugs, no hidden backlog checks)
            print(f"| {r['job']}/{r['trial']} | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | {len(d['notes_files'] or [])} | "
                  f"{d['tests_added']} | {d['oracle_calls']} | {d['compactions']} | {d['sub_agents']} |")
            continue
        w = d["waves"] or {}
        dl = w.get("deliveries") or {}
        wv = (f"{len(w.get('delivered', []))}/{len(w.get('released', []))}"
              + (" (" + ", ".join(f"w{k}: {v.get('method')} +{v.get('delay_sec')}s" for k, v in sorted(dl.items())) + ")" if dl else ""))
        print(f"| {r['job']}/{r['trial']} | {wv} | {len(d['bugs_fixed'])}/{d['bugs_total']} | {len(d['bugs_changed_at_planted_line'])} | "
              f"{fmt(statistics.median(hrs), 1) if hrs else '–'} | {len(d['decisions_passed'])}/{d['decisions_total']} | "
              f"{len(d['decision_issues_asked'])} ({','.join('#' + n for n in d['decision_issues_asked']) or '–'}) | "
              f"{len(d['asked_before_first_library_commit'])} | {fmt(d['issues_resolved'])}/{fmt(d['issues_total'])} | "
              f"{len(d['notes_files'] or [])} | {d['tests_added']} | {d['oracle_calls']} | {d['compactions']} | {d['sub_agents']} |")
    print("\nOnly issues the agent was told about count: wave 0 and every delivered wave (how: between invocations, "
          "after a commit, or on timeout; +seconds after release). Bugs: hidden T checks; 'at planted line': the planted "
          "line is gone from the graded commit; hours: when it changed. Decisions: hidden P checks (HIDDEN_SPEC.md).")
    for r in rows:
        if r["egress"].get("blocked_attempts"):
            print(f"\nnote: {r['trial']} tried to reach blocked hosts: {', '.join(r['egress']['blocked_attempts'][:8])}")
    if a.json:
        a.json.write_text(json.dumps(rows, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
