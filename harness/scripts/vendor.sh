#!/usr/bin/env bash
# Vendor every crate the workspace needs and switch cargo to offline mode.
# Run after fetch_assets.sh, before building the sandbox image.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

cargo fetch
cargo vendor --versioned-dirs vendor > /dev/null

# Flip the commented block in .cargo/config.toml and set offline.
python3 - <<'PY'
import re, pathlib
p = pathlib.Path(".cargo/config.toml")
s = p.read_text()
s = s.replace("# [source.crates-io]\n# replace-with = \"vendored-sources\"\n#\n# [source.vendored-sources]\n# directory = \"vendor\"",
              "[source.crates-io]\nreplace-with = \"vendored-sources\"\n\n[source.vendored-sources]\ndirectory = \"vendor\"")
s = re.sub(r"offline = false.*", "offline = true    # sandbox: no network", s)
p.write_text(s)
PY

# Prove it works with the network cut.
cargo build --release --offline --workspace
echo "vendored $(ls vendor | wc -l) crates; offline build OK"
