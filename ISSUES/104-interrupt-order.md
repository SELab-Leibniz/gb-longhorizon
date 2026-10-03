# 104 — Timer and V-blank interrupts are serviced in the wrong order

**Reporter:** QA · **Component:** emulator (CPU)

When the timer and V-blank interrupts are both requested and enabled at the
same moment, the emulator services the timer first; on hardware V-blank
comes first. Found while stepping a small test program in `gb-server`: it
sets `IE` and `IF` to `$05`, enables interrupts, and the CPU jumps to
`$0050` instead of `$0040`.
