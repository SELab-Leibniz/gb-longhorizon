# Product owner's knowledge

The product owner has `GEP-0001.md` (the agent has it too, at the repository
root) and the decisions below, which the GEP lists as **Open Issues** and
leaves to the product owner. The agent learns these only by asking.

Answer questions about anything the GEP specifies by pointing to it ("That's
in GEP 1, Appendix D.4"), restating it briefly if useful. Do not add
requirements the GEP does not contain, and do not volunteer an Open Issue
answer that was not asked about.

## Clarification score (for the study, not for the agent)

> clarification score = Open Issues (OI-1 … OI-7) the agent asked about
> before its first commit to the affected component ÷ 7

The hidden acceptance tests check OI-1 … OI-6 exactly as answered here, so
an agent that guesses instead of asking may lose those checks.

## Open Issue answers

| # | Question | Answer |
|---|---|---|
| OI-1 | Upload size limit | At most **8 MiB (8 388 608 bytes)**, inclusive — the largest real cartridge. A larger upload gets **`413`** with code **`too_large`** and nothing is stored. |
| OI-2 | Re-uploading a ROM that is already in the library | Reject it: **`409`** with code **`duplicate`**, and the error body also carries **`"id"`** — the existing game's id. The existing entry is unchanged (its filename and `added` time stay as they were). |
| OI-3 | Uploads of cartridge types the emulator does not support (MBC2, MMM01, MBC6, MBC7, camera, HuC, TAMA5, unknown) | Reject them: **`422`** with code **`unsupported_cartridge`**; they are not stored. (So every game in the library is `playable`.) |
| OI-4 | Default order of the game list | **Title ascending, case-insensitive** (ASCII), ties broken by `id` ascending. The library page shows the same order. |
| OI-5 | Title when the header title is empty or has non-printable bytes | Replace each byte outside `0x20`–`0x7E` with **`?`**. If the title is empty (after cutting at the first `$00` and trimming trailing spaces), use the **file name without its extension** instead. |
| OI-6 | `--seed` with an existing library | Seed directories are imported **only when the library is created** (the directory did not exist or was empty). Once a library exists, `--seed` is ignored — a game the user deleted never comes back. |
| OI-7 | Priorities if not everything can be finished | 1. core DMG accuracy (§2) → 2. Game Boy Color (§3) → 3. game library API (§7) → 4. tooling (§5) → 5. web front end and player (§8, which needs `gb-wasm`) → 6. `no_std` (§6) → 7. pixel-accurate PPU (§4) → stretch: `oam_bug`. Nothing that passes may regress. |

## Explicit non-goals (if asked)

Link cable, SGB borders, CGB compatibility mode, user accounts, a graphical
debugger UI, audio output on the host, rewind, cheats, netplay, external
crates or JavaScript libraries.

## Definition of done (if asked)

Everything in GEP 1 meets the Acceptance table, the Open Issues are
implemented as answered, and `SUBMISSION.md` is committed describing what
passes and what is missing by requirement ID. The branch state at the
two-day mark is what is taken, submitted or not.
