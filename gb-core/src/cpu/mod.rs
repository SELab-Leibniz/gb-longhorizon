//! Sharp SM83 CPU core.
//!
//! Split into:
//! * [`registers`] — the register file and flag helpers (small, start here)
//! * [`opcodes`] — decode + execute for the 256 base and 256 `CB`-prefixed
//!   opcodes
//! * this file — the fetch/execute loop, interrupt dispatch, HALT/STOP,
//!   and the IME/EI delay
//!
//! Reference: Pan Docs "CPU Instruction Set" and "Interrupts". The opcode
//! table in `docs/opcodes.json` gives lengths, cycle counts and flag effects
//! for every instruction.

pub mod opcodes;
pub mod registers;

pub use registers::{Flags, Registers};

use crate::emulator::{StateError, StepResult};
use crate::mmu::Mmu;

/// CPU state: registers plus the control flags that are not memory-mapped.
#[derive(Debug, Clone)]
pub struct Cpu {
    /// Register file.
    pub regs: Registers,
    /// Interrupt Master Enable.
    pub ime: bool,
    /// `EI` enables IME one instruction late; this tracks the pending enable.
    pub ime_pending: bool,
    /// CPU is in HALT until an interrupt is requested.
    pub halted: bool,
    /// The "HALT bug": HALT with IME=0 and a pending interrupt fails to
    /// increment PC on the next fetch. Blargg's `halt_bug.gb` tests this.
    pub halt_bug: bool,
}

impl Cpu {
    /// Register values after the DMG boot ROM has finished
    /// (AF=01B0 BC=0013 DE=00D8 HL=014D SP=FFFE PC=0100).
    pub fn post_boot() -> Self {
        Self {
            regs: Registers::post_boot(),
            ime: false,
            ime_pending: false,
            halted: false,
            halt_bug: false,
        }
    }

    /// Fetch, decode and execute one instruction, servicing interrupts first
    /// if IME is set and one is pending.
    ///
    /// Every memory access (including opcode and operand fetches and stack
    /// pushes/pops) must go through `mmu.cycle_read` / `mmu.cycle_write`, and
    /// every internal delay cycle through `mmu.idle_cycle`, in the order the
    /// hardware performs them — that is what advances the rest of the system
    /// (DECISIONS.md D1). Return the total T-cycles consumed.
    ///
    /// Must return [`StepResult::Breakpoint`] when the executed opcode is
    /// `0x40` (`LD B,B`) — the test harness depends on it.
    pub fn step(&mut self, _mmu: &mut Mmu) -> StepResult {
        todo!("cpu::Cpu::step — see docs/opcodes.json and Pan Docs 'CPU Instruction Set'")
    }

    /// Copy of the register file.
    pub fn registers(&self) -> Registers {
        self.regs
    }

    /// Append CPU state to a save-state buffer.
    pub fn save_state(&self, _out: &mut Vec<u8>) {
        todo!("cpu::Cpu::save_state")
    }

    /// Restore CPU state from a save-state buffer, advancing `cursor`.
    pub fn load_state(&mut self, _state: &[u8], _cursor: &mut usize) -> Result<(), StateError> {
        todo!("cpu::Cpu::load_state")
    }
}
