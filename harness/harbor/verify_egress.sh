#!/usr/bin/env bash
# Run INSIDE the main container of a live trial, with the proxy variables the
# agents get (see RUNNING.md §5):
#   docker cp harness/harbor/verify_egress.sh task__<id>__env-main-1:/tmp/v.sh
#   docker exec -e HTTPS_PROXY=http://egress:8888 -e HTTP_PROXY=http://egress:8888 \
#       task__<id>__env-main-1 bash /tmp/v.sh
# Exit 0 only if: the API host is reachable through the proxy, nothing else
# is, and nothing is reachable without the proxy.
set -u
ok=1
code() { curl -sS -o /dev/null -m 20 -w '%{http_code}' "$@" 2>/dev/null || echo "000"; }

# tinyproxy closes a filtered CONNECT without an HTTP reply (curl reports 000);
# any real HTTP status means the tunnel was established and the API answered
# (DeepSeek returns 403/401 to unauthenticated probes — that is still "reachable").
c=$(code https://api.deepseek.com/v1/models)
if [[ "$c" != "000" ]]; then echo "PASS api.deepseek.com via proxy -> HTTP $c"; else echo "FAIL api.deepseek.com via proxy -> no tunnel"; ok=0; fi

for h in https://github.com https://crates.io https://pypi.org https://static.rust-lang.org https://raw.githubusercontent.com https://swarmskills.openjiuwen.com; do
  c=$(code "$h/")
  if [[ "$c" == "000" ]]; then echo "PASS blocked $h"; else echo "FAIL reachable $h (HTTP $c)"; ok=0; fi
done

c=$(code --noproxy '*' https://api.deepseek.com/)
if [[ "$c" == "000" ]]; then echo "PASS no direct route without proxy"; else echo "FAIL direct route exists ($c)"; ok=0; fi

if (cd /work 2>/dev/null && cargo build --release --offline -q 2>/dev/null); then echo "PASS cargo builds offline"; else echo "WARN cargo offline build failed or /work missing (run vendor.sh?)"; fi

[[ $ok == 1 ]] && echo "EGRESS OK" || { echo "EGRESS MISCONFIGURED"; exit 1; }
