#!/usr/bin/env python3
"""Block hashes for CPU traces in Gameboy Doctor format.

A trace is one line per executed instruction ("A:01 F:B0 ... PCMEM:00,C3,13,02").
Lines are normalised (strip, upper-case) and grouped into blocks of BLOCK
lines; each block is summarised by the first 16 hex digits of its MD5. Two
traces agree up to the first block whose hashes differ, which lets the grader
compare multi-million-line traces against references that are far too large
to ship.

    trace_blocks.py make  LOG(.zip|.txt) OUT.blocks        # build reference
    trace_blocks.py score OUT.blocks < agent_trace.txt      # prints JSON
"""
import hashlib
import io
import json
import sys
import zipfile

BLOCK = 1000


def iter_lines(stream):
    for raw in stream:
        line = raw.strip().upper() if isinstance(raw, str) else raw.decode("ascii", "replace").strip().upper()
        if line:
            yield line


def blocks(lines):
    h, n, total = hashlib.md5(), 0, 0
    for line in lines:
        h.update(line.encode() + b"\n")
        n += 1
        total += 1
        if n == BLOCK:
            yield h.hexdigest()[:16], total
            h, n = hashlib.md5(), 0
    if n:
        yield h.hexdigest()[:16], total


def make(src, out):
    if src.endswith(".zip"):
        z = zipfile.ZipFile(src)
        stream = io.TextIOWrapper(z.open(z.namelist()[0]), encoding="ascii")
    else:
        stream = open(src)
    total = 0
    with open(out, "w") as f:
        f.write(f"# block={BLOCK}\n")
        for digest, total in blocks(iter_lines(stream)):
            f.write(digest + "\n")
    with open(out, "a") as f:
        f.write(f"# lines={total}\n")
    return total


def load(ref):
    digests, lines = [], 0
    for line in open(ref):
        line = line.strip()
        if line.startswith("# lines="):
            lines = int(line.split("=", 1)[1])
        elif line and not line.startswith("#"):
            digests.append(line)
    return digests, lines


def score(ref, stream):
    want, want_lines = load(ref)
    matched = 0
    got_lines = 0
    for i, (digest, got_lines) in enumerate(blocks(iter_lines(stream))):
        if i < len(want) and digest == want[i] and matched == i:
            matched += 1
        elif matched == i:
            break   # first divergence: stop reading
    # matched blocks -> matched lines (the final block may be partial)
    matched_lines = min(matched * BLOCK, want_lines)
    return {"reference_lines": want_lines, "matched_lines": matched_lines,
            "first_divergent_block": matched if matched < len(want) else None,
            "fraction": round(matched_lines / want_lines, 6) if want_lines else 0.0}


if __name__ == "__main__":
    if sys.argv[1] == "make":
        print(make(sys.argv[2], sys.argv[3]))
    elif sys.argv[1] == "score":
        print(json.dumps(score(sys.argv[2], sys.stdin)))
