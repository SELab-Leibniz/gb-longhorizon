#!/usr/bin/env python3
"""Generate the iCode agent + model profiles for a black-box run.

    make_profile.py --home DIR [--provider deepseek-openai|openai|mock]
                    [--model deepseek-flash] [--base-url URL]

Writes under DIR/.chrys/:
  agents/LongRun.yaml   the built-in Code profile, unchanged except that the
                        interactive-only tools (ask_user, doc_converter) are
                        dropped, TASK.md/QUESTIONS.md are added to the
                        auto-loaded memory files, and the long-running task
                        protocol is appended to the instructions
  models/gbmodel00001.yaml
  settings.yaml         web tools off, project hooks off, telemetry off

Everything else (compaction, sub-agents, todo, search) stays at iCode's
defaults — the agent is evaluated as shipped.
"""
import argparse
import importlib.resources
import sys
from pathlib import Path

import yaml

PROTOCOL = """
## Long-running task protocol (this environment)
- Your task is in `TASK.md`. Work on it continuously; there is no human watching.
- You will be re-invoked with "continue" prompts. Each time, re-read `TASK.md`,
  check `QUESTIONS.md` for new answers from the product owner, and carry on from
  the repository's current state (git log, test results). Never start over.
- Requirement questions: append `## Q: ...` to `QUESTIONS.md`, then continue
  working on something else while the answer arrives. Do not block.
- Commit after each coherent piece of work with a message that says why.
- When you believe everything is complete and verified, say so in one line and
  include the word DONE; otherwise keep going.
"""


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--home", required=True, type=Path)
    ap.add_argument("--provider", default="deepseek-openai")
    ap.add_argument("--model", default="deepseek-flash")
    ap.add_argument("--base-url", default="")
    ap.add_argument("--max-context", type=int, default=128000)
    ap.add_argument("--max-output", type=int, default=32000)  # reasoning models need room to think; 8192 starved deepseek-flash
    a = ap.parse_args()

    chrys = a.home / ".chrys"
    (chrys / "agents").mkdir(parents=True, exist_ok=True)
    (chrys / "models").mkdir(parents=True, exist_ok=True)

    code_yaml = importlib.resources.files("chrys.service.profiles.agents.builtins").joinpath("Code.yaml").read_text()
    prof = yaml.safe_load(code_yaml)
    prof["name"] = "LongRun"
    prof["id"] = "a0a0a0a0a001"
    prof["display_name"] = "Long-run Code Agent"
    prof["instructions"] = prof["instructions"].rstrip() + "\n" + PROTOCOL
    prof["tools"]["builtins"] = [t for t in prof["tools"]["builtins"] if t not in ("ask_user", "doc_converter")]
    prof["memory"] = {"files": ["AGENTS.md", "TASK.md", "QUESTIONS.md"]}
    prof["approval"] = {"default": "auto", "user_can_override": False}
    prof["skills"] = {"auto_load_user_agents_skills": False, "auto_load_cwd_agents_skills": False}
    (chrys / "agents" / "LongRun.yaml").write_text(yaml.safe_dump(prof, sort_keys=False, allow_unicode=True, width=100))

    model = {
        "id": "gbmodel00001", "name": "gb-model", "provider": a.provider, "api_style": "chat_completions",
        "model_id": a.model, "max_context_tokens": a.max_context, "max_output_tokens": a.max_output,
        "http_read_timeout": 600, "stream": False,
    }
    if a.provider == "deepseek-openai":
        model["api_key"] = "{{DEEPSEEK_API_KEY}}"
        if a.base_url:
            model["base_url"] = a.base_url
    elif a.provider == "openai":
        model["api_key"] = "{{OPENAI_API_KEY}}"
        model["base_url"] = a.base_url or "https://api.openai.com/v1"
    (chrys / "models" / "gbmodel00001.yaml").write_text(yaml.safe_dump(model, sort_keys=False))

    settings = {
        "model": {"profile": {"active": "gbmodel00001"}},
        "tools": {"web_search": {"mode": "off"}, "web_fetch": {"mode": "off"}},
        "project": {"hooks_enabled": False},
        "otel": {"enabled": False},
    }
    (chrys / "settings.yaml").write_text(yaml.safe_dump(settings, sort_keys=False))
    print(f"profiles written under {chrys}", file=sys.stderr)


if __name__ == "__main__":
    main()
