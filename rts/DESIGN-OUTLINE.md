# RTS showcase — design outline (draft for review)

**Status:** draft v0.1, for review before any code is written.
**Working title:** *Meridian* (an original game; the name is a placeholder).
**Branch:** `rts-showcase` (reuses the showcase-v2 harness: product owner,
issue waves and their delivery, verifier, diagnostics).

Decisions taken so far:
- the game is our own original design, adopting Red Alert's mechanics;
- the base code and the reference simulation are written by us;
- the agent gets a one-page brief with deliberate gaps;
- the phases are as proposed below.

---

## 1. What the showcase has to show

A coding agent working for 48 hours on a game that is too big to finish
early, in a codebase it did not write, with requirements it has to pull out
of a product owner, and with a scope that changes during the run.

| Technique | Where the task demands it |
|---|---|
| Requirement clarification | The brief is vague on purpose; the exact rules, numbers and the definition of done are held by the product owner, topic by topic (§13). Later phases arrive as change requests. |
| Code localization | The inherited codebase (~3–5k lines) has core functions masked and a few planted bugs (§15). Bug reports describe symptoms only. |
| Memory | Four phases and a stream of bug reports over 48 h; earlier phases must keep working (regression checks). |
| Agent verifier | Deterministic simulation and replays the agent can test against; a public scenario set; a limited reference oracle (open decision, §18). |
| Context compaction | Length of run and size of the design. |

## 2. The game in one paragraph

Two factions fight over a tile map for a mineral called **ore**. Each side
deploys a mobile base vehicle into a construction yard and builds out a base
within reach of its existing buildings, powered by power plants. It harvests
ore with harvesters that unload at a refinery, trains infantry and builds
vehicles in factories, defends with turrets, and destroys the enemy's base.
The **Allied** side starts the run. The **Soviet** side and then a scripted
**mission** come later as change requests.

## 3. What we take from Red Alert, and what is ours

Everything is **mechanics only**: no Red Alert names, art, sounds, unit
stats or text. Every unit, building, weapon and number is ours.

| Red Alert mechanic | In *Meridian* | Phase |
|---|---|---|
| Deploying a construction vehicle into a construction yard | Same idea; the vehicle and yard have our own names and costs | 0 |
| Building only near existing buildings, on clear terrain | A "base reach" radius in cells, placement rules for terrain and occupancy | 0 |
| Ore fields, harvesters, refineries, silos; ore regrows | Ore density levels, harvester capacity, unload time, storage limits, regrowth rule | 0 |
| Power produced and consumed; low power slows production and disables some defences | Power balance, low-power production factor, list of power-dependent buildings | 0 |
| Production queues per category; several factories speed production; cost paid as it builds; pause and cancel | Same, with our own rates and refund rules | 0 |
| Selling buildings for part of their cost; repairing for credits | Refund rate, repair rate and cost | 0 |
| Weapons with damage, rate of fire, range and a warhead; warheads with a percentage against each armour type | Our weapon and warhead tables; armour types (none, light, heavy, concrete, …); our percentages | 0 / 1 |
| Splash damage with fall-off; tracked vehicles crushing infantry | Splash radius and fall-off rule; which units crush which | 0 / 1 |
| Engineers capturing or repairing buildings; medics healing infantry | Our engineer and medic rules | 0 |
| Shroud over unexplored map; a device that hides an area from the enemy | Shroud and sight ranges; the hiding device is a Phase 1 or 3 item | 0 / 3 |
| Faction-specific units with distinct mechanics (arc / chained damage, heavy twin-weapon tank, long-range rockets, attack dogs) | Soviet units built on these **mechanics**, with our own names and numbers | 1 |
| Superweapons (teleport, invulnerability, missile strike) | Optional late phase; our own names and rules | 3 (optional) |
| Mission scripting: triggers, teams, reinforcements, objectives | Our trigger/event/action model and mission file format | 2 |
| Skirmish AI | Reference AIs for grading; the agent's AI is graded by win rate | 0 → |

