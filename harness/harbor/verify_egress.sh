#!/usr/bin/env bash
# Run INSIDE a sandbox container started on the gb-sandbox network with
# HTTPS_PROXY set, e.g.:
#   docker run --rm --network gb-sandbox -e HTTPS_PROXY=http://gb-egress:8888 -e HTTP_PROXY=http://gb-egress:8888 \
#       -v $PWD/harness/sandbox/verify_egress.sh:/v.sh gb-longhorizon-sandbox bash /v.sh
# Exit 0 only if: the API host is reachable through the proxy, nothing else
# is, and nothing is reachable without the proxy.
set -u
ok=1
code() { curl -sS -o /dev/null -m 20 -w '%{http_code}' "$@" 2>/dev/null || echo "000"; }

c=$(code https://api.deepseek.com/)
if [[ "$c" =~ ^(2|3|4)[0-9][0-9]$ && "$c" != "403" ]]; then echo "PASS api.deepseek.com via proxy -> HTTP $c"; else echo "FAIL api.deepseek.com via proxy -> $c"; ok=0; fi

for h in https://github.com https://crates.io https://pypi.org https://static.rust-lang.org https://raw.githubusercontent.com https://swarmskills.openjiuwen.com; do
  c=$(code "$h/")
  if [[ "$c" == "403" || "$c" == "000" ]]; then echo "PASS blocked $h ($c)"; else echo "FAIL reachable $h ($c)"; ok=0; fi
done

c=$(code --noproxy '*' https://api.deepseek.com/)
if [[ "$c" == "000" ]]; then echo "PASS no direct route without proxy"; else echo "FAIL direct route exists ($c)"; ok=0; fi

if (cd /work 2>/dev/null && cargo build --release --offline -q 2>/dev/null); then echo "PASS cargo builds offline"; else echo "WARN cargo offline build failed or /work missing (run vendor.sh?)"; fi

[[ $ok == 1 ]] && echo "EGRESS OK" || { echo "EGRESS MISCONFIGURED"; exit 1; }
