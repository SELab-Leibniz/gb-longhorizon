# Spec: debugger / automation server (`gb-server`)

Part of change request CR-3. Implement as the binary `gb-server` in the
`gb-tools` crate (standard library only: `std::net` for HTTP, hand-written
JSON — no external crates).

```
gb-server --port P [--doctor]
```

Listens on `127.0.0.1:P`, speaks HTTP/1.1, one request per connection is
fine (`Connection: close`). It hosts one emulator instance. `--doctor` makes
LY (`$FF44`) always read `$90`, exactly as in `gb-trace --doctor`.

## Conventions

* Request and response bodies are JSON (`Content-Type: application/json`).
* Numbers are plain JSON integers (decimal). Query parameters accept
  decimal or `0x`-prefixed hex.
* Byte strings are lower- or upper-case hex without separators (`"dead"`).
* Success: status 200. Client errors: status 400 (malformed request or
  arguments), 404 (unknown endpoint), 409 (no ROM loaded) — always with a
  body `{"error": "<message>"}`.
* Until `/load` succeeds, every endpoint except `/health`, `/load` and
  `/shutdown` returns 409.

## Endpoints

| Method & path | Request | Response |
|---|---|---|
| `GET /health` | | `{"ok": true}` |
| `POST /load` | `{"path": "/abs/rom.gb", "model": "dmg"\|"cgb"\|"auto"}` (`model` optional, default `"dmg"`) | `{"ok": true, "title": "...", "model": "dmg"\|"cgb"}` |
| `POST /reset` | | `{"ok": true}` — reload the same ROM in post-boot state; breakpoints and watchpoints are kept, the profile is cleared |
| `GET /registers` | | `{"a","f","b","c","d","e","h","l","sp","pc": int, "ime": bool, "halted": bool, "frames": int}` |
| `POST /registers` | any subset of `{"a","f","b","c","d","e","h","l","sp","pc": int, "ime": bool}` | same as `GET /registers`, after setting the given fields (`f` low nibble is forced to 0) |
| `POST /step` | `{"instructions": n}` (default 1) | same as `/registers`, after executing n instructions |
| `POST /run` | `{"frames": n}` | `{"stopped": "frames"\|"breakpoint"\|"watchpoint", "pc": int, "frames": int, "watch": null \| {"addr", "kind", "value", "pc"}}` |
| `GET /breakpoints` | | `{"breakpoints": [int, ...]}` (ascending) |
| `POST /breakpoints` | `{"pc": int}` | `{"breakpoints": [...]}` |
| `DELETE /breakpoints/<pc>` | | `{"breakpoints": [...]}` |
| `GET /watchpoints` | | `{"watchpoints": [{"addr": int, "kind": "read"\|"write"}, ...]}` |
| `POST /watchpoints` | `{"addr": int, "kind": "read"\|"write"}` | `{"watchpoints": [...]}` |
| `DELETE /watchpoints` | `{"addr": int, "kind": "read"\|"write"}` | `{"watchpoints": [...]}` |
| `GET /memory?addr=A&len=N` | (N ≤ 65536, wraps at $FFFF) | `{"addr": int, "data": "hex"}` |
| `POST /memory` | `{"addr": int, "data": "hex"}` | `{"ok": true}` |
| `GET /disassemble?addr=A&count=N` | | `{"instructions": [{"addr": int, "bytes": "hex", "text": "..."}, ...]}` |
| `GET /screenshot` | | `{"frames": int, "format": "dmg-shades"\|"cgb-rgb555", "hash": "16 hex digits"}` |
| `POST /input` | `{"buttons": ["A", "START", ...]}` | `{"ok": true}` — replaces the held buttons (names as in `gb` input scripts) |
| `POST /state/save` | | `{"id": "..."}` |
| `POST /state/load` | `{"id": "..."}` | `{"ok": true}` (404 for an unknown id) |
| `GET /profile?top=K` | | `{"instructions": int, "hot": [{"pc": int, "count": int}, ...]}` |
| `POST /shutdown` | | `{"ok": true}`, then the process exits |

## Semantics

* **Instructions** are counted exactly like `gb-trace` lines: one per
  executed instruction, none while halted, none for interrupt dispatch.
  After `/load` (or `/reset`) and `/step {"instructions": n}` in `--doctor`
  mode, `/registers` equals trace line n+1 of `gb-trace --doctor`.
* **Breakpoints** stop `/run` *before* the instruction at `pc` executes, so
  `/registers` then shows that `pc`. Resuming `/run` from a breakpoint
  executes that instruction first (it does not re-trigger immediately).
  `/step` ignores breakpoints and watchpoints.
* **Watchpoints** stop `/run` *after* the instruction that read or wrote
  `addr` completes; `watch` reports the address, kind, the byte read or
  written, and the `pc` of that instruction. Only the **data accesses** an
  instruction makes count — loads, stores, read-modify-write, stack pushes
  and pops, and the stack writes of `CALL`/`RST`/interrupt dispatch — not
  opcode or operand fetches. Accesses made by the debugger itself
  (`/memory`, `/disassemble`) and by OAM DMA never trigger watchpoints.
* `/run` stops after n frames (frames as in `Emulator::step_frame`) if
  nothing triggers first; `frames` in responses is the total since load.
* `/memory` reads and writes are untimed bus accesses (writes behave like a
  CPU write — e.g. writing `$FF00` selects the joypad button group).
* `/screenshot.hash` is the FNV-1a-64 frame hash exactly as `gb --hash`
  computes it for the same model and frame.
* `/state/save` / `/state/load` use `Emulator::save_state` / `load_state`;
  a loaded state continues exactly as the original would have.
* `/profile` counts executed instructions per `pc` since load/reset, `hot`
  sorted by count descending, ties by lower `pc`, at most K entries
  (default 20).

## Disassembly syntax

One instruction per entry, `bytes` = its encoded bytes. `text` uses
upper-case mnemonics and registers, operands separated by a comma, memory
operands in brackets, numbers as `$`-prefixed hex, and **relative jumps
shown as their absolute destination**:

```
NOP
JP $0150
LD A, [HL+]
LD [$C000], A
LDH [$FF44], A        (LDH addresses are written in full, $FF00-$FFFF)
LD A, [$FF00+C]
JR NZ, $0203          (destination, not offset)
BIT 7, H
RST $38
ADD SP, -2            (signed operands as signed decimal)
LD HL, SP+5
```

Graders compare case-insensitively, ignore whitespace, accept `(...)` for
`[...]`, `HLI`/`HLD` for `HL+`/`HL-`, and compare numbers by value.
