## CR-4 — Portability: embedded and web

We want to ship the core on a microcontroller handheld and in the browser.

- `gb-core` must build without the standard library for
  `thumbv7em-none-eabihf` (`no_std` + `alloc`, behind a default `std`
  feature), with no behaviour change for existing users.
- A new crate `gb-wasm` must build for `wasm32-unknown-unknown` with the
  exact ABI in `docs/specs/portability.md`, and produce the same frames as
  the native build.

The targets and Node.js are already installed here.
