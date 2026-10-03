//! The public facade: owns the CPU and the bus, and exposes the handful of
//! operations the CLI, GUI and test harness need.
//!
//! This file is **fully implemented plumbing**. The behaviour lives in the
//! modules it calls into, which are stubs for the agent to implement.

use crate::cpu::{Cpu, Registers};
use crate::joypad::Buttons;
use crate::mmu::{DataAccess, Mmu};
use crate::prelude::*;
use crate::{cartridge::Cartridge, LoadError, CYCLES_PER_FRAME};

/// Outcome of executing one instruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepResult {
    /// Instruction executed; carries the time it took in master-clock
    /// T-cycles (4 per M-cycle at normal speed, 2 per M-cycle in CGB
    /// double-speed mode), already applied to the peripherals by the CPU's
    /// bus accesses.
    Ran(u32),
    /// The CPU executed `LD B,B` (opcode 0x40, 4 T-cycles, already ticked).
    /// Test ROMs from the Mooneye suite use this as a software breakpoint to
    /// signal completion; the harness inspects the registers when it sees this.
    Breakpoint,
}

/// Which console is emulated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Model {
    /// Original Game Boy (DMG). The model the initial brief asks for.
    Dmg,
    /// Game Boy Color running in CGB mode (colour palettes, VRAM/WRAM banks,
    /// HDMA, double speed). Not part of the initial requirements; the API is
    /// here so the harness can address it if the scope grows.
    Cgb,
}

impl Model {
    /// The model a cartridge asks for: CGB if header byte 0x143 has bit 7 set
    /// (0x80 = works on both, 0xC0 = CGB only), otherwise DMG.
    pub fn for_rom(rom: &[u8]) -> Model {
        match rom.get(0x143) {
            Some(flag) if flag & 0x80 != 0 => Model::Cgb,
            _ => Model::Dmg,
        }
    }
}

/// A complete Game Boy system.
pub struct Emulator {
    cpu: Cpu,
    mmu: Mmu,
    model: Model,
    frames: u64,
    instructions: u64,
    rom: Vec<u8>,
}

impl Emulator {
    /// Build a DMG system around the given ROM image (same as
    /// `load_with_model(rom, Model::Dmg)`).
    ///
    /// No boot ROM is run: the CPU and I/O registers start in the state the
    /// boot ROM leaves them in (see `DECISIONS.md`, D3).
    pub fn load(rom: &[u8]) -> Result<Self, LoadError> {
        Self::load_with_model(rom, Model::Dmg)
    }

    /// Build a system of the given model around the ROM image.
    pub fn load_with_model(rom: &[u8], model: Model) -> Result<Self, LoadError> {
        let cart = Cartridge::from_bytes(rom)?;
        Ok(Self {
            cpu: Cpu::post_boot(model),
            mmu: Mmu::new(cart, model),
            model,
            frames: 0,
            instructions: 0,
            rom: rom.to_vec(),
        })
    }

    /// The model this system emulates.
    pub fn model(&self) -> Model {
        self.model
    }

    /// Execute exactly one instruction (plus any interrupt dispatch that
    /// precedes it). The CPU advances the peripherals itself, one M-cycle per
    /// bus access (DECISIONS.md D1); this method does not tick anything.
    pub fn step_instruction(&mut self) -> StepResult {
        self.instructions += 1;
        self.cpu.step(&mut self.mmu)
    }

    /// Run until one full frame (70 224 T-cycles of the 4 MiHz master clock;
    /// in CGB double-speed mode the CPU executes twice as many of its own
    /// cycles in that time — see `StepResult::Ran`) has elapsed.
    ///
    /// Returns `true` if a `LD B,B` breakpoint was hit during the frame.
    pub fn step_frame(&mut self) -> bool {
        let mut elapsed = 0u32;
        let mut hit_breakpoint = false;
        while elapsed < CYCLES_PER_FRAME {
            match self.step_instruction() {
                StepResult::Ran(c) => elapsed += c,
                StepResult::Breakpoint => {
                    hit_breakpoint = true;
                    // `LD B,B` is a 4-cycle instruction; its fetch already
                    // ticked the bus, so only account for the time here.
                    elapsed += 4;
                }
            }
        }
        self.frames += 1;
        hit_breakpoint
    }

    /// The most recently completed frame, 160×144 bytes, row-major, shade
    /// 0..=3 (DMG). In CGB mode use [`Self::framebuffer_rgb555`].
    pub fn framebuffer(&self) -> &[u8] {
        self.mmu.ppu.framebuffer()
    }

    /// The most recently completed frame as 160×144 RGB555 pixels
    /// (`r | g << 5 | b << 10`, 5 bits each). Valid in both modes; in DMG
    /// mode the four shades map to 0x7FFF, 0x56B5, 0x294A, 0x0000.
    pub fn framebuffer_rgb555(&self) -> &[u16] {
        self.mmu.ppu.framebuffer_rgb555()
    }

