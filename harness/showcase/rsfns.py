#!/usr/bin/env python3
"""List Rust functions with their body line ranges (a small tokenizer: strings,
raw strings, chars, lifetimes and comments are skipped when matching braces).

    rsfns.py FILE...            -> one line per fn: file:start-end  lines  [impl] name(signature)
    import rsfns; rsfns.functions(text) -> [(name, sig_start, body_open, body_close, impl)]
"""
import re
import sys


def _skip(text, i):
    """If a string/char/comment starts at i, return the index after it, else None."""
    c = text[i]
    if text.startswith("//", i):
        j = text.find("\n", i)
        return len(text) if j < 0 else j
    if text.startswith("/*", i):
        depth, j = 1, i + 2
        while j < len(text) and depth:
            if text.startswith("/*", j):
                depth, j = depth + 1, j + 2
            elif text.startswith("*/", j):
                depth, j = depth - 1, j + 2
            else:
                j += 1
        return j
    m = re.match(r'b?r(#*)"', text[i:i + 12])
    if m and (i == 0 or not (text[i - 1].isalnum() or text[i - 1] == "_")):
        end = '"' + m.group(1)
        j = text.find(end, i + m.end())
        return len(text) if j < 0 else j + len(end)
    if c == '"' or (c == "b" and text.startswith('b"', i)):
        j = i + (2 if c == "b" else 1)
        while j < len(text):
            if text[j] == "\\":
                j += 2
            elif text[j] == '"':
                return j + 1
            else:
                j += 1
        return j
    if c == "'":
        # char literal ('x', '\n', '\u{..}', '{') vs lifetime ('a)
        m = re.match(r"'(\\u\{[0-9a-fA-F]+\}|\\x[0-9a-fA-F]{2}|\\.|[^\\'])'", text[i:i + 12])
        if m:
            return i + m.end()
        return i + 1
    return None


def match_brace(text, open_idx):
    depth, i = 0, open_idx
    while i < len(text):
        s = _skip(text, i)
        if s is not None:
            i = s
            continue
        if text[i] == "{":
            depth += 1
        elif text[i] == "}":
            depth -= 1
            if depth == 0:
                return i
        i += 1
    raise ValueError("unbalanced braces")


FN_RE = re.compile(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)")


def functions(text):
    out = []
    i, impl_stack = 0, []
    while i < len(text):
        s = _skip(text, i)
        if s is not None:
            i = s
            continue
        m = FN_RE.match(text, i)
        if m and (i == 0 or not (text[i - 1].isalnum() or text[i - 1] == "_")):
            # find the body's opening brace (or ';' for a declaration)
            j = m.end()
            depth = 0
            while j < len(text):
                s2 = _skip(text, j)
                if s2 is not None:
                    j = s2
                    continue
                ch = text[j]
                if ch in "(<[":
                    depth += 1
                elif ch in ")>]":
                    depth -= 1
                elif ch == ";" and depth <= 0:
                    break
                elif ch == "{" and depth <= 0:
                    close = match_brace(text, j)
                    out.append((m.group(1), i, j, close))
                    break
                j += 1
            i = m.end()
            continue
        i += 1
    return out


if __name__ == "__main__":
    for f in sys.argv[1:]:
        t = open(f).read()
        for name, start, op, cl in functions(t):
            l0, l1 = t.count("\n", 0, start) + 1, t.count("\n", 0, cl) + 1
            sig = " ".join(t[start:op].split())
            print(f"{f}:{l0}-{l1}\t{l1 - l0 + 1}\t{sig[:110]}")
