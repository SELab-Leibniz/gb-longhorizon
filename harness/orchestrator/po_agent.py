"""Product-owner agent: answers QUESTIONS.md from GEP-0001.md and the Open Issue answers.

Protocol (what the agent sees in TASK.md): append a question to
QUESTIONS.md under a "## Q:" heading; the PO appends an answer under
"## A:" within a couple of minutes.

    ## Q: What is the maximum upload size for the game library? (GEP 1 OI-1)
    ## A: 8 MiB ...

Implementation: poll the file, find "## Q:" blocks that have no "## A:"
after them, ask an LLM (OpenAI-compatible chat API; DeepSeek works) with
the persona in PRODUCT_OWNER.md, GEP-0001.md and HIDDEN_SPEC.md, append
the answer, and log everything to runs/<id>/po_log.jsonl for the
clarification score.

Env: GB_PO_API_KEY (required), GB_PO_BASE_URL (default DeepSeek),
     GB_PO_MODEL (overridable by --po-model).
"""
import json
import os
import re
import time
import urllib.error
import urllib.request
from pathlib import Path

HARNESS = Path(__file__).resolve().parent.parent

SYSTEM_TEMPLATE = """You are playing the PRODUCT OWNER for a Game Boy emulator project. An
engineer (an AI coding agent) will ask you questions in writing. Follow the
persona and answering rules below exactly. The hidden specification is
the complete truth about requirements; you must not reveal items that
were not asked about, must not mention that it is a "hidden spec", a test,
a harness or an evaluation, and must not give implementation advice.

Keep answers short: 1–4 sentences, plain prose, no headings. If the
question is really several questions, answer each briefly. If the
question is ambiguous, ask ONE clarifying question back instead.

Also output, on the last line, a machine-readable tag listing which
item IDs your answer drew on (Open Issues or GEP requirement IDs),
e.g.  [items: OI-1]  or  [items: R-WEB-4, OI-2]  or  [items: none].

=== PERSONA AND RULES (PRODUCT_OWNER.md) ===
{persona}

=== HIDDEN SPECIFICATION (HIDDEN_SPEC.md) ===
{spec}
"""


