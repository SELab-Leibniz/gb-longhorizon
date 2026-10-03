# Hardware documentation

Populated by `harness/scripts/fetch_assets.sh`:

- `pandocs/` — Pan Docs, the community hardware reference, as Markdown.
  Start with `CPU_Instruction_Set.md`, `Memory_Map.md`, `Rendering.md`,
  `Timer_and_Divider_Registers.md`, `Interrupts.md`, `MBCs.md`,
  `Power_Up_Sequence.md`.
- `opcodes.json` — machine-readable table of all 512 opcodes: mnemonic,
  operands, byte length, cycle counts (taken / not taken) and flag effects;
  `opcode_descriptions.json` has a prose description of each.
- `gbctr/` — *Game Boy: Complete Technical Reference* (Gekkio, CC BY-SA 4.0),
  Typst source. `chapter/cpu/instruction-set.typ` gives every instruction
  **M-cycle by M-cycle** (which cycle fetches, reads, writes or idles) —
  the reference for the bus model in `DECISIONS.md` D1.
  `chapter/cpu/timing.typ` explains fetch/execute overlap. The cartridge and
  peripheral chapters complement Pan Docs.

- `specs/trace-example-01-special.txt` — the first 2 000 lines of the
  reference CPU trace for `cpu_instrs/individual/01-special.gb` (GEP 1
  Appendix A).

The specification of what to build is `GEP-0001.md` at the repository root.

The sandbox has no network. Everything you need about the hardware is in
this directory; if something is missing, ask the product owner.
