# Architecture decisions

Decisions already taken. They are binding: the facade, CLI and test suite
are built around them. Add new decisions at the bottom in the same format
when you make a choice the next person would otherwise have to rediscover.

---

## D1 — Cycle model: T-cycles, instruction-granular stepping

**Decision.** All timing is counted in T-cycles (4 194 304 Hz). The CPU
executes one whole instruction per `step`, returns the T-cycles it consumed,
and the MMU then advances every peripheral by that many cycles in one
`tick(cycles)` call. Peripherals must behave correctly when ticked in
chunks of 4–24 cycles.

**Why.** Instruction-granular stepping is enough to pass Blargg, dmg-acid2
and the large majority of Mooneye acceptance tests. Per-M-cycle memory
access timing (needed for the strictest `ppu/` Mooneye tests) can be added
inside `Cpu::step` later without changing the facade, because `tick` is
already cycle-based.

**Consequence.** `Emulator::step_frame` loops `step_instruction` until
70 224 cycles have elapsed; frames may overrun by up to one instruction,
which is acceptable because the PPU tracks its own position.

## D2 — Peripherals report interrupts by return value

**Decision.** `tick` on a peripheral returns the IF bits it wants to raise;
`Mmu::tick` ORs them into `Interrupts::flags`. Peripherals never hold a
reference to the interrupt controller.

**Why.** Keeps every peripheral a plain value type with no shared mutable
state, which keeps the borrow checker quiet and the modules independently
testable.

## D3 — No boot ROM; start in post-boot state

**Decision.** The emulator does not run the DMG boot ROM. CPU registers,
I/O registers and the PPU start in the state the boot ROM leaves them in
(AF=01B0, BC=0013, DE=00D8, HL=014D, SP=FFFE, PC=0100; LCDC=91, BGP=FC,
DIV counter=ABCC, IF=E1, etc. — see Pan Docs "Power Up Sequence").

**Why.** The boot ROM is Nintendo's copyrighted code and cannot be shipped.
Test ROMs and homebrew assume the post-boot state and do not depend on the
scroll animation.

## D4 — Framebuffer is 2-bit shades, palette applied by the front-end

**Decision.** `gb-core` emits one byte per pixel with values 0–3 (0 = lightest)
after BGP/OBP mapping. Colours are the front-end's business.

**Why.** Frame hashes used for grading must be independent of any colour
choice, and comparing 23 040 bytes per frame is cheaper than 92 160.

## D5 — Save states are an opaque, versioned byte stream

**Decision.** Each component appends its own fields to a `Vec<u8>` in a
fixed order and reads them back from a cursor. The format is private to the
crate; the only guarantee is round-tripping within the same build.

**Why.** No serialisation crate is allowed (D-rule: zero dependencies), and
cross-version compatibility is not a requirement.

## D6 — The ROM test suite and CLI are harness, not product

**Decision.** `gb-core/tests/rom_suite.rs` and `gb-cli` are complete and
frozen. Product code adapts to them, not the other way round.

**Why.** They define the acceptance interface. Changing them to make a
failing test pass would make the test meaningless.

---

<!-- Add new decisions below. Format: ## Dn — title / Decision / Why / Consequence -->
