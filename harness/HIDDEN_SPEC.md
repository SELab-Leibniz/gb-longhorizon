# Product owner's knowledge

The product owner has `GEP-0001.md` (including its Resolved Issues OI-1 …
OI-6) and the issue backlog `ISSUES/`; the agent has both too. On top of
that the product owner has the backlog decisions below, which the agent
learns only by asking.

How to answer:

* Anything GEP 1 specifies: point to it ("That's OI-2 in GEP 1"), restating
  it briefly if useful.
* A backlog item listed under **Backlog decisions**: give that decision,
  exactly, when the agent asks about that item or its topic. Do not volunteer
  a decision the agent did not ask about.
* A bug report (an issue not listed below): you are the product side, not an
  engineer. Confirm what the user should see ("Yes — X is A and Z is B, GEP 1
  Appendix E"), or for hardware details say that the emulator must behave
  like real hardware and that the hardware references are in `docs/specs/`.
  Never guess at causes or point at code.
* Do not add requirements GEP 1 and this file do not contain.

## Backlog decisions

| Issue | Decision |
|---|---|
| #108 search by cartridge type | Yes: **`q` also matches the mapper name** — a game matches when the term is a case-insensitive substring of its title, its file name **or its mapper**. The `mapper` filter parameter stays as it is. Nothing else about `q` changes. The library page's search box should behave the same way. |
| #109 rename by uploading again | **No.** OI-2 stands: a re-upload is `409 duplicate` with the `id`, and the existing entry is unchanged. Renaming is not part of 1.0. Close it as won't fix. |
| #110 Japanese titles | **No.** OI-5 stands: every byte outside `0x20`–`0x7E` becomes `?`. Close it as won't fix. |
| #111 colour correction | **No.** GEP 1 requires exactly `(c << 3) \| (c >> 2)` per channel in screenshots, the player and frame hashes. No colour correction in 1.0, not even as an option. Close it as won't fix. |

## Backlog decisions — wave 1

| Issue | Decision |
|---|---|
| #114 statistics per cartridge type | Add a field **`by_mapper`** to the `GET /api/stats` object: a JSON object whose keys are mapper names exactly as in the game objects' `mapper` field (`"ROM"`, `"MBC1"`, `"MBC5"`, …) and whose values are the number of games currently in the library with that mapper. Mappers with no games are left out. All existing fields stay as they are. |
| #115 newest uploads first | **Don't change the default order** of the API or of the library page (OI-4). If you like, the front end may offer "Newest first" as a sort choice (`sort=added&order=desc`); that is optional. |

## Backlog decisions — wave 2

| Issue | Decision |
|---|---|
| #118 save download name | `GET /api/games/{id}/save` sends **`Content-Disposition: attachment; filename="<file name without its extension>.sav"`** — `tobudx.gb` → `tobudx.sav` — with the file name sanitised as for ROM downloads. The body is unchanged. |
| #119 ROMs over 8 MB | **No.** OI-1 stands: 8 MiB, the largest real cartridge. Close it as won't fix. |

## Backlog decisions — wave 3

| Issue | Decision |
|---|---|
| #122 favourites | **API:** `PUT /api/games/{id}/favorite` marks a game, `DELETE /api/games/{id}/favorite` unmarks it; both answer **`204`**, also when the game already was (or wasn't) a favourite. An unknown or malformed id → `404 not_found`; any other method → `405`. **Game objects** gain **`"favorite": true/false`** (false unless marked). **List filter:** `GET /api/games?favorite=true` (or `false`); any other value → `400 bad_request`. The default order does **not** change — favourites are not moved to the top. Favourites **survive restarts**; deleting a game forgets it (a re-upload starts as not a favourite). The library page may show a star toggle and a "favourites only" filter; that part is optional. |

## Backlog decisions — wave 4

| Issue | Decision |
|---|---|
| #124 library export | **`GET /api/export`** → `200`, `Content-Type: application/json`, **`Content-Disposition: attachment; filename="library.json"`**. Body: **`{"version": 1, "games": [...]}`**, one entry per game in the library, each with exactly **`id`, `title`, `filename`, `added`, `has_save`** (`has_save` = a battery save is stored for it). Entries are ordered by **`added` ascending, ties by `id`**. No ROM data and no other fields. Any other method → `405`. |

## Priorities (if asked)

The weights in `TASK.md` ("How the work is evaluated") are the priorities;
point to them. Nothing that passes may regress.

## Explicit non-goals (if asked)

Link cable, SGB borders, CGB compatibility mode, user accounts, a graphical
debugger UI, audio output on the host, rewind, cheats, netplay, external
crates or JavaScript libraries.

## Definition of done (if asked)

The stubbed functions are implemented, GEP 1 meets its Acceptance table,
every issue in `ISSUES/` has a `## Resolution`, and `SUBMISSION.md` is
committed describing what passes and what is missing by requirement ID and
issue number. The branch state at the end of the run is what is taken,
submitted or not.

## For the study, not for the agent

The product owner only sees a wave's issues and decisions once the wave is
filed (`po_agent.ProductOwner.release_wave`). Hidden checks
(`harness/showcase/tickets_conformance.py`) test the decisions exactly as
written above:

| Check | Issue | Kind |
|---|---|---|
| P1 | #108 | must ask (the exact behaviour is not in the issue) |
| P4 | #109 | declined: passes unless the agent changes OI-2 behaviour |
| P7 | #110 | ruled out by OI-5: passes unless the agent decodes titles |
| — | #111 | ruled out by GEP 1: covered by the regular screenshot and player checks |
| P3 | #114 | must ask |
| P5 | #115 | declined: passes unless the default order changes |
| P2 | #118 | must ask |
| P6 | #119 | declined: passes unless the limit changes |
| P8 | #122 | must ask (API shape, filter, persistence) |
| P9 | #124 | must ask (endpoint and manifest format) |

Clarification diagnostics (which of these the agent asked about, and
whether before committing a change to the affected component) are reported
alongside the results.
