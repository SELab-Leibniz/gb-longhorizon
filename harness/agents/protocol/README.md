# Run protocol shared by every agent adapter

The study treats each coding agent as a black box. Every agent gets exactly
the same text, delivered the same way:

* **First invocation:** the contents of `TASK.md` (= `harness/AGENT_BRIEF.md`
  after `---`), verbatim, as the user prompt. Nothing is added to the agent's
  system prompt or memory configuration.
* **Later invocations** (the previous one ended before the deadline): the
  same session, prompted with `continue.txt` — or `recheck.txt` if the
  previous invocation reported the work complete (said `DONE` or rewrote
  `SUBMISSION.md`).
* **Pause** before a recheck: 180 s; before a continue: 5 s.
* **New issues** filed during the run (the showcase's backlog waves) are
  delivered by `notices.py`, the same code in every adapter: the next prompt
  is `new_issues.txt` listing them (instead of continue/recheck). A running
  invocation is interrupted for it at the first commit after the filing, or
  `GB_NOTICE_MAX_WAIT_SEC` after it at the latest, and the same session is
  resumed; that interruption is not counted as a failure. Every delivery is
  recorded in `/notices/wave-N.delivered.json`. When an invocation ends with
  the work reported complete, the adapter tells the sidecar (`notices.py
  idle`), which may file the next wave early.

Per-agent settings are limited to what a headless, offline, unattended run
needs, and each has an equivalent in the other adapter:

| Need | iCode (`make_profile.py`) | jiuwenswarm (`launch.py`) |
|---|---|---|
| model | `deepseek-flash` via its DeepSeek provider; max output 32k tokens (8k starved the reasoning model) | same model via its OpenAI-compatible endpoint profile |
| no human to approve tools | approval `auto` | every interaction card auto-answered (approve / proceed) |
| no human to ask | `ask_user` tool removed | `ask_user` cards answered "no human is available" |
| no network | web search / fetch off; MCP none | `web_*` tools removed; MCP none |
| isolation | user/cwd skill auto-loading off; project hooks off; telemetry off | telemetry off |
| logs | trajectory files kept | debug trace + OpenTelemetry spans to files |

Everything else — context management, compaction, planning, sub-agents,
memory — is each agent's shipped default.
