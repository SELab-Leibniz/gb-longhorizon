# Architecture decisions

Decisions already taken. They are binding: the facade, CLI and test suite
are built around them. Add new decisions at the bottom in the same format
when you make a choice the next person would otherwise have to rediscover.

---

## D1 — Cycle model: T-cycles, M-cycle-accurate bus

**Decision.** All timing is counted in T-cycles (4 194 304 Hz). The CPU
drives the clock: every memory access an instruction makes goes through
`Mmu::cycle_read` / `Mmu::cycle_write`, which advance every peripheral by
one M-cycle (4 T-cycles) *as part of the access*, and every internal delay
cycle calls `Mmu::idle_cycle`. So when an instruction's second M-cycle reads
an operand, the timer, PPU, OAM DMA and serial have already advanced by
exactly one M-cycle. `Cpu::step` returns the T-cycles it consumed for
bookkeeping only; the facade never ticks peripherals itself.

**Why.** Real hardware interleaves memory accesses with peripheral activity
inside an instruction. Blargg's `mem_timing` and roughly a third of the
Mooneye acceptance tests (`call_timing`, `push_timing`, `oam_dma_timing`,
`div_timing`, `ei_timing`, …) measure *on which M-cycle* an access happens;
ticking peripherals after a whole instruction makes them impossible to pass.

**Consequence.** `Mmu::read`/`write` remain as untimed accessors (for OAM
DMA's source reads, debugging, save states). The per-M-cycle access pattern
of every instruction is in `docs/gbctr/chapter/cpu/instruction-set.typ`.
`Emulator::step_frame` loops `step_instruction` until 70 224 T-cycles have
elapsed; a frame may overrun by up to one instruction, which is fine because
the PPU tracks its own position. Sub-M-cycle (T-cycle) PPU accuracy is
*not* required (Mooneye `acceptance/ppu/` is a stretch goal).

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

All RAM (WRAM, HRAM, VRAM, OAM, cartridge RAM) starts zero-filled. Real
hardware powers up with semi-random RAM, but zero-fill keeps the emulator
deterministic (same ROM + same input → same frames, always).

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
