## CR-3 — Developer tooling: CPU tracing, profiling and a debugger API

QA and the tools team need to inspect the emulator from outside. Please add
a workspace crate `gb-tools` (standard library only) with two binaries:

- `gb-trace` — CPU trace in the Gameboy Doctor format and an execution
  profiler. Spec: `docs/specs/cpu-trace.md` (an example reference trace is
  in `docs/specs/trace-example-01-special.txt`).
- `gb-server` — an HTTP/JSON debugger and automation API: load, step, run,
  breakpoints, watchpoints, memory, disassembly, screenshots, input, save
  states, profile. Spec: `docs/specs/debugger-api.md`.

Both specs are binding; the tools team will test against them exactly.
