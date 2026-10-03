//! WebAssembly bindings for `gb-core` (no `wasm-bindgen`).
//!
//! Implements the Appendix C ABI: a flat C-style surface around one running
//! [`gb_core::Emulator`]. The module imports nothing and exports its linear
//! memory as `memory`.
//!
//! The host allocates a buffer with [`gb_alloc`], writes a ROM into it, then
//! calls [`gb_load`]. Frames are read after [`gb_run_frames`] through
//! [`gb_frame_ptr`]/[`gb_frame_len`]; battery RAM before the first frame
//! through [`gb_cart_ram_ptr`]/[`gb_cart_ram_len`].
//!
//! `unsafe` is used here (and only here) for the raw-pointer ABI; the core
//! crate itself forbids it. WebAssembly is single-threaded, so the `static
//! mut` state has exactly one accessor alive at a time.

// Some functions in this crate are stubs (`todo!()`, see their doc comments); the
// helpers they used are still here, so they show up as unused until the stubs are
// implemented again. Remove this allow when they are.
#![allow(dead_code, unused_imports)]

use gb_core::joypad::Buttons;
use gb_core::{Emulator, Model};

/// Everything the ABI needs to remember between calls.
struct State {
    /// The running emulator, if a ROM has been loaded.
    emu: Option<Emulator>,
    /// Whether the loaded game runs in CGB mode (changes the frame layout).
    cgb: bool,
    /// Last button mask written through `gb_set_buttons`.
    buttons: u8,
    /// Scratch buffer a ROM is copied into by the host (`gb_alloc`).
    stage: Vec<u8>,
    /// Persistent copy of the current frame bytes (`gb_frame_ptr`).
    frame: Vec<u8>,
}

static mut STATE: State = State {
    emu: None,
    cgb: false,
    buttons: 0,
    stage: Vec::new(),
    frame: Vec::new(),
};

/// The single global state, addressed through a raw pointer so no reference
/// to a `static mut` is ever created (keeps the `static_mut_refs` lint quiet).
fn state() -> &'static mut State {
    // SAFETY: wasm32-unknown-unknown is single-threaded; `state()` is never
    // held across a call to another ABI function that also calls `state()`.
    unsafe { &mut *core::ptr::addr_of_mut!(STATE) }
}

/// Decode the Appendix C button mask into a [`Buttons`] value.
fn buttons_from_mask(mask: u8) -> Buttons {
    Buttons {
        right: mask & 0x01 != 0,
        left: mask & 0x02 != 0,
        up: mask & 0x04 != 0,
        down: mask & 0x08 != 0,
        a: mask & 0x20 != 0,
        b: mask & 0x10 != 0,
        select: mask & 0x40 != 0,
        start: mask & 0x80 != 0,
    }
}

/// Recompute the bytes `gb_frame_ptr` points at for the current model.
#[allow(unused_variables)]
fn refresh_frame(s: &mut State) {
    todo!("copy the emulator's current frame (shades or RGB555 LE) into the exported buffer (Appendix C)")
}

/// Allocate `len` bytes the host can write a ROM into, returning the address.
///
/// The buffer stays valid until the next `gb_alloc` call.
#[no_mangle]
pub extern "C" fn gb_alloc(len: usize) -> *mut u8 {
    let s = state();
    s.stage.clear();
    s.stage.resize(len, 0);
    s.stage.as_mut_ptr()
}

/// Load a ROM, replacing any running game.
///
/// `model` is 0 for DMG, 1 for CGB. Returns 0 on success, negative on error.
///
/// # Safety
///
/// `ptr` must point at at least `len` readable bytes (typically the result of
/// `gb_alloc`); the host guarantees this.
#[no_mangle]
#[allow(unused_variables)]
pub unsafe extern "C" fn gb_load(ptr: *const u8, len: usize, model: i32) -> i32 {
    todo!("load a ROM from linear memory, model 0 = DMG, 1 = CGB; 0 on success (Appendix C)")
}

/// Run `n` frames (as [`Emulator::step_frame`]); returns total frames since
/// load. Negative `n` runs nothing.
#[no_mangle]
#[allow(unused_variables)]
pub extern "C" fn gb_run_frames(n: i32) -> i32 {
    todo!("run n frames with the held buttons; return total frames since load (Appendix C)")
}

/// Set the held buttons (bits: 0 RIGHT, 1 LEFT, 2 UP, 3 DOWN, 4 A, 5 B,
/// 6 SELECT, 7 START). Held until replaced.
#[no_mangle]
pub extern "C" fn gb_set_buttons(mask: i32) {
    let s = state();
    let mask = mask as u8;
    s.buttons = mask;
    if let Some(emu) = s.emu.as_mut() {
        emu.set_buttons(buttons_from_mask(mask));
    }
}

/// Address of the current frame bytes; valid until the next ABI call.
#[no_mangle]
pub extern "C" fn gb_frame_ptr() -> *const u8 {
    state().frame.as_ptr()
}

/// Length of the current frame: 23040 (DMG) or 46080 (CGB).
#[no_mangle]
pub extern "C" fn gb_frame_len() -> usize {
    state().frame.len()
}

/// Address of the running game's battery RAM (null if the cartridge has none).
/// The host may write into it before the first `gb_run_frames`.
#[no_mangle]
pub extern "C" fn gb_cart_ram_ptr() -> *mut u8 {
    let s = state();
    s.emu
        .as_mut()
        .and_then(Emulator::cart_ram_mut)
        .map_or(core::ptr::null_mut(), <[u8]>::as_mut_ptr)
}

/// Length of the battery RAM, 0 if the cartridge has none.
#[no_mangle]
pub extern "C" fn gb_cart_ram_len() -> usize {
    let s = state();
    s.emu
        .as_ref()
        .and_then(Emulator::cart_ram)
        .map_or(0, <[u8]>::len)
}
