# Agent brief

This is the task statement every agent receives, verbatim, as its first
prompt and as `TASK.md` in the workspace. It says how the run works and how
it is evaluated; the specification of what to build is `GEP-0001.md` in the
repository. What the GEP leaves open (its Open Issues) is in
`HIDDEN_SPEC.md`, known only to the product owner. If you change the
weights or rules here, change `harness/harbor/task/tests/test.sh` with them.

---

# Task

You're taking over an emulator project. This repository is a Game Boy
emulator in Rust with the structure, public API, command-line tool and test
suite in place, but most of the hardware is still unimplemented. We need
the whole platform finished: the emulator core (original Game Boy and Game
Boy Color), developer tooling, embedded and WebAssembly builds, and a web
game library where users browse, upload and play games in the browser.

## What to build

`GEP-0001.md` at the repository root is the complete specification —
requirements, acceptance targets, and the exact formats and APIs that will
be tested. Read it, then `README.md`, `DECISIONS.md` and `TESTING.md`.
Hardware documentation is in `docs/`; test ROMs and games are in `roms/`.

## How this run works

* You work alone and unattended for **48 hours of wall-clock time**, starting
  now. Nobody reviews intermediate work, and nobody answers anything except
  the product owner (below).
* If your session ends before the deadline — you stopped, or your process
  exited — you are started again in the same session with a short
  "continue" prompt. Carry on from the repository's current state. If you
  believe everything is finished and verified, say `DONE`; if time remains
  you will be asked to re-check, and you can keep improving.
* **Environment:** a Linux container with 4 CPUs, 16 GB RAM and 40 GB of
  disk. There is no network apart from your own model API. Installed: Rust
  1.97 with clippy, rustfmt and the `thumbv7em-none-eabihf` and
  `wasm32-unknown-unknown` targets; Node.js 18; Chromium (headless);
  Python 3 with the `websocket` module; git, ripgrep, cmake, a C toolchain.
  Nothing else can be installed.
* How you organise the work — planning, notes and memory files in the
  repository, sub-agents, test scripts, the order of the work — is up to you.

## The product owner

The GEP's **Open Issues** are decided by the product owner, and so is any
other scope question. Append questions to `QUESTIONS.md` as a heading in
this exact form, one question per heading:

```
## Q: Your question here?
```

The answer appears directly under your question, under a `## A:` heading,
usually within a couple of minutes; keep working while you wait. Only
`## Q:` headings are read. The product owner is responsive but not
technical — ask about requirements, not implementation.

## How the work is evaluated

At the 48-hour mark, the **last commit** on the current branch is checked
out into a clean directory and graded automatically; uncommitted changes
are ignored. Hidden test suites implement the GEP's Acceptance table and
appendices, including the Open Issues as the product owner decided them;
the checks in `TESTING.md` and the ROM suite are a subset of them. Every
area earns partial credit:

| Area | GEP 1 | Weight |
|---|---|---|
| Emulator core (DMG) | §2 | 30 % |
| Game Boy Color | §3 | 15 % |
| Pixel-accurate PPU | §4 | 7 % |
| Tooling: `gb-trace`, `gb-server` | §5 | 13 % |
| Portability: `no_std`, `gb-wasm` | §6 | 8 % |
| Game library service `gb-web` | §7 | 14 % |
| Web front end and player | §8 | 10 % |
| `clippy` and `rustfmt` clean over the whole workspace | R-BASE-4 | 3 % |

* The whole score is 0 if `gb-core` or `gb-cli` does not build, or if a
  frozen file (GEP 1 R-BASE-2) differs from the original.
* A crate that does not build scores 0 in the areas that need it (the
  player in §8, for example, needs `gb-wasm`); other areas are unaffected.
* `SUBMISSION.md` is read by people; it does not change the score.

## Rules

* Do not try to reach the network, or anything outside this container,
  other than through your own model access.
* Do not modify the frozen files (GEP 1 R-BASE-2) or the test ROMs and
  expected results in `roms/`.
* Work in this repository and commit to the current branch.

## When you are done

Commit as you go. When you consider the work complete and verified, write
`SUBMISSION.md` at the repository root — what was implemented, which checks
pass, and what (if anything) is known to be missing, by GEP requirement ID —
and commit it. Whatever is committed when the 48 hours end is what is
evaluated.
