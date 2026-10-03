#!/usr/bin/env python3
"""Aggregate Harbor trials of the gb benchmark into one report.

    summarize.py JOBS_DIR [JOBS_DIR ...] [--json out.json] [--price-in P --price-cached P --price-out P]

For every trial (a directory with result.json) it reads the verifier's
reward.json, the trial's artifacts and the agent's trajectory, and prints a
Markdown report: one row per trial, then mean / spread / min / max per agent.
Prices are USD per million tokens (optional; omit to report tokens only).

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
from datetime import datetime
from pathlib import Path

ALLOWED_HOSTS = {"api.deepseek.com"}
PHASES = ["core", "cgb", "ppu", "tooling", "portability", "library", "front_end"]


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
    for q in qs:
        ids = {i.upper() for i in q.get("items", []) if re.fullmatch(r"(?i)OI-\d", i)}
        ids |= set(re.findall(r"\bOI-\d\b", q.get("question", "").upper()))
        asked |= ids
        if not q.get("agent_had_committed_product_code"):
            early |= ids
    return {"questions": len(qs), "open_issues_asked": sorted(asked), "asked_before_first_product_commit": sorted(early)}


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
    for r in rows:
        if r["egress"].get("blocked_attempts"):
            print(f"\nnote: {r['trial']} tried to reach blocked hosts: {', '.join(r['egress']['blocked_attempts'][:8])}")
    if a.json:
        a.json.write_text(json.dumps(rows, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
