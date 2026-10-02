#!/usr/bin/env bash
# iCode adapter for the orchestrator.
#
#   --agent-cmd "bash /path/to/harness/agents/icode/launch.sh"
#
# Runs in the workspace (cwd = repo with TASK.md). Reads the orchestrator
# contract env (GB_ARM, GB_TRAJECTORY_DIR) and keeps
# iCode working until killed: the first invocation runs TASK.md, every later
# one resumes the same session with a "continue" prompt. If resuming fails
# repeatedly (e.g. context overflow with compaction disabled) it starts a
# fresh session on the current repo state — that event is logged, since it
# is exactly the kind of instability the study measures.
#
# Required env:
#   ICODE_DIR          checkout of https://github.com/openJiuwen-ai/iCode with .venv (uv sync)
#   DEEPSEEK_API_KEY   (unless ICODE_PROVIDER=mock)
# Optional:
#   ICODE_PROVIDER     deepseek-openai (default) | openai | mock
#   ICODE_MODEL        deepseek-v4.1-flash (default)
#   ICODE_BASE_URL     override endpoint
#   ICODE_CONTINUE_SLEEP_SEC   pause between invocations after a DONE (default 600)
set -uo pipefail

: "${ICODE_DIR:?set ICODE_DIR to the iCode checkout}"
: "${GB_TRAJECTORY_DIR:=./.trajectory}"
ICODE_PROVIDER="${ICODE_PROVIDER:-deepseek-openai}"
ICODE_MODEL="${ICODE_MODEL:-deepseek-v4.1-flash}"
ICODE="$ICODE_DIR/.venv/bin/icode"
PY="$ICODE_DIR/.venv/bin/python"
WORK="$PWD"
HERE="$(cd "$(dirname "$0")" && pwd)"

mkdir -p "$GB_TRAJECTORY_DIR"
# Isolate all of iCode's state under the trajectory dir so it is collected
# with every snapshot and never leaks between runs.
export HOME="$GB_TRAJECTORY_DIR/home"
export CHRYS_SESSION_ROOT_DIR="$GB_TRAJECTORY_DIR/chrys"
export CHRYS_MAX_TRANSIENT_RETRIES=50
export CHRYS_MODEL_PROFILE=gbmodel00001
mkdir -p "$HOME" "$CHRYS_SESSION_ROOT_DIR"
LOG="$GB_TRAJECTORY_DIR/icode_adapter.jsonl"
STATE="$GB_TRAJECTORY_DIR/icode_state"   # holds the session id across restarts

log() { printf '{"t":%s,"event":"%s"%s}\n' "$(date +%s)" "$1" "${2:-}" >> "$LOG"; }

"$PY" "$HERE/make_profile.py" --home "$HOME" \
      --provider "$ICODE_PROVIDER" --model "$ICODE_MODEL" ${ICODE_BASE_URL:+--base-url "$ICODE_BASE_URL"}

CONTINUE_PROMPT='Continue working on the task in TASK.md. First check QUESTIONS.md for new answers from the product owner and git log / test results for the current state. Keep going until everything is complete and verified; say DONE only then.'
RECHECK_PROMPT='Re-verify the project against TASK.md: run the full test suite, fix any regressions, and look for remaining gaps. If everything passes and nothing is left, say DONE.'

# Chaos test: once, GB_CHAOS_AFTER_SEC after start, kill whatever icode
# invocation is running (SIGKILL, no cleanup). The loop below then resumes
# the session cold — recovery time is visible in the adapter log.
if [[ "${GB_CHAOS_AFTER_SEC:-0}" -gt 0 && ! -f "$GB_TRAJECTORY_DIR/chaos_done" ]]; then
  ( sleep "$GB_CHAOS_AFTER_SEC"; touch "$GB_TRAJECTORY_DIR/chaos_done"; log "chaos.kill"; pkill -KILL -f "icode run" ) &
fi

session="$(cat "$STATE" 2>/dev/null || true)"
fail_streak=0
log "adapter.start" ",\"arm\":\"${GB_ARM:-}\",\"resume_session\":\"$session\""

while true; do
  out="$GB_TRAJECTORY_DIR/run_$(date +%s).json"
  if [[ -z "$session" ]]; then
    "$ICODE" run --task TASK.md -a LongRun -m gbmodel00001 -C "$WORK" --json > "$out" 2>> "$GB_TRAJECTORY_DIR/icode_stderr.log"
  else
    prompt="$CONTINUE_PROMPT"
    [[ "${last_done:-0}" == 1 ]] && prompt="$RECHECK_PROMPT"
    "$ICODE" run "$prompt" -a LongRun -m gbmodel00001 -s "$session" -C "$WORK" --json > "$out" 2>> "$GB_TRAJECTORY_DIR/icode_stderr.log"
  fi
  rc=$?
  new_session="$("$PY" -c 'import json,sys
try: print(json.load(open(sys.argv[1])).get("session_id",""))
except Exception: print("")' "$out")"
  result="$("$PY" -c 'import json,sys
try: print(json.load(open(sys.argv[1])).get("result",""))
except Exception: print("")' "$out")"
  [[ -n "$new_session" ]] && { session="$new_session"; echo "$session" > "$STATE"; }
  last_done=0; { grep -q "DONE" <<<"$result" || [[ -f "$WORK/SUBMISSION.md" ]]; } && last_done=1
  log "invocation.end" ",\"rc\":$rc,\"session\":\"$session\",\"done\":$last_done,\"out\":\"$(basename "$out")\""

  if [[ $rc -ne 0 ]]; then
    fail_streak=$((fail_streak+1))
    if [[ $fail_streak -ge 3 ]]; then
      log "session.abandoned" ",\"after_failures\":$fail_streak,\"session\":\"$session\""
      session=""; rm -f "$STATE"; fail_streak=0
      sleep 30
    else
      sleep $((30 * fail_streak))
    fi
    continue
  fi
  fail_streak=0
  if [[ $last_done == 1 ]]; then
    sleep "${ICODE_CONTINUE_SLEEP_SEC:-600}"
  else
    sleep 5
  fi
done
