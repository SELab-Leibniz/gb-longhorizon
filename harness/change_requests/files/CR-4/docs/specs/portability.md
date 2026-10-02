# Spec: portability — embedded (`no_std`) and WebAssembly

Part of change request CR-4. The required Rust targets
(`thumbv7em-none-eabihf`, `wasm32-unknown-unknown`) and Node.js are
installed in this environment; nothing else may be added.

## 1. `gb-core` without the standard library

* `gb-core` gets a cargo feature `std`, enabled by default. With
  `--no-default-features` the crate is `#![no_std]` and uses only `core`
  and `alloc` (`Vec`, `Box`, `String` from `alloc`).
* This must build:

  ```
  cargo build -p gb-core --release --no-default-features --target thumbv7em-none-eabihf
  ```
* Everything that exists today keeps working unchanged with default
  features (`gb`, the ROM suite, `gb-tools`). Anything that genuinely needs
  `std` (e.g. `std::error::Error` impls) moves behind the `std` feature.
* No `unsafe` in `gb-core` (D-rule unchanged), no new dependencies.

## 2. WebAssembly build: `gb-wasm`

A new workspace crate `gb-wasm` (`crate-type = ["cdylib"]`, standard
library allowed, no external crates — no `wasm-bindgen`) that builds with

```
cargo build -p gb-wasm --release --target wasm32-unknown-unknown
```

into `target/wasm32-unknown-unknown/release/gb_wasm.wasm`. The module must
need **no imports** and export its linear memory as `memory` plus these
functions (all `i32` in the wasm ABI; `unsafe` is allowed in this crate
only, as needed for raw pointers):

| Export | Meaning |
|---|---|
| `gb_alloc(len) -> ptr` | allocate `len` bytes the host can write a ROM into |
| `gb_load(ptr, len, model) -> status` | load a ROM; `model` 0 = DMG, 1 = CGB; returns 0 on success, negative on error |
| `gb_run_frames(n) -> frames` | run n frames (as `Emulator::step_frame`); returns total frames since load |
| `gb_set_buttons(mask)` | held buttons: bit 0 RIGHT, 1 LEFT, 2 UP, 3 DOWN, 4 A, 5 B, 6 SELECT, 7 START |
| `gb_frame_ptr() -> ptr` | address of the current frame bytes |
| `gb_frame_len() -> len` | 23040 (DMG shades) or 46080 (CGB RGB555 little-endian) |

The frame bytes are exactly what `gb --hash` hashes, so a host can compute
the same FNV-1a-64 value. Running the same ROM for the same number of frames
with the same buttons must give the **same hash in WebAssembly as natively**.

A minimal host, for your own testing (`node host.js`, Node.js 18):

```js
const fs = require("fs");
(async () => {
  const wasm = fs.readFileSync("target/wasm32-unknown-unknown/release/gb_wasm.wasm");
  const { instance } = await WebAssembly.instantiate(wasm, {});   // no imports
  const e = instance.exports, rom = fs.readFileSync("roms/test/acid2/dmg-acid2.gb");
  const p = e.gb_alloc(rom.length);
  new Uint8Array(e.memory.buffer, p, rom.length).set(rom);
  if (e.gb_load(p, rom.length, 0) !== 0) throw new Error("load failed");
  e.gb_set_buttons(0);
  e.gb_run_frames(120);
  const frame = new Uint8Array(e.memory.buffer, e.gb_frame_ptr(), e.gb_frame_len());
  let h = 0xcbf29ce484222325n;                                      // FNV-1a-64
  for (const b of frame) h = ((h ^ BigInt(b)) * 0x100000001b3n) & 0xffffffffffffffffn;
  console.log(h.toString(16).padStart(16, "0"));   // must equal: gb --rom ... --frames 120 --hash
})();
```

The verifier runs a check like this for several DMG and CGB ROMs, with and
without held buttons, using a fresh instance per ROM. Read
`gb_frame_ptr()`/`gb_frame_len()` only after `gb_run_frames` returns, and
re-read `memory.buffer` afterwards (memory may have grown).
