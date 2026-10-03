# Product-owner sidecar: answers QUESTIONS.md from the hidden spec and
# snapshots the repo every few hours. Shares the /work volume with the
# agent container but runs in its own container with its own network, so
# the agent never sees the spec or the PO's API key.
FROM python:3.12-slim
RUN apt-get update && apt-get install -y --no-install-recommends git && rm -rf /var/lib/apt/lists/*
WORKDIR /po
# Staged by harness/harbor/sync.sh from GEP-0001.md, harness/{HIDDEN_SPEC,PRODUCT_OWNER}.md
# and orchestrator/po_agent.py
COPY GEP-0001.md HIDDEN_SPEC.md PRODUCT_OWNER.md po_agent.py sidecar.py ./
RUN git config --global --add safe.directory /work
CMD ["python3", "-u", "/po/sidecar.py"]
