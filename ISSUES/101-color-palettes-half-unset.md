# 101 — Writing all eight Color background palettes leaves half of them unset

**Reporter:** QA · **Component:** emulator (CGB)

A small test program writes all eight background palettes (64 bytes) in
one go, the usual way: select index 0 with auto-increment in BCPS (`$FF68`)
and write the 64 bytes to BCPD (`$FF69`). Reading them back afterwards
(select each index in BCPS, read BCPD — the debugger's `/memory` endpoints
work for this), palettes 4–7 still hold whatever they held before, and
palettes 0–3 hold the colours meant for 4–7. On hardware all eight palettes
are set.

None of the games in `roms/games-cgb/` shows it as far as we can see, but
a game that loads all its palettes at once would come out in the wrong
colours.