## 4. Simulation model (the determinism contract)

The rules are graded by exact hidden scenarios, so two correct
implementations must produce identical results:

- **Fixed simulation step:** e.g. 15 steps per second of game time. The
  renderer interpolates; the simulation never depends on frame rate.
- **Whole numbers only:**
  - positions in sub-cell units (e.g. 256 per cell);
  - facing as 0–255;
  - hit points, damage, credits and timers in integers;
  - every division has a stated rounding rule.
- **Order of updates is specified:**
  - commands → production → movement → combat → deaths → economy → vision;
  - entities update in ascending id order.
- **Randomness** (only where the rules call for it, such as artillery
  scatter): one specified generator (e.g. xorshift32), seeded per match, with
  a defined order of draws. The hidden scenarios that grade exact outcomes
  avoid random weapons; the others use tolerances.
- **Canonical state dump:** a JSON snapshot whose fields are defined by the
  contract. It is used by the tests, replays and the oracle.

## 5. Content by phase

Names are placeholders; rosters are deliberately modest so each unit carries
a distinct mechanic rather than just different numbers.

**Phase 0 — Allied** (in the inherited code, partly masked):
- **Buildings:**
  - construction yard;
  - power plant and advanced power plant;
  - refinery (comes with a harvester);
  - silo, barracks, war factory, radar, repair bay, service depot (optional);
  - defences: machine-gun nest, cannon turret, anti-air (if aircraft ever
    appear).
- **Units:**
  - infantry: rifleman, rocket infantry (anti-armour), engineer, medic;
  - vehicles: scout car, light tank, medium tank, artillery (splash, minimum
    range), harvester, the mobile base vehicle.

**Phase 1 — Soviet** (change request), with new mechanics, not only new
numbers:
- **Units:**
  - conscript (cheap, weak);
  - grenadier (arcing splash);
  - flame infantry (damage over time, burning-terrain rule);
  - attack dog (fast, one-hit kill on infantry only);
  - heavy tank;
  - siege tank (two weapons and self-repair, like the twin-weapon archetype);
  - rocket artillery (long range, slow reload, visible projectile that can
    be destroyed);
  - arc tank (**chained damage** jumping to nearby targets).
- **Buildings and economy:**
  - arc tower (powerful, goes offline on low power);
  - a different economy twist (e.g. a more expensive but faster harvester);
  - flame tower.
- **Cross-faction rules:**
  - armour and warhead table entries for every new pair;
  - a capture rule that differs by faction.

**Phase 2 — Mission** (change request):
- **The trigger system** (§11), a mission file format, and one predefined
  mission:
  - a hand-made map, a starting force, scripted enemy waves;
  - reinforcements, timed and conditional objectives;
  - win and lose conditions.
- **A mission browser** in the front end.

**Phase 3 — Patch and polish** (change request plus bug reports):
- a balance patch (numbers change; tests must follow the new numbers);
- play-tester bug reports;
- optionally the area-hiding device or one superweapon per side.

**Throughout (open-ended):**
- skirmish AI strength;
- pathfinding quality;
- performance with 500+ units.

## 6. Economy, power, construction and production (rules to specify)

- Starting credits, ore value per load and per density level, harvester
  capacity, harvest time per cell, unload time, and the silo storage limit
  (what happens to excess ore).
- Ore regrowth and spread: per-cell growth timer, spread rule.
- Power:
  - production and consumption per building;
  - the low-power production factor;
  - which buildings stop working on low power.
- Placement: base reach radius, terrain rules, footprint, overlap.
- Production queues:
  - one active item per category;
  - cost charged per step while building;
  - the speed-up per extra factory;
  - pause, cancel and refund;
  - a ready-to-place state for buildings.
- Selling and repair: refund fraction, repair rate and cost per step,
  interrupted repair.
- Capture: the engineer's effect depending on the building's health
  threshold.

## 7. Combat (rules to specify)

