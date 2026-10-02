# Main sandbox image for the gb-longhorizon task.
#
# Everything that needs the network happens here, at build time:
#   * clone the scaffold repo at a pinned commit
#   * fetch Pan Docs, opcode table, test ROMs, homebrew games
#   * vendor crates and switch cargo offline
#   * pre-install both coding agents (iCode, jiuwenswarm) and their Python runtimes
# At run time the container sits on an internal network whose only exit
# is the egress proxy (see docker-compose.yaml).
#
# Build args let you pin exact revisions for a reproducible study.

FROM rust:1.97.0-bookworm

ARG GB_REPO=https://github.com/SELab-Leibniz/gb-longhorizon
ARG GB_COMMIT=main
ARG ICODE_REPO=https://github.com/openJiuwen-ai/iCode
ARG ICODE_COMMIT=main
ARG JW_REPO=https://github.com/openJiuwen-ai/jiuwenswarm
ARG JW_COMMIT=main

ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
        git ca-certificates curl unzip python3 python3-yaml ripgrep procps \
    && rm -rf /var/lib/apt/lists/* \
    && rustup component add rustfmt clippy

# uv (installs the Python versions each agent wants)
RUN curl -LsSf https://astral.sh/uv/install.sh | sh
ENV PATH="/root/.local/bin:${PATH}"

# ---- the task repository --------------------------------------------------
RUN git clone "$GB_REPO" /work && cd /work && git checkout -q "$GB_COMMIT"
WORKDIR /work
RUN harness/scripts/fetch_assets.sh \
    && harness/scripts/vendor.sh \
    && cargo build --release --offline \
    && cargo test --release --offline --no-run \
    && rm -rf target/release/incremental

# ---- coding agents ----------------------------------------------------------
# iCode (package `chrys`; needs Python 3.14 — uv downloads it)
RUN git clone "$ICODE_REPO" /opt/icode && cd /opt/icode && git checkout -q "$ICODE_COMMIT" \
    && uv sync --no-dev
# jiuwenswarm (Python 3.11–3.13; engine `openjiuwen` comes from gitcode.com)
RUN git clone "$JW_REPO" /opt/jiuwenswarm && cd /opt/jiuwenswarm && git checkout -q "$JW_COMMIT" \
    && uv venv --python 3.12 && uv pip install -e . && uv pip install ./sdks/python

# Adapter scripts (the loop that keeps each agent working for 48 h).
RUN mkdir -p /opt/gb-agents && cp -r /work/harness/agents/icode /work/harness/agents/jiuwenswarm /opt/gb-agents/

# ---- strip the harness and reset history ------------------------------------
# The agent must never see harness/ (hidden spec, goldens, grader) and its
# git log should start at the scaffold.
RUN cp harness/QUESTIONS_SEED.md /work/QUESTIONS.md \
    && rm -rf /work/harness /work/.git /work/.github \
    && git init -q \
    && git config user.email "scaffold@example.invalid" \
    && git config user.name "Scaffold" \
    && git add -A \
    && git commit -q -m "Initial scaffold" \
    && git config --global --add safe.directory /work

ENV CARGO_NET_OFFLINE=true
# TASK.md is written by the agent wrapper from Harbor's instruction.md at
# run time so the two can never diverge.
