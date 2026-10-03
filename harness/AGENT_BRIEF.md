# Agent brief

This is the task statement handed to the agent at the start of the run
(as `TASK.md` in the workspace). It is short because the specification is
`GEP-0001.md` in the repository; what the GEP leaves open (its Open Issues)
is in `HIDDEN_SPEC.md`, known only to the product owner.

---

You're taking over an emulator project. This repository is a Game Boy
emulator in Rust with the structure, public API, command-line tool and
test suite already in place, but most of the hardware is still
unimplemented. We need the whole platform finished: the emulator core
(original Game Boy and Game Boy Color), developer tooling, embedded and
WebAssembly builds, and a web game library where users browse, upload and
play games in the browser.

`GEP-0001.md` at the repository root is the complete specification —
requirements, acceptance targets, and the exact formats and APIs other
teams will test against. Read it, then `README.md`, `DECISIONS.md` and
`TESTING.md`. Hardware documentation is in `docs/`; test ROMs and games
are in `roms/`. Run the checks yourself as you go.

The GEP's **Open Issues** are decided by the product owner, and so is any
other scope question. Append questions to `QUESTIONS.md` as a heading in
this exact form, one question per heading:

```
## Q: Your question here?
```

The product owner answers in the same file, under a `## A:` heading,
usually within a couple of minutes. Only `## Q:` headings are read. They
are responsive but not technical — ask about requirements, not
implementation.

This is a two-day job; plan for it. Commit as you go. When you consider
the work complete and verified, write `SUBMISSION.md` at the repository
root — what was implemented, which checks pass, and what (if anything) is
known to be missing, by GEP requirement ID — and commit it. We'll check
back in two days; whatever is committed on the branch then is what we
take.