    /// Number of frames completed since load.
    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// Replace the current button state. Held buttons stay held until the
    /// next call; the joypad raises its interrupt on high→low transitions.
    pub fn set_buttons(&mut self, buttons: Buttons) {
        self.mmu.joypad.set_buttons(buttons);
    }

    /// Drain everything the game has written to the serial port since the
    /// last call. Blargg's test ROMs print their results here.
    pub fn take_serial(&mut self) -> Vec<u8> {
        self.mmu.serial.take_output()
    }

    /// Snapshot of CPU registers (for the Mooneye breakpoint protocol and
    /// for debugging).
    pub fn registers(&self) -> Registers {
        self.cpu.registers()
    }

    /// Serialise the full machine state. Format is private to this crate;
    /// the only contract is `load_state(save_state())` is a no-op.
    pub fn save_state(&self) -> Vec<u8> {
        let mut out = Vec::new();
        self.cpu.save_state(&mut out);
        self.mmu.save_state(&mut out);
        out
    }

    /// Restore a state produced by [`Self::save_state`].
    pub fn load_state(&mut self, state: &[u8]) -> Result<(), StateError> {
        let mut cursor = 0usize;
        self.cpu.load_state(state, &mut cursor)?;
        self.mmu.load_state(state, &mut cursor)?;
        Ok(())
    }

    /// Untimed read of one bus address (no peripheral advance). Used by the
    /// harness to read results that test ROMs leave in memory (Blargg's
    /// `dmg_sound` / `oam_bug` write theirs to cartridge RAM at $A000).
    pub fn peek(&self, addr: u16) -> u8 {
        self.mmu.read(addr)
    }

    /// Cartridge RAM contents (battery-backed saves), if the cartridge has RAM.
    pub fn cart_ram(&self) -> Option<&[u8]> {
        self.mmu.cartridge.ram()
    }

    /// Mutable cartridge RAM (WebAssembly host writes a battery save straight
    /// into linear memory before the first frame).
    pub fn cart_ram_mut(&mut self) -> Option<&mut [u8]> {
        self.mmu.cartridge.ram_mut()
    }

    /// Overwrite cartridge RAM (loading a battery save). Extra bytes are
    /// ignored, and a cartridge without RAM is left untouched.
    pub fn set_cart_ram(&mut self, data: &[u8]) {
        self.mmu.cartridge.set_ram(data);
    }

    /// Enable or disable "Gameboy Doctor" mode: reads of LY (`$FF44`) return
    /// `$90` so reference traces do not depend on PPU timing.
    pub fn set_doctor(&mut self, on: bool) {
        self.mmu.set_doctor(on);
    }

    /// Data-bus accesses (loads, stores, read-modify-write, pushes, pops and
    /// interrupt stack writes) recorded while executing instructions since
    /// the last call, in order, one instruction's worth at a time.
    pub fn take_accesses(&mut self) -> Vec<DataAccess> {
        self.mmu.take_accesses()
    }

    /// Untimed write of one bus address (no peripheral advance). Used by the
    /// debugger to poke memory.
    pub fn poke(&mut self, addr: u16, value: u8) {
        self.mmu.write(addr, value);
    }

    /// Replace the whole register file (debugger).
    pub fn set_registers(&mut self, regs: Registers) {
        self.cpu.regs = regs;
    }

    /// True while the CPU is HALTed (debugger).
    pub fn is_halted(&self) -> bool {
        self.cpu.halted
    }

    /// Debug helper: read one raw OAM byte (`$FE00`–`$FE9F`) without the
    /// OAM-DMA bus mask applied by [`Self::peek`]. Used by the timing-test
    /// tracers.
    pub fn peek_oam(&self, index: usize) -> u8 {
        self.mmu.ppu.oam.get(index).copied().unwrap_or(0xFF)
    }

    /// Temporary debug: raw system counter.
    pub fn debug_timer_counter(&self) -> u16 {
        self.mmu.timer.debug_counter()
    }

    /// Temporary debug: timer reload state.
    pub fn debug_timer_reload(&self) -> (bool, u8) {
        self.mmu.timer.debug_reload()
    }

    /// Whether IME is set (debugger).
    pub fn ime(&self) -> bool {
        self.cpu.ime
    }

    /// Total instructions executed since load (debugger).
    pub fn instruction_count(&self) -> u64 {
        self.instructions
    }

    /// Restart from power-on state using the same ROM image. Clears the
    /// frame/instruction counters; hardware registers return to their
    /// post-boot values.
    pub fn reset(&mut self) {
        if let Ok(fresh) = Emulator::load_with_model(&self.rom, self.model) {
            self.cpu = fresh.cpu;
            self.mmu = fresh.mmu;
            self.frames = 0;
            self.instructions = 0;
        }
    }
}

/// Error restoring a save state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateError {
    /// Buffer ended before all fields were read.
    Truncated,
    /// A field held a value that cannot occur (bad version, wrong ROM, …).
    Corrupt(&'static str),
}

impl core::fmt::Display for StateError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            StateError::Truncated => write!(f, "save state truncated"),
            StateError::Corrupt(why) => write!(f, "save state corrupt: {why}"),
        }
    }
}

impl core::error::Error for StateError {}
