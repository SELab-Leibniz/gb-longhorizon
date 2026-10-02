//! Instruction decode and execute.
//!
//! Suggested shape (not mandated): one `fn execute(cpu, mmu, opcode) -> u32`
//! for the base table and one for the `0xCB` prefix table, each returning
//! the T-cycles consumed. Conditional jumps/calls/returns take different
//! cycle counts when taken vs not taken — `docs/opcodes.json` lists both.
//!
//! Things that commonly go wrong and that the test ROMs catch:
//! * `DAA` (Blargg cpu_instrs 01 – "special")
//! * half-carry on 16-bit `ADD HL,rr` is computed on bit 11, not bit 3
//! * `ADD SP,e8` / `LD HL,SP+e8` set H and C from the *low byte* add, Z=N=0
//! * `POP AF` must clear the low nibble of F
//! * `HALT` with IME=0 and (IE & IF) != 0 triggers the HALT bug

use super::Cpu;
use crate::mmu::Mmu;

/// Execute an already-fetched base-table opcode. Returns T-cycles.
pub fn execute(_cpu: &mut Cpu, _mmu: &mut Mmu, _opcode: u8) -> u32 {
    todo!("cpu::opcodes::execute — 256 base opcodes")
}

/// Execute an already-fetched `CB`-prefixed opcode. Returns T-cycles
/// (excluding the 4 cycles spent fetching the `0xCB` prefix itself).
pub fn execute_cb(_cpu: &mut Cpu, _mmu: &mut Mmu, _opcode: u8) -> u32 {
    todo!("cpu::opcodes::execute_cb — 256 CB-prefixed opcodes")
}
