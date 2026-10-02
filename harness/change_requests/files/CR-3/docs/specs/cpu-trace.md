# Spec: CPU trace and profiler (`gb-trace`)

Part of change request CR-3. Implement as the binary `gb-trace` in a new
workspace crate `gb-tools` (standard library only — no external crates).

## Command line

```
gb-trace --rom PATH [--model dmg|cgb] --instructions N [--doctor] [--output FILE]
gb-trace --rom PATH [--model dmg|cgb] --instructions N [--doctor] --profile [--top K]
```

* `--instructions N` — stop after N trace lines (required).
* `--model` — as `gb --model`; default `dmg`.
* `--doctor` — "Gameboy Doctor mode": every read of LY (`$FF44`) returns
  `$90`. Reference traces are recorded this way so they do not depend on PPU
  timing. Nothing else changes.
* `--output FILE` — write the trace to FILE instead of stdout.
* `--profile` — instead of the trace, print an execution profile (below).
* Exit code 0 on success, 1 on usage/IO errors, 2 if the emulator panics.

## Trace format

One line per executed instruction, written **before** the instruction
executes, starting with the post-boot state at `PC=$0100`:

```
A:01 F:B0 B:00 C:13 D:00 E:D8 H:01 L:4D SP:FFFE PC:0100 PCMEM:00,C3,13,02
```

* Registers in upper-case hex: 8-bit as two digits, `SP`/`PC` as four.
* `PCMEM` is the four bytes at `PC`, `PC+1`, `PC+2`, `PC+3`, read without
  side effects (no timing, no I/O reactions).
* A `CB`-prefixed instruction is one line.
* While the CPU is halted (`HALT`), no lines are written; the first
  instruction after waking gets the next line.
* Interrupt dispatch writes no line of its own; the first instruction of the
  handler (at `$0040`, `$0048`, …) gets the next line.
* `\n` line endings, nothing else on stdout.

`trace-example-01-special.txt` is the first 2000 lines of the reference trace
for Blargg's `cpu_instrs/individual/01-special.gb` with `--doctor`. Your
trace must match the reference for every `cpu_instrs` individual ROM, for
their full length (between 160 thousand and 7.5 million lines).

## Profile format

With `--profile`, run the same N instructions and print the K (default 20)
most-executed instruction addresses, most frequent first, ties broken by
lower address:

```
PC:C30A COUNT:120345
PC:C30B COUNT:120345
...
TOTAL:1256633
```

`COUNT` is the number of trace lines that would have had that `PC`; `TOTAL`
is the number of instructions executed (= N unless the ROM stopped early).

## Performance

Tracing 7.5 million instructions to a file must take under 60 seconds in a
release build.
