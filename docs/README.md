# Hardware documentation

Populated by `harness/scripts/fetch_assets.sh`:

- `pandocs/` — Pan Docs, the community hardware reference, as Markdown.
  Start with `CPU_Instruction_Set.md`, `Memory_Map.md`, `Rendering.md`,
  `Timer_and_Divider_Registers.md`, `Interrupts.md`, `MBCs.md`,
  `Power_Up_Sequence.md`.
- `opcodes.json` — machine-readable table of all 512 opcodes: mnemonic,
  operands, byte length, cycle counts (taken / not taken) and flag effects.

The sandbox has no network. Everything you need about the hardware is in
this directory; if something is missing, ask the product owner.
