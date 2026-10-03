# 101 — `--model auto` runs dual-mode games in black and white

**Reporter:** QA · **Component:** emulator / `gb` CLI

`gb --model auto` should run a cartridge in Color mode whenever the
cartridge supports Color. Color-only games do start in colour, but games
that work on both (for example `roms/games/tobudx.gb`, `tuff.gb`) start in
black and white.