- **Weapon:** damage, rate of fire (steps between shots), range (min and
  max), projectile type (instant, travelling, arcing), projectile speed,
  warhead.
- **Warhead:** percentage against each armour type; splash radius and
  fall-off; whether it hits air, ground or both.
- **Damage resolution:**
  `damage × percent / 100`, rounded by a stated rule, never below 1 on a hit;
  then splash to the cells around.
- **Targeting:**
  - automatic target choice (priority list, nearest first, ties by id);
  - stance (guard or hold), retaliation, leash range.
- **Special rules:** infantry crushed by tracked vehicles; infantry going
  prone under fire (damage factor); chained damage (Phase 1); damage over
  time (Phase 1).

## 8. Movement and pathfinding

- Cell grid with terrain types (clear, rough, road, water, ore, rock), speed
  multipliers, and occupancy (infantry per cell, one vehicle per cell).
- Turning (facing change per step), speed per unit type, reverse movement
  (if any).
- **Pathfinding:**
  - *Correctness*: a path exists → the unit gets there; no path → it moves
    to the nearest reachable cell. Graded with exact scenarios.
  - *Quality*: arrival time and units stuck on benchmark maps, graded with
    tolerances. This dimension is open-ended.
- **Groups:** move orders for many units, avoiding deadlock.

## 9. Vision

- Shroud: unexplored cells are black until seen; sight range per unit.
- Fog: whether explored cells show only remembered state, decided in the
  design (a gap left for clarification).
- Radar: a minimap that needs the radar building and power.

## 10. Skirmish AI

- The agent's AI must build a base, harvest, defend and attack within the
  rules (no cheating: same rules, same information).
- **Grading:**
  - win rate against three of our reference AIs (easy, medium, hard), over
    seeded matches on fixed maps;
  - games capped at N steps, scored by a rule if no side wins.
- The reference AIs are deterministic and kept hidden.

## 11. Missions and triggers (Phase 2)

- **Model:** a trigger is an *event* and a *condition*, followed by
  *actions*.
  - events: time, unit enters area, building destroyed, side has no units, …;
  - actions: spawn team, reinforce, reveal area, show message, win, lose, …
- **Teams:** a composition plus a route or mission (move, attack, guard).
- **Mission file:** a format we specify. The format is a deliberate gap for
  clarification, or partly given.
- **Grading:**
  - scripted playthroughs (recorded input replays) must fire the triggers at
    the right steps and reach the right outcome;
  - plus human play.

## 12. Front end

- **Canvas rendering** of the map, units, buildings, shroud, health bars,
  selection boxes and the minimap.
- **Controls:**
  - drag-select and click-select;
  - right-click to move or attack;
  - a sidebar build menu with queue progress;
  - building placement preview;
  - hotkeys (groups, stop, guard).
- **Speed:** a stable frame rate with 500 units on screen.
- **Test hook:** a `window.rtsTest` contract, like `window.gbTest`:
  - load a scenario;
  - issue commands as a player;
  - step N simulation steps;
  - read the canonical state;
  - read the frame.

  This is how the interface checks and the scripted playthroughs drive the
  game.
- **Art:** shapes and colours drawn by code; no image files. Art is graded
  only for legibility.

## 13. What the agent sees and what the product owner holds

| Layer | Content |
|---|---|
| **Brief (agent)** | One page: the pitch (§2); the deliverables (playable game, headless simulation, test hook); that a product owner holds the design and the definition of done; how the run works. Deliberately thin on rules and numbers. |
| **Inherited code (agent)** | The base game (Phase 0, partly masked). Its code and comments imply some rules; a few are deliberately out of date with the design. |
| **Contracts (agent)** | Simulation API, canonical state format, test hook, replay format. These are given exactly, because grading depends on them. |
| **Design document (product owner, hidden)** | Every rule and number in §§4–11, split by phase. The product owner answers per topic and never dumps the whole document. Phases 1–3 become known to the product owner only when filed. |
| **Reference and tests (grader, hidden)** | The reference simulation, scenario corpus, reference AIs, mission playthroughs. |

