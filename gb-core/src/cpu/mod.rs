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

use crate::emulator::{Model, StateError, StepResult};
use crate::interrupts::Interrupt;
use crate::mmu::Mmu;
use crate::prelude::*;

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
    /// Register values after the boot ROM has finished.
    /// DMG: AF=01B0 BC=0013 DE=00D8 HL=014D SP=FFFE PC=0100.
    /// CGB (CGB-mode cartridge): AF=1180 BC=0000 DE=FF56 HL=000D SP=FFFE PC=0100.
    pub fn post_boot(model: Model) -> Self {
        let mut regs = Registers::post_boot();
        if model == Model::Cgb {
            regs.set_af(0x1180);
            regs.set_bc(0x0000);
            regs.set_de(0xFF56);
            regs.set_hl(0x000D);
        }
        Self {
            regs,
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
    pub fn step(&mut self, mmu: &mut Mmu) -> StepResult {
        mmu.take_accesses();
        let start = mmu.real_cycles();
        // Whether an `EI` was executed one instruction ago: its delayed enable
        // takes effect after the instruction executed below.
        let ime_armed = self.ime_pending;

        // An interrupt is taken when IME is set and IF & IE request one,
        // before the next instruction is fetched.
        if self.ime {
            if let Some(irq) = mmu.interrupts.pending() {
                self.halted = false;
                service_interrupt(self, mmu, irq);
                return StepResult::Ran(elapsed(start, mmu));
            }
        }

        // HALTed: burn one M-cycle per step until an interrupt is requested,
        // then fall back into normal execution (servicing it if IME is set).
        if self.halted {
            mmu.idle_cycle();
            if mmu.interrupts.flags & mmu.interrupts.enable & 0x1F != 0 {
                self.halted = false;
            }
            return StepResult::Ran(elapsed(start, mmu));
        }

        let pc = self.regs.pc;
        let opcode = mmu.cycle_read_fetch(pc);
        if self.halt_bug {
            // The HALT bug suppresses the PC increment for exactly one fetch,
            // so the byte just read is re-executed.
            self.halt_bug = false;
        } else {
            self.regs.pc = pc.wrapping_add(1);
        }

        if opcode == 0x40 {
            // `LD B,B` is the magic breakpoint the test ROMs pass through.
            // The fetch above already ticked the bus.
            return StepResult::Breakpoint;
        }

        if opcode == 0xCB {
            let cb = mmu.cycle_read_fetch(self.regs.pc);
            self.regs.pc = self.regs.pc.wrapping_add(1);
            opcodes::execute_cb(self, mmu, cb);
        } else {
            opcodes::execute(self, mmu, opcode);
        }

        // An `EI` one instruction ago takes effect now, unless the instruction
        // just executed was a `DI` that cancelled it.
        if ime_armed && self.ime_pending {
            self.ime = true;
            self.ime_pending = false;
        }

        StepResult::Ran(elapsed(start, mmu))
    }

    /// Copy of the register file.
    pub fn registers(&self) -> Registers {
        self.regs
    }

    /// Append CPU state to a save-state buffer.
    #[allow(unused_variables)]
    #[allow(clippy::ptr_arg)]
    pub fn save_state(&self, out: &mut Vec<u8>) {
        todo!("append the CPU's registers and flags to `out` (R-CORE-6)")
    }

    /// Restore CPU state from a save-state buffer, advancing `cursor`.
    #[allow(unused_variables)]
    pub fn load_state(&mut self, state: &[u8], cursor: &mut usize) -> Result<(), StateError> {
        todo!("restore the CPU's state written by save_state (R-CORE-6)")
    }
}

/// Real T-cycles ticked on the bus since `start`.
fn elapsed(start: u64, mmu: &Mmu) -> u32 {
    (mmu.real_cycles() - start) as u32
}

/// Take an interrupt: 2 wait M-cycles, push PC (high then low), then one
/// M-cycle to load the vector. IME is cleared so a nested interrupt is not
/// taken until another `EI`/`RETI`.
fn service_interrupt(cpu: &mut Cpu, mmu: &mut Mmu, irq: Interrupt) {
    mmu.idle_cycle();
    mmu.idle_cycle();
    let pc = cpu.regs.pc;
    cpu.regs.sp = cpu.regs.sp.wrapping_sub(1);
    mmu.cycle_write(cpu.regs.sp, (pc >> 8) as u8);
    cpu.regs.sp = cpu.regs.sp.wrapping_sub(1);
    mmu.cycle_write(cpu.regs.sp, pc as u8);
    mmu.idle_cycle();
    cpu.regs.pc = irq.vector();
    cpu.ime = false;
    mmu.interrupts.acknowledge(irq);
}
