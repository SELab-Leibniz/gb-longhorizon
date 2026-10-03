#!/usr/bin/env python3
"""Build the stubbed workspace in the toolchain container (as the sandbox does:
gb-gui removed, offline) and add #[allow(...)] to items that are now unused.
Repeats until the workspace builds without warnings. Prints remaining errors.

    fixwarn.py REPO_DIR
"""
import json
import subprocess
import sys
from pathlib import Path

repo = Path(sys.argv[1]).resolve()
LINTS = {"dead_code": "dead_code", "unused_imports": "unused_imports", "unused_variables": "unused_variables",
         "unused_mut": "unused_mut", "unused_assignments": "unused_assignments"}
SCRIPT = r'''
set -e
rm -rf /w && cp -a /src /w && cd /w && sed -i 's/"gb-gui", //' Cargo.toml && rm -rf gb-gui
RUSTFLAGS="" cargo build --workspace --all-targets --offline --message-format=json -q 2>/dev/null || true
'''


def build():
    p = subprocess.run(["docker", "run", "--rm", "-v", f"{repo}:/src:ro", "-v", "gb-v2-cargo:/usr/local/cargo/registry",
                        "rust:1.97.0-bookworm", "bash", "-c", SCRIPT], capture_output=True, text=True, errors="replace")
    msgs = []
    for line in p.stdout.splitlines():
        try:
            m = json.loads(line)
        except Exception:
            continue
        if m.get("reason") == "compiler-message":
            msgs.append(m["message"])
    return msgs


for rnd in range(6):
    msgs = build()
    errors = [m for m in msgs if m["level"] == "error"]
    warns = [m for m in msgs if m["level"] == "warning" and (m.get("code") or {}).get("code") in LINTS]
    other = [m for m in msgs if m["level"] == "warning" and m not in warns]
    print(f"round {rnd}: {len(errors)} errors, {len(warns)} fixable warnings, {len(other)} other warnings")
    if errors:
        for m in errors[:8]:
            print("ERROR:", m["rendered"][:600])
        sys.exit(1)
    if not warns:
        for m in other[:10]:
            print("OTHER:", m["rendered"][:400])
        break
    inserts = {}
    for m in warns:
        span = next((s for s in m["spans"] if s.get("is_primary")), m["spans"][0])
        f = span["file_name"].replace("/w/", "")
        inserts.setdefault(f, set()).add((span["line_start"], LINTS[m["code"]["code"]]))
    for f, items in inserts.items():
        path = repo / f
        lines = path.read_text().splitlines(keepends=True)
        for line_no, lint in sorted(items, reverse=True):
            target = lines[line_no - 1]
            indent = target[: len(target) - len(target.lstrip())]
            # put the attribute above the item (and above any attributes / doc comments directly on it)
            i = line_no - 1
            while i > 0 and lines[i - 1].strip().startswith(("#[", "///")) and "allow(" not in lines[i - 1]:
                i -= 1
            if lint in ("unused_variables", "unused_mut", "unused_assignments"):
                i = line_no - 1   # statement-level: right above the statement
            lines.insert(i, f"{indent}#[allow({lint})]\n")
        path.write_text("".join(lines))
        print(f"  {f}: {len(items)} allow attributes added")