**Deliberate gaps** (the brief is silent or vague; the inherited code
guesses, sometimes wrongly):
- the definition of done per phase;
- win and lose conditions;
- the low-power rules;
- silo overflow;
- capture thresholds;
- target priority;
- fog versus shroud;
- the mission format;
- what a balance patch overrides.

The diagnostics report which topics each agent asked about, and whether it
asked before building them.

## 14. Phases and change requests

Times are for the 48-hour run; a phase is filed early if the agent declares
the work complete. Delivery uses the existing notice mechanism (delivered as
a prompt, recorded, scored only once delivered).

| Phase | Filed at | Content |
|---|---|---|
| 0 | start | Allied game from the inherited code |
| 1 | ~8 h | Soviet faction (CR-1) |
| 2 | ~20 h | Mission system and one mission (CR-2) |
| 3 | ~32 h | Balance patch and bug reports (CR-3); optional superweapons |
| — | throughout | Bug reports in small waves; open-ended AI, pathfinding and speed |

## 15. The inherited codebase

- **About 3–5k lines:**
  - a deterministic simulation core (~2.5–3k);
  - a canvas front end (~1.5–2k);
  - a few scenario files and a starter map.
- **What works:** map loading, rendering, selection, movement on open
  ground, the build menu interface, a few Allied units.
- **Masked to signatures** (like v2):
  - the combat resolution step;
  - the production queue step;
  - the harvesting state machine;
  - the pathfinder;
  - the power balance effects;
  - parts of the test hook.
- **Planted bugs** (like v2's second round): none in files with masked
  functions, symptoms far from causes, each with a hidden check validated
  with and without the bug.

## 16. Evaluation

| Layer | When | What |
|---|---|---|
| Rule correctness (automated) | every hour (snapshots) and at the end | hidden exact scenarios per area and phase; earlier phases kept as regression checks |
| Open-ended quality (automated) | every hour and at the end | win rate against the reference AIs; pathfinding benchmarks; speed with 500 units; interface checks in headless Chromium |
| Product decisions (automated) | at the end | hidden checks of the definition-of-done items the product owner decides |
| Human play-testing | at each phase checkpoint and at the end | blind A/B builds; at least 3 raters; scripted probe missions with pass/fail, then free play rated against a rubric |

**Draft weights:**
- rule correctness 40 % (Phase 0: 15, Phase 1: 12, Phase 2: 8, Phase 3: 5);
- open-ended quality 20 %;
- product decisions 10 %;
- human rating 25 %;
- code health (build, lint, tests) 5 %.

Validation before any run:
- every hidden check passes on the reference;
- a second, independent implementation agrees with the reference on every
  scenario;
- every graded behaviour is traced to a section of the design document;
- planted bugs are validated as in v2.

## 17. Technology and constraints

- **Recommended:**
  - simulation core in **Rust**: integer maths, compiled to WebAssembly for
    the browser and natively for the grader, reusing the existing toolchain
    and checks;
  - front end in plain JavaScript and Canvas;
  - **no third-party dependencies** (the sandbox is offline).
- **Alternative:** TypeScript or JavaScript throughout. It's simpler for the
  front end, but determinism is harder to guarantee.
- **Sandbox:** as now: offline, Rust, Node.js, headless Chromium, product
  owner over `QUESTIONS.md`, issue waves delivered as prompts.

## 18. Open decisions

1. Rust core + JavaScript front end (recommended), or JavaScript/TypeScript
   only?
2. A limited reference oracle in the sandbox (final state and frame hashes
   for a scenario; no per-step traces), or none?
3. Aircraft or naval units in a later phase, or not at all?
4. Superweapons in Phase 3, or not at all?
5. Who the human raters are, and how many. This decides how much the human
   score can weigh.
6. Rough size of the design document: about 25–40 pages of rules and
   tables, written first, then reviewed before the reference is built.
