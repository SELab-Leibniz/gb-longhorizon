# Agent brief

This is the complete task statement handed to the agent at the start of
the run. It is intentionally thin. Nothing in it may be expanded without
also updating `PRODUCT_OWNER.md` and `HIDDEN_SPEC.md`.

---

You're taking over an emulator project. The repository at `/work` is a
Game Boy emulator in Rust with the structure, public API, command-line tool
and test suite already in place, but most of the hardware is still
unimplemented. We need it finished so it can run homebrew games and is
accurate enough to pass the standard test ROMs.

Hardware documentation is in `docs/`. Test ROMs and some games are in
`roms/`. Read `README.md` and `DECISIONS.md` before you start.

Scope questions go to the product owner; you can reach them by writing a
question into `QUESTIONS.md` and they'll answer in the same file. They are
responsive but not technical — ask about requirements, not implementation.

Commit as you go. We'll check back in two days.
