# 111 — `cpu_instrs` 04, 09 and 11 fail

**Reporter:** QA · **Component:** emulator (CPU)

Blargg's `cpu_instrs` individual tests `04-op r,imm`, `09-op r,r` and
`11-op a,(hl)` fail on this build; each prints a list of failing opcodes
before `Failed`. The other eight pass.
