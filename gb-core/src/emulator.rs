//! The public facade: owns the CPU and the bus, and exposes the handful of
//! operations the CLI, GUI and test harness need.
//!
//! This file is **fully implemented plumbing**. The behaviour lives in the
//! modules it calls into, which are stubs for the agent to implement.

use crate::cpu::{Cpu, Registers};
use crate::joypad::Buttons;
use crate::mmu::Mmu;
use crate::{cartridge::Cartridge, LoadError, CYCLES_PER_FRAME, FRAME_PIXELS};

/// Outcome of executing one instruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepResult {
    /// Instruction executed; carries the number of T-cycles it took (already
    /// applied to the peripherals by the CPU's bus accesses).
    Ran(u32),
    /// The CPU executed `LD B,B` (opcode 0x40, 4 T-cycles, already ticked).
    /// Test ROMs from the Mooneye suite use this as a software breakpoint to
    /// signal completion; the harness inspects the registers when it sees this.
    Breakpoint,
}

/// A complete DMG system.
pub struct Emulator {
    cpu: Cpu,
    mmu: Mmu,
    /// 160×144 shades, one byte per pixel, values 0 (white) ..= 3 (black).
    framebuffer: Vec<u8>,
    frames: u64,
}

impl Emulator {
    /// Build a system around the given ROM image.
    ///
    /// No boot ROM is run: the CPU and I/O registers start in the state the
    /// DMG boot ROM leaves them in (see `DECISIONS.md`, D3).
    pub fn load(rom: &[u8]) -> Result<Self, LoadError> {
        let cart = Cartridge::from_bytes(rom)?;
        Ok(Self {
            cpu: Cpu::post_boot(),
            mmu: Mmu::new(cart),
            framebuffer: vec![0; FRAME_PIXELS],
            frames: 0,
        })
    }

    /// Execute exactly one instruction (plus any interrupt dispatch that
    /// precedes it). The CPU advances the peripherals itself, one M-cycle per
    /// bus access (DECISIONS.md D1); this method does not tick anything.
    pub fn step_instruction(&mut self) -> StepResult {
        self.cpu.step(&mut self.mmu)
    }

    /// Run until one full frame (70 224 T-cycles) has elapsed and copy the
    /// PPU output into [`Self::framebuffer`].
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
        self.framebuffer.copy_from_slice(self.mmu.ppu.framebuffer());
        self.frames += 1;
        hit_breakpoint
    }

    /// The most recently completed frame, 160×144 bytes, row-major, shade 0..=3.
    pub fn framebuffer(&self) -> &[u8] {
        &self.framebuffer
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
}

/// Error restoring a save state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateError {
    /// Buffer ended before all fields were read.
    Truncated,
    /// A field held a value that cannot occur (bad version, wrong ROM, …).
    Corrupt(&'static str),
}

impl std::fmt::Display for StateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StateError::Truncated => write!(f, "save state truncated"),
            StateError::Corrupt(why) => write!(f, "save state corrupt: {why}"),
        }
    }
}

impl std::error::Error for StateError {}
