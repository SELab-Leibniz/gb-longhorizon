# Test and game ROMs

Nothing in this directory is committed to git except this file, the layout,
and `acid2/expected.fnv`. ROM files are fetched by
`harness/scripts/fetch_assets.sh` and baked into the sandbox image.

```
roms/
  test/
    blargg/     cpu_instrs/, instr_timing/, mem_timing/, halt_bug.gb
    blargg-mem/ dmg_sound/, oam_bug/  (report through cartridge RAM)
    mooneye/    acceptance/**, emulator-only/mbc1/**, emulator-only/mbc5/**
    acid2/      dmg-acid2.gb, expected.fnv
  games/        open-source homebrew .gb files (see LICENSES.md after fetch)
```

## How each family signals pass/fail

| Family | Signal | Notes |
|---|---|---|
| Blargg | Serial output contains `Passed` / `Failed` | `cpu_instrs`, `instr_timing`, `mem_timing`, `halt_bug`. |
| Blargg (memory) | Once $A001-$A003 = DE B0 61: $A000 is the status (0x80 running, 0x00 passed, other = failed); text at $A004 | `dmg_sound` (audio) and `oam_bug`. `gb --blargg-mem` implements this protocol. |
| Mooneye | `LD B,B` executed with B,C,D,E,H,L = 3,5,8,13,21,34 | 0x42 in every register means fail. `manual-only/`, `misc/` and ROMs for other models (SGB, DMG0, MGB, CGB, AGB suffixes) are excluded; `-GS`, `-dmgABC*` and unsuffixed ROMs target this DMG. |
| dmg-acid2 | FNV-1a hash of the framebuffer after 120 frames | Expected hash produced by the reference emulator. |

## Homebrew games

Ten titles chosen for open licence, source availability, coverage of
no-MBC / MBC1 / MBC5 (there is no open-licence MBC3 homebrew of note),
dual DMG/CGB headers, and variety of rendering (window, sprites, parallax,
scrolling). Provenance and licences: `LICENSES.md`. The harness keeps the golden frame hashes for these;
they are **not** in the sandbox.
