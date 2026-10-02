# Main sandbox image for the gb-longhorizon task.
#
# Everything that needs the network happens here, at build time:
#   * unpack the scaffold (git archive of this repo's HEAD, via sync.sh)
#   * fetch Pan Docs, opcode table, test ROMs, homebrew games
#   * vendor crates and switch cargo offline
#   * pre-install both coding agents (iCode, jiuwenswarm) and their Python runtimes
# At run time the container sits on an internal network whose only exit
# is the egress proxy (see docker-compose.yaml).
#
# Build args let you pin exact revisions for a reproducible study.

FROM rust:1.97.0-bookworm

ARG ICODE_REPO=https://github.com/openJiuwen-ai/iCode
# iCode main @ 2026-10-02
ARG ICODE_COMMIT=b5366a0954305535b143dd8e5bfc9c54917cfea0
ARG JW_REPO=https://github.com/openJiuwen-ai/jiuwenswarm
# jiuwenswarm develop (default branch; 'main' is the stale jiuwenclaw line)
ARG JW_COMMIT=f0a69728c96b5961d993449f1a901cbd2f4dac5b

ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
        git ca-certificates curl unzip cmake build-essential python3 python3-yaml ripgrep procps \
    && rm -rf /var/lib/apt/lists/* \
    && rustup component add rustfmt clippy

# uv (installs the Python versions each agent wants)
RUN curl -LsSf https://astral.sh/uv/install.sh | sh
ENV PATH="/root/.local/bin:${PATH}"

# ---- coding agents ----------------------------------------------------------
# iCode (package `chrys`; needs Python 3.14 — uv downloads it)
RUN git clone "$ICODE_REPO" /opt/icode && cd /opt/icode && git checkout -q "$ICODE_COMMIT" \
    && uv sync --no-dev
# jiuwenswarm (Python 3.11–3.13; engine `openjiuwen` comes from gitcode.com)
RUN git clone "$JW_REPO" /opt/jiuwenswarm && cd /opt/jiuwenswarm && git checkout -q "$JW_COMMIT" \
    && uv venv --python 3.12 && uv pip install -e .
# (the process SDK is imported from sdks/python/src by the adapter; no install needed)

# The repo's rust-toolchain.toml says "stable" (so dev machines build); inside
# the sandbox pin the toolchain the image ships, so rustup never tries to
# download a newer stable and every run uses the same compiler.
ENV RUSTUP_TOOLCHAIN=1.97.0

# ---- the task repository --------------------------------------------------
# scaffold.tar = `git archive HEAD` of this repo, staged by harness/harbor/sync.sh
COPY scaffold.tar /tmp/scaffold.tar
RUN mkdir -p /work && tar -xf /tmp/scaffold.tar -C /work && rm /tmp/scaffold.tar
WORKDIR /work
# The demo window (gb-gui) needs a display and pulls ~300 MB of platform
# crates (windows-sys, wayland, x11...) into the vendor dir. The sandbox is
# headless, so drop it here; gb-core and gb-cli have zero dependencies.
# The repo itself keeps gb-gui for the post-run demo.
RUN sed -i 's/members = \["gb-core", "gb-cli", "gb-gui"\]/members = ["gb-core", "gb-cli"]/' Cargo.toml \
    && rm -rf gb-gui \
    && python3 - <<'PYEOF'
import re, pathlib
p = pathlib.Path("README.md"); s = p.read_text()
s = s.replace("A small window front-end (`gb-gui`) exists\nfor demos.", "(A demo window front-end exists outside this headless environment.)")
s = re.sub(r"^gb-gui/ .*\n", "", s, flags=re.M)
s = re.sub(r"^cargo run -p gb-gui .*\n", "", s, flags=re.M)
p.write_text(s)
assert "gb-gui" not in s, "README still mentions gb-gui"
PYEOF
RUN sed -i '/gb-gui/d' Cargo.toml \
    && grep -q 'members = \["gb-core", "gb-cli"\]' Cargo.toml \
    && ! grep -q gb-gui Cargo.toml README.md
RUN harness/scripts/fetch_assets.sh \
    && harness/scripts/vendor.sh \
    && cargo build --release --offline \
    && cargo test --release --offline --no-run \
    && rm -rf target/release/incremental

# Adapter scripts (the loop that keeps each agent working for 48 h).
RUN mkdir -p /opt/gb-agents && cp -r /work/harness/agents/icode /work/harness/agents/jiuwenswarm /opt/gb-agents/

# ---- strip the harness and reset history ------------------------------------
# The agent must never see harness/ (hidden spec, goldens, grader) and its
# git log should start at the scaffold. Scaffold commits are authored
# "Scaffold"; the repo default identity is "Coding Agent", so every commit
# the agent makes is attributable in git-log.txt.
RUN cp harness/QUESTIONS_SEED.md /work/QUESTIONS.md \
    && rm -rf /work/harness /work/.github \
    && git init -q \
    && git add -A \
    && git -c user.email=scaffold@example.invalid -c user.name=Scaffold commit -q -m "Initial scaffold" \
    && git config user.email "agent@example.invalid" \
    && git config user.name "Coding Agent" \
    && git config --global --add safe.directory /work

ENV CARGO_NET_OFFLINE=true
# TASK.md is written by the agent wrapper from Harbor's instruction.md at
# run time so the two can never diverge.
