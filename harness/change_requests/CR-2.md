## CR-2 — Pixel-accurate rendering

Several titles and demos change PPU registers in the middle of a scanline
(palettes, scroll, window, sprite size). A scanline renderer gets these
wrong. Please make rendering accurate at the pixel level on both DMG and
CGB — i.e. model the PPU's pixel pipeline.

- New test assets: `roms/test/mealybug-dmg/` and `roms/test/mealybug-cgb/`
  (the Mealybug Tearoom tests; each ROM's expected frame hash is next to it,
  derived from the authors' reference screenshots). Run with
  `-- mealybug_dmg` / `-- mealybug_cgb`.
- Mooneye's `acceptance/ppu/` tests are now in scope as well.
- No regressions in anything that passes today.
