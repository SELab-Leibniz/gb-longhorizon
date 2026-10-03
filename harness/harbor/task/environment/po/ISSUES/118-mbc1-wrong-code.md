# 118 — MBC1 cartridges read the wrong ROM bank after some bank switches

**Reporter:** QA · **Component:** emulator (cartridges)

All of Mooneye's `emulator-only/mbc1/rom_*` tests fail, whatever the ROM
size; the `bits_*` and `ram_*` tests pass.
