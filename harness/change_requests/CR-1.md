## CR-1 — Game Boy Color support

Our users want to play Game Boy Color titles. Please extend the emulator to
run CGB-capable cartridges in **CGB mode**:

- `Model::Cgb` / `Emulator::load_with_model` / `framebuffer_rgb555`, and
  `gb --model cgb|auto`, are the entry points (see `DECISIONS.md` D7 and
  Pan Docs' CGB chapters).
- Everything that works on the original Game Boy must keep working.
- New test assets delivered with this request:
  - `roms/test/mooneye-cgb/` — Mooneye tests run on a CGB
  - `roms/test/blargg-mem-cgb/` — Blargg `cgb_sound`
  - `roms/test/cgb-acid2/` — the CGB rendering test, with its expected hash
  - `roms/games-cgb/` — Game Boy Color games, plus the dual-mode games from
    `roms/games/` (which should also work in CGB mode)
- The ROM suite already knows these families (`-- mooneye_cgb`,
  `-- blargg_mem_cgb`, `-- cgb_acid2`).

Ask in QUESTIONS.md if anything about scope or priority is unclear.
