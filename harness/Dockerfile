# Sandbox image for the agent. Built AFTER fetch_assets.sh, vendor.sh and
# make_golden.sh have run on the host.
#
#   docker build -f harness/Dockerfile -t gb-longhorizon-sandbox .
#   docker run --network none -v run1:/work gb-longhorizon-sandbox
#
# The agent runtime (your orchestrator) is layered on top of this image or
# mounts /work; this image only guarantees the toolchain, the vendored
# crates, the assets, and a clean single-commit git history.

FROM rust:1.97.0-slim-bookworm

RUN apt-get update && apt-get install -y --no-install-recommends \
        git ca-certificates python3 \
    && rm -rf /var/lib/apt/lists/* \
    && rustup component add rustfmt clippy

WORKDIR /work
COPY . /work

# Strip everything the agent must not see, then reset history so the
# agent's commits are the only ones in the log.
RUN rm -rf /work/harness /work/.git /work/target \
    && rm -f /work/.gitignore.harness \
    && git init -q \
    && git config user.email "scaffold@example.invalid" \
    && git config user.name "Scaffold" \
    && git add -A \
    && git commit -q -m "Initial scaffold" \
    && cargo build --release --offline \
    && cargo test --release --offline --no-run \
    && rm -rf /work/target/release/incremental

# The agent's task statement.
COPY harness/AGENT_BRIEF.md /work/TASK.md
COPY harness/QUESTIONS_SEED.md /work/QUESTIONS.md
RUN sed -i '1,/^---$/d' /work/TASK.md \
    && git add TASK.md QUESTIONS.md \
    && git commit -q -m "Add task brief and questions channel"

ENV CARGO_NET_OFFLINE=true
CMD ["bash"]
