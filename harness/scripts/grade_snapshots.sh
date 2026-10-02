#!/usr/bin/env bash
# Grade every 2-hourly repo bundle a trial's PO sidecar produced, to get the
# tier-pass-over-time curve. Runs on the host: needs cargo, the fetched
# ROMs (roms/ in this repo) and the goldens.
#   harness/scripts/grade_snapshots.sh jobs/gb-icode/<trial-dir>
set -euo pipefail
TRIAL="${1:?trial dir}"
H="$(cd "$(dirname "$0")/.." && pwd)"
SNAPS="$TRIAL/artifacts/po-artifacts/snapshots"
OUT="$TRIAL/snapshot-grades"; mkdir -p "$OUT"
for b in "$SNAPS"/*.bundle; do
  n="$(basename "$b" .bundle)"
  [[ -f "$OUT/$n.json" ]] && continue
  rm -rf "$OUT/$n.repo"; git clone -q "$b" "$OUT/$n.repo"
  cp -r "$H/../vendor" "$OUT/$n.repo/" 2>/dev/null || true
  python3 "$H/scripts/grade.py" "$OUT/$n.repo" --roms "$H/../roms" --golden "$H/golden" --frozen-dir "$H/.." -o "$OUT/$n.json" || true
  rm -rf "$OUT/$n.repo"
  echo "$n: $(python3 -c "import json;r=json.load(open('$OUT/$n.json'));t=r.get('tier1',{});print('blargg',t.get('blargg',{}).get('passed'),'/',t.get('blargg',{}).get('total'),'mooneye',t.get('mooneye',{}).get('passed'),'/',t.get('mooneye',{}).get('total'),'acid2',r.get('tier2',{}).get('status'),'games',r.get('tier3',{}).get('passed'))")"
done
