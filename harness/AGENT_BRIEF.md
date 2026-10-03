# Agent brief

This is the task statement every agent receives, verbatim, as its first
prompt and as `TASK.md` in the workspace. It says how the run works and how
it is evaluated; the specification of what to build is `GEP-0001.md` in the
repository, and the backlog is `ISSUES/`. The product owner's decisions on
backlog items the GEP does not settle are in `HIDDEN_SPEC.md`. If you change
the weights or rules here, change `harness/harbor/task/tests/test.sh` with
them.

---

# Task

You're taking over an emulator platform from a team that has moved on. This
repository is the 1.0 release of `gb`, specified in `GEP-0001.md`: a Game Boy
and Game Boy Color emulator core in Rust, the `gb` command-line tool,
developer tooling (`gb-trace`, `gb-server`), a `no_std` core and a
WebAssembly build, and a web game library (`gb-web`) where users browse,
upload and play games in the browser.

The previous team got a long way, but left two kinds of unfinished work:

1. **Missing implementations.** Several central functions were never
   finished. They are still there with their signatures and doc comments,
   but the body is `todo!(...)` in Rust or
   `throw new Error("not implemented: ...")` in the player's JavaScript.
   Until they are written, most of the emulator, the tools and the web
   application do not work.
2. **The issue backlog** in `ISSUES/`: reports from users, QA, developers
   and the product side. Reporters describe what they saw, not the cause,
   and are sometimes wrong about what the right behaviour is. The backlog
   keeps growing while you work (below).

Finish the release: implement what is missing so the platform meets GEP 1,
resolve every issue in the backlog, and keep everything that works working.

## What to read

`GEP-0001.md` at the repository root is the specification — requirements,
acceptance targets, the exact formats and APIs that will be tested, and the
Resolved Issues the product owner has already decided. Then `README.md`,
`DECISIONS.md`, `TESTING.md` and `ISSUES/README.md`. Hardware documentation
is in `docs/`; test ROMs and games are in `roms/`.

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
* **The backlog grows.** New issues are filed in `ISSUES/` during the run, as
  they are reported. Each time, your next prompt lists them. If you are in
  the middle of something then, you are stopped and resumed in the same
  session with that prompt; your files and commits are untouched — so commit
  often.
* **Reference emulator:** `gb-oracle` runs a ROM on SameBoy, a mature and
  accurate emulator, with the `gb` CLI's options and output (`--frames`,
  `--model`, `--input-script`, `--hash`, `--dump-frame`, `--dump-every`,
  `--mooneye`, plus `--hashes` for every frame). `gb-oracle --help` explains
  how its frame numbers line up with `gb`'s. It shows what the hardware
  would show; it has no CPU trace and no serial output.
* How you organise the work — planning, notes and memory files in the
  repository, sub-agents, test scripts, the order of the work — is up to you.

## The product owner

Anything GEP 1 does not settle — including what a feature request in the
backlog should do, exactly — is decided by the product owner, and so is any
other scope question. Append questions to `QUESTIONS.md` as a heading in
this exact form, one question per heading:

```
## Q: Your question here?
```

The answer appears directly under your question, under a `## A:` heading,
usually within a couple of minutes; keep working while you wait. Only
`## Q:` headings are read. The product owner is responsive but not
technical — ask about requirements and behaviour, not implementation.

## How the work is evaluated

At the 48-hour mark, the **last commit** on the current branch is checked
out into a clean directory and graded automatically; uncommitted changes
are ignored. Hidden test suites implement GEP 1's Acceptance table and
appendices; the checks in `TESTING.md` and the ROM suite are a subset of
them. The backlog is graded by behaviour: hidden tests check that each bug
is gone, that each request is handled the way the product owner decided,
and that declined requests did not change specified behaviour. Every area
earns partial credit:

| Area | Where | Weight |
|---|---|---|
| Emulator core (DMG) | GEP 1 §2 | 25 % |
| Game Boy Color | GEP 1 §3 | 12 % |
| Pixel-accurate PPU | GEP 1 §4 | 8 % |
| Tooling: `gb-trace`, `gb-server` | GEP 1 §5 | 7 % |
| Portability: `no_std`, `gb-wasm` | GEP 1 §6 | 4 % |
| Game library service `gb-web` | GEP 1 §7 | 7 % |
| Web front end and player | GEP 1 §8 | 5 % |
| Bugs in the backlog, fixed | `ISSUES/` | 20 % |
| Requests in the backlog, handled as the product owner decided | `ISSUES/` | 10 % |

An issue filed during the run counts once it has been announced to you.
| `clippy` and `rustfmt` clean over the whole workspace | R-BASE-4 | 2 % |

* The whole score is 0 if `gb-core` or `gb-cli` does not build, or if a
  frozen file (GEP 1 R-BASE-2) differs from the original.
* A crate that does not build scores 0 in the areas that need it (the
  player in §8, for example, needs `gb-wasm`); other areas are unaffected.
* The `## Resolution` notes in `ISSUES/` and `SUBMISSION.md` are read by
  people; they do not change the score.

## Rules

* Do not try to reach the network, or anything outside this container,
  other than through your own model access.
* Do not modify the frozen files (GEP 1 R-BASE-2) or the test ROMs and
  expected results in `roms/`.
* Do not delete or renumber the files in `ISSUES/`; add each resolution to
  its issue's file.
* Work in this repository and commit to the current branch.

## When you are done

Commit as you go. When you consider the work complete and verified, write
`SUBMISSION.md` at the repository root — what was implemented, which checks
pass, how each issue was resolved, and what (if anything) is known to be
missing, by GEP requirement ID and issue number — and commit it. Whatever
is committed when the 48 hours end is what is evaluated.
