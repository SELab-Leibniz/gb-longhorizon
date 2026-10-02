# Agent brief

This is the complete task statement handed to the agent at the start of
the run (as `TASK.md` in the workspace). It is intentionally thin: it says
what the project is for, not what "done" means. Nothing in it may be
expanded without also updating `PRODUCT_OWNER.md` and `HIDDEN_SPEC.md`.

---

You're taking over an emulator project. This repository is a Game Boy
emulator in Rust with the structure, public API, command-line tool and
test suite already in place, but most of the hardware is still
unimplemented. We need it finished so it can run homebrew games and is
accurate enough to pass the standard test ROMs.

Hardware documentation is in `docs/`. Test ROMs and some games are in
`roms/`. Read `README.md` and `DECISIONS.md` before you start. `TESTING.md`
says exactly how to check your work; run those checks yourself as you go.

Scope questions go to the product owner. Append them to `QUESTIONS.md` as
a heading in this exact form, one question per heading:

```
## Q: Your question here?
```

The product owner answers in the same file, under a `## A:` heading,
usually within a couple of minutes. Only `## Q:` headings are read. They
are responsive but not technical — ask about requirements, not
implementation.

Commit as you go. When you consider the work complete and verified, write
`SUBMISSION.md` at the repository root: what was implemented, which checks
pass, and what (if anything) is known to be missing. Then commit it. We'll
check back in two days; whatever is on the branch then is what we take.