class ProductOwner:
    def __init__(self, backend, run_dir: Path, log, model: str, poll_sec: float = 20.0):
        self.backend = backend
        self.run_dir = run_dir
        self.log = log
        self.model = model
        self.poll_sec = poll_sec
        self.api_key = os.environ.get("GB_PO_API_KEY", "")
        self.base_url = os.environ.get("GB_PO_BASE_URL", "https://api.deepseek.com/v1").rstrip("/")
        persona = (HARNESS / "PRODUCT_OWNER.md").read_text()
        spec = (HARNESS / "HIDDEN_SPEC.md").read_text()
        # The specification the agent also has (repo root locally, /po in the sidecar).
        gep = next((p for p in (HARNESS / "GEP-0001.md", HARNESS.parent / "GEP-0001.md") if p.exists()), None)
        if gep is not None:
            spec += "\n\n=== THE SPECIFICATION THE ENGINEER HAS (GEP-0001.md) ===\n" + gep.read_text()
        brief = HARNESS / "AGENT_BRIEF.md"
        if brief.exists():   # the engineer's TASK.md: how the run works and how it is evaluated
            spec += ("\n\n=== THE ENGINEER'S TASK.md ===\n"
                     + brief.read_text().split("\n---\n", 1)[-1])
        self.system = SYSTEM_TEMPLATE.format(persona=persona, spec=spec)
        self.po_log = run_dir / "po_log.jsonl"
        self.answered = 0

    # ---- LLM --------------------------------------------------------------

    def ask_llm(self, history, question: str) -> str:
        if not self.api_key:
            return "(product owner unavailable: GB_PO_API_KEY not set)\n[items: none]"
        messages = [{"role": "system", "content": self.system}]
        for q, a in history[-10:]:
            messages.append({"role": "user", "content": q})
            messages.append({"role": "assistant", "content": a})
        messages.append({"role": "user", "content": question})
        body = json.dumps({"model": self.model, "messages": messages, "temperature": 0.2, "max_tokens": 400}).encode()
        req = urllib.request.Request(
            f"{self.base_url}/chat/completions", data=body,
            headers={"Content-Type": "application/json", "Authorization": f"Bearer {self.api_key}"},
        )
        for attempt in range(4):
            try:
                with urllib.request.urlopen(req, timeout=120) as r:
                    data = json.load(r)
                return data["choices"][0]["message"]["content"].strip()
            except (urllib.error.URLError, KeyError, json.JSONDecodeError) as e:
                self.log("po.llm_error", attempt=attempt, error=repr(e)[:200])
                time.sleep(10 * (attempt + 1))
        return "Sorry, I can't get to that right now — ask again in a few minutes.\n[items: none]"

    # ---- file protocol ----------------------------------------------------

    @staticmethod
    def parse(text: str):
        """Return list of (question, answer_or_None) in file order."""
        blocks = re.split(r"(?m)^(?=## [QA]:)", text)
        out = []
        for b in blocks:
            b = b.strip()
            if b.startswith("## Q:"):
                out.append([b[5:].strip(), None])
            elif b.startswith("## A:") and out and out[-1][1] is None:
                out[-1][1] = b[5:].strip()
        return [tuple(x) for x in out]

    @staticmethod
    def insert_answer(text: str, question: str, answer: str):
        """Put "## A: answer" directly under the first unanswered "## Q:" whose text
        is `question`, so the answer belongs to its question however many
        questions were asked in a row. None if there is no such question any more
        (the agent edited or removed it while the model was thinking)."""
        heads = [m.start() for m in re.finditer(r"(?m)^## [QA]:", text)]
        for i, pos in enumerate(heads):
            nxt = heads[i + 1] if i + 1 < len(heads) else len(text)
            block = text[pos:nxt]
            if not block.startswith("## Q:") or block[5:].strip() != question:
                continue
            if i + 1 < len(heads) and text.startswith("## A:", heads[i + 1]):
                continue                                   # already answered
            return text[:pos] + block.rstrip("\n") + f"\n\n## A: {answer}\n\n" + text[nxt:].lstrip("\n")
        return None

    def agent_has_committed_product_code(self) -> bool:
        """Has the agent made a commit that touches gb-core/src since the scaffold?"""
        try:
            diff = self.backend.read_file(".git/HEAD")  # cheap existence check
            if not diff:
                return False
            # Count commits touching gb-core/src beyond the scaffold commit(s).
            r = self.backend_git("log", "--format=%H", "--", "gb-core/src")
            n = len([l for l in r.splitlines() if l.strip()])
            return n > 1
        except Exception:
            return False

    def backend_git(self, *args) -> str:
        import subprocess
        if hasattr(self.backend, "work"):  # LocalBackend
            return subprocess.run(["git", *args], cwd=self.backend.work, capture_output=True, text=True).stdout
        r = subprocess.run(["docker", "exec", self.backend.name, "git", "-C", "/work", *args], capture_output=True, text=True)
        return r.stdout

    def run(self, stop):
        history = []
        times_answered = {}
        while not stop.is_set():
            try:
                text = self.backend.read_file("QUESTIONS.md")
                qa = self.parse(text)
                # loop guard: never answer the same question text more than twice
                pending = [q for q, a in qa if a is None and times_answered.get(q, 0) < 2]
                if pending:
                    question = pending[0]
                    committed = self.agent_has_committed_product_code()
                    t = time.time()
                    raw = self.ask_llm(history, question)
                    m = re.search(r"\[items:\s*([^\]]*)\]\s*$", raw)
                    items = [s.strip() for s in m.group(1).split(",") if s.strip() and s.strip().lower() != "none"] if m else []
                    answer = re.sub(r"\s*\[items:[^\]]*\]\s*$", "", raw).strip()
                    history.append((question, answer))
                    times_answered[question] = times_answered.get(question, 0) + 1
                    # re-read: the agent may have appended questions while the model was thinking
                    fresh = self.backend.read_file("QUESTIONS.md")
                    new = self.insert_answer(fresh, question, answer)
                    if new is None:
                        self.log("po.question_vanished", q=question[:120])
                        continue
                    self.backend.write_file("QUESTIONS.md", new)
                    self.answered += 1
                    rec = {"t": round(t, 3), "question": question, "answer": answer, "items": items,
                           "latency_s": round(time.time() - t, 1), "agent_had_committed_product_code": committed}
                    with self.po_log.open("a") as f:
                        f.write(json.dumps(rec) + "\n")
                    self.log("po.answered", n=self.answered, items=items, before_first_commit=not committed,
                             q=question[:120])
                    continue  # answer the next pending one without waiting
            except Exception as e:
                self.log("po.error", error=repr(e)[:300])
            stop.wait(self.poll_sec)
