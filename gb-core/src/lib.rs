//! `gb-core` — a headless Game Boy (DMG) and Game Boy Color (CGB) emulator core.
//!
//! The crate is organised the way the hardware is organised. Each module
//! owns one piece of the console and exposes a small, explicit interface;
//! [`Emulator`] wires them together and is the only type the outside
//! world (CLI, GUI, tests) needs.
//!
//! ```text
//!   Emulator
//!   ├── cpu        Sharp SM83 core: registers, decode/execute, interrupts
//!   ├── mmu        address decoding, WRAM/HRAM, I/O dispatch, OAM DMA
//!   │   ├── cartridge   ROM/RAM + memory bank controllers (MBC1/3/5)
//!   │   ├── ppu         pixel pipeline → 160×144 framebuffer, LCD regs
//!   │   ├── apu         four sound channels → sample buffer
//!   │   ├── timer       DIV / TIMA / TMA / TAC
//!   │   ├── joypad      P1 register + button state
//!   │   ├── serial      SB / SC, output captured for test ROMs
//!   │   └── interrupts  IF / IE flags
//!   └── util       hashing, small helpers
//! ```
//!
//! Hard rules for this crate (checked in CI):
//! * `#![forbid(unsafe_code)]`
//! * zero external dependencies
//! * deterministic: identical inputs → identical framebuffers, always
//!
//! See `DECISIONS.md` at the repository root for the architectural
//! choices that are already fixed (cycle model, boot behaviour, etc.).

// Some functions in this crate are stubs (`todo!()`, see their doc comments); the
// helpers they used are still here, so they show up as unused until the stubs are
// implemented again. Remove this allow when they are.
#![allow(dead_code, unused_imports)]
#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

pub mod apu;
pub mod cartridge;
pub mod cpu;
pub mod emulator;
pub mod interrupts;
pub mod joypad;
pub mod mmu;
pub mod ppu;
pub(crate) mod prelude;
pub mod serial;
pub mod timer;
pub mod util;

pub use cartridge::LoadError;
pub use emulator::{Emulator, Model, StepResult};
pub use joypad::Buttons;
pub use mmu::DataAccess;

/// LCD width in pixels.
pub const SCREEN_WIDTH: usize = 160;
/// LCD height in pixels.
pub const SCREEN_HEIGHT: usize = 144;
/// Number of pixels in one frame.
pub const FRAME_PIXELS: usize = SCREEN_WIDTH * SCREEN_HEIGHT;

/// Master clock in Hz (DMG).
pub const CLOCK_HZ: u32 = 4_194_304;
/// T-cycles per frame (154 scanlines × 456 dots).
pub const CYCLES_PER_FRAME: u32 = 70_224;
