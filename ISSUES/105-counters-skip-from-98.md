# 105 — Two-digit counters skip from 98 to garbage

**Reporter:** user · **Component:** emulator (CPU)

The score in my homebrew game is a two-digit decimal (BCD) counter: add 1,
then `DAA`. It goes 97, 98 and then shows `F9` instead of 99, and the
hundreds digit goes up at the same moment. The same ROM counts correctly
on hardware. Blargg's `cpu_instrs` test `01-special` also fails on this
build.
