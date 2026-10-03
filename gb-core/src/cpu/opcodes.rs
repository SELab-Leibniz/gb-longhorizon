//! Instruction decode and execute.
//!
//! Two entry points: [`execute`] for the 256 base opcodes and
//! [`execute_cb`] for the 256 `CB`-prefixed opcodes. The caller (`Cpu::step`)
//! has already fetched the opcode — and, for `CB`, the prefix and the
//! opcode byte — so these functions perform only the remaining M-cycles, in
//! the exact order the hardware performs them (DECISIONS.md D1): every
//! memory access is a `cycle_read` / `cycle_write` and every internal delay
//! an `idle_cycle`. The returned value is the number of real T-cycles those
//! accesses consumed.
//!
//! Conditional jumps/calls/returns take different cycle counts when taken vs
//! not taken; the taken path is the one that performs the extra work.
//!
//! Things that commonly go wrong and that the test ROMs catch:
//! * `DAA` (Blargg cpu_instrs 01 – "special")
//! * half-carry on 16-bit `ADD HL,rr` is computed on bit 11, not bit 3
//! * `ADD SP,e8` / `LD HL,SP+e8` set H and C from the *low byte* add, Z=N=0
//! * `POP AF` must clear the low nibble of F
//! * `HALT` with IME=0 and (IE & IF) != 0 triggers the HALT bug

use super::{Cpu, Flags};
use crate::mmu::Mmu;

/// Execute an already-fetched base-table opcode. Returns real T-cycles.
pub fn execute(cpu: &mut Cpu, mmu: &mut Mmu, opcode: u8) -> u32 {
    let start = mmu.real_cycles();
    dispatch(cpu, mmu, opcode);
    (mmu.real_cycles() - start) as u32
}

/// Execute an already-fetched `CB`-prefixed opcode. Returns real T-cycles
/// (excluding the fetch of the `0xCB` prefix and the opcode byte itself).
#[allow(unused_variables)]
pub fn execute_cb(cpu: &mut Cpu, mmu: &mut Mmu, opcode: u8) -> u32 {
    todo!("execute one CB-prefixed opcode (rotates, shifts, BIT/RES/SET) and return its cycles")
}

#[allow(unused_variables)]
fn dispatch(cpu: &mut Cpu, mmu: &mut Mmu, opcode: u8) {
    todo!("execute one unprefixed opcode, M-cycle by M-cycle through the bus (R-CORE-1, DECISIONS D1)")
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn fetch8(cpu: &mut Cpu, mmu: &mut Mmu) -> u8 {
    let v = mmu.cycle_read_fetch(cpu.regs.pc);
    cpu.regs.pc = cpu.regs.pc.wrapping_add(1);
    v
}

fn fetch16(cpu: &mut Cpu, mmu: &mut Mmu) -> u16 {
    let lo = fetch8(cpu, mmu) as u16;
    let hi = fetch8(cpu, mmu) as u16;
    (hi << 8) | lo
}

fn jump_relative(cpu: &mut Cpu, e: i8) {
    cpu.regs.pc = cpu.regs.pc.wrapping_add(e as i16 as u16);
}

fn cond(cpu: &Cpu, cc: u8) -> bool {
    match cc {
        0 => !cpu.regs.flag(Flags::Z),
        1 => cpu.regs.flag(Flags::Z),
        2 => !cpu.regs.flag(Flags::C),
        _ => cpu.regs.flag(Flags::C),
    }
}

fn get8(cpu: &Cpu, idx: u8) -> u8 {
    match idx & 7 {
        0 => cpu.regs.b,
        1 => cpu.regs.c,
        2 => cpu.regs.d,
        3 => cpu.regs.e,
        4 => cpu.regs.h,
        5 => cpu.regs.l,
        7 => cpu.regs.a,
        _ => 0,
    }
}

fn set8(cpu: &mut Cpu, idx: u8, v: u8) {
    match idx & 7 {
        0 => cpu.regs.b = v,
        1 => cpu.regs.c = v,
        2 => cpu.regs.d = v,
        3 => cpu.regs.e = v,
        4 => cpu.regs.h = v,
        5 => cpu.regs.l = v,
        7 => cpu.regs.a = v,
        _ => {}
    }
}

/// Read an operand that may be `(HL)` (idx 6), performing the memory access.
fn read_operand(cpu: &Cpu, mmu: &mut Mmu, idx: u8) -> u8 {
    if idx & 7 == 6 {
        mmu.cycle_read(cpu.regs.hl())
    } else {
        get8(cpu, idx)
    }
}

/// Write an operand that may be `(HL)` (idx 6), performing the memory access.
fn write_operand(cpu: &mut Cpu, mmu: &mut Mmu, idx: u8, v: u8) {
    if idx & 7 == 6 {
        let hl = cpu.regs.hl();
        mmu.cycle_write(hl, v);
    } else {
        set8(cpu, idx, v);
    }
}

fn get16(cpu: &Cpu, idx: u8) -> u16 {
    match idx & 3 {
        0 => cpu.regs.bc(),
        1 => cpu.regs.de(),
        2 => cpu.regs.hl(),
        _ => cpu.regs.sp,
    }
}

fn set16(cpu: &mut Cpu, idx: u8, v: u16) {
    match idx & 3 {
        0 => cpu.regs.set_bc(v),
        1 => cpu.regs.set_de(v),
        2 => cpu.regs.set_hl(v),
        _ => cpu.regs.sp = v,
    }
}

fn get16_af(cpu: &Cpu, idx: u8) -> u16 {
    match idx & 3 {
        0 => cpu.regs.bc(),
        1 => cpu.regs.de(),
        2 => cpu.regs.hl(),
        _ => cpu.regs.af(),
    }
}

fn set16_af(cpu: &mut Cpu, idx: u8, v: u16) {
    match idx & 3 {
        0 => cpu.regs.set_bc(v),
        1 => cpu.regs.set_de(v),
        2 => cpu.regs.set_hl(v),
        _ => cpu.regs.set_af(v),
    }
}

fn push16(cpu: &mut Cpu, mmu: &mut Mmu, v: u16) {
    mmu.idle_cycle();
    cpu.regs.sp = cpu.regs.sp.wrapping_sub(1);
    mmu.cycle_write(cpu.regs.sp, (v >> 8) as u8);
    cpu.regs.sp = cpu.regs.sp.wrapping_sub(1);
    mmu.cycle_write(cpu.regs.sp, v as u8);
}

fn pop16(cpu: &mut Cpu, mmu: &mut Mmu) -> u16 {
    let lo = mmu.cycle_read(cpu.regs.sp);
    cpu.regs.sp = cpu.regs.sp.wrapping_add(1);
    let hi = mmu.cycle_read(cpu.regs.sp);
    cpu.regs.sp = cpu.regs.sp.wrapping_add(1);
    ((hi as u16) << 8) | lo as u16
}

fn halt(cpu: &mut Cpu, mmu: &Mmu) {
    let pending = (mmu.interrupts.flags & mmu.interrupts.enable & 0x1F) != 0;
    if !cpu.ime && pending {
        cpu.halt_bug = true;
    } else {
        cpu.halted = true;
    }
}

// ---------------------------------------------------------------------------
// ALU
// ---------------------------------------------------------------------------

fn set_flags(cpu: &mut Cpu, z: bool, n: bool, h: bool, c: bool) {
    let mut f = 0u8;
    if z {
        f |= Flags::Z as u8;
    }
    if n {
        f |= Flags::N as u8;
    }
    if h {
        f |= Flags::H as u8;
    }
    if c {
        f |= Flags::C as u8;
    }
    cpu.regs.f = f;
}

fn alu(cpu: &mut Cpu, op: u8, v: u8) {
    match op {
        0 => add_a(cpu, v, false),
        1 => {
            let carry = cpu.regs.flag(Flags::C);
            add_a(cpu, v, carry);
        }
        2 => sub_a(cpu, v, false),
        3 => {
            let carry = cpu.regs.flag(Flags::C);
            sub_a(cpu, v, carry);
        }
        4 => {
            cpu.regs.a &= v;
            set_flags(cpu, cpu.regs.a == 0, false, true, false);
        }
        5 => {
            cpu.regs.a ^= v;
            set_flags(cpu, cpu.regs.a == 0, false, false, false);
        }
        6 => {
            cpu.regs.a |= v;
            set_flags(cpu, cpu.regs.a == 0, false, false, false);
        }
        _ => {
            let a = cpu.regs.a;
            sub_a(cpu, v, false);
            cpu.regs.a = a;
        }
    }
}

fn add_a(cpu: &mut Cpu, v: u8, carry: bool) {
    let a = cpu.regs.a;
    let c = carry as u16;
    let sum = a as u16 + v as u16 + c;
    let h = (a & 0x0F) as u16 + (v & 0x0F) as u16 > 0x0F;
    cpu.regs.a = sum as u8;
    set_flags(cpu, cpu.regs.a == 0, false, h, sum > 0xFF);
}

fn sub_a(cpu: &mut Cpu, v: u8, carry: bool) {
    let a = cpu.regs.a;
    let c = carry as i16;
    let result = a as i16 - v as i16 - c;
    let h = (a & 0x0F) as i16 - (v & 0x0F) as i16 - c < 0;
    cpu.regs.a = result as u8;
    set_flags(cpu, cpu.regs.a == 0, true, h, result < 0);
}

fn inc8(cpu: &mut Cpu, v: u8) -> u8 {
    let r = v.wrapping_add(1);
    cpu.regs.set_flag(Flags::Z, r == 0);
    cpu.regs.set_flag(Flags::N, false);
    cpu.regs.set_flag(Flags::H, (v & 0x0F) == 0x0F);
    r
}

fn dec8(cpu: &mut Cpu, v: u8) -> u8 {
    let r = v.wrapping_sub(1);
    cpu.regs.set_flag(Flags::Z, r == 0);
    cpu.regs.set_flag(Flags::N, true);
    cpu.regs.set_flag(Flags::H, (v & 0x0F) == 0x00);
    r
}

fn add_hl(cpu: &mut Cpu, v: u16) {
    let hl = cpu.regs.hl();
    let sum = hl as u32 + v as u32;
    cpu.regs.set_flag(Flags::N, false);
    cpu.regs
        .set_flag(Flags::H, (hl & 0x0FFF) + (v & 0x0FFF) > 0x0FFF);
    cpu.regs.set_flag(Flags::C, sum > 0xFFFF);
    cpu.regs.set_hl(sum as u16);
}

fn add_sp_e(cpu: &mut Cpu, e: i8) -> u16 {
    let sp = cpu.regs.sp;
    let eu = e as u8;
    let h = (sp & 0x0F) + (eu as u16 & 0x0F) > 0x0F;
    let c = (sp & 0xFF) + eu as u16 > 0xFF;
    cpu.regs.set_flag(Flags::Z, false);
    cpu.regs.set_flag(Flags::N, false);
    cpu.regs.set_flag(Flags::H, h);
    cpu.regs.set_flag(Flags::C, c);
    sp.wrapping_add(e as i16 as u16)
}

fn daa(cpu: &mut Cpu) {
    let a = cpu.regs.a;
    let n = cpu.regs.flag(Flags::N);
    let h = cpu.regs.flag(Flags::H);
    let c = cpu.regs.flag(Flags::C);

    let mut correction = 0u8;
    let mut carry = c;
    if h || (!n && (a & 0x0F) > 0x09) {
        correction |= 0x06;
    }
    if c || (!n && a >= 0x99) {
        correction |= 0x60;
        carry = true;
    }
    cpu.regs.a = if n {
        a.wrapping_sub(correction)
    } else {
        a.wrapping_add(correction)
    };
    cpu.regs.set_flag(Flags::Z, cpu.regs.a == 0);
    cpu.regs.set_flag(Flags::H, false);
    cpu.regs.set_flag(Flags::C, carry);
}

fn rlc(cpu: &mut Cpu, v: u8) -> u8 {
    let c = v >> 7;
    let r = (v << 1) | c;
    set_flags(cpu, r == 0, false, false, c != 0);
    r
}

fn rrc(cpu: &mut Cpu, v: u8) -> u8 {
    let c = v & 1;
    let r = (v >> 1) | (c << 7);
    set_flags(cpu, r == 0, false, false, c != 0);
    r
}

fn rl(cpu: &mut Cpu, v: u8) -> u8 {
    let c = v >> 7;
    let r = (v << 1) | cpu.regs.flag(Flags::C) as u8;
    set_flags(cpu, r == 0, false, false, c != 0);
    r
}

fn rr(cpu: &mut Cpu, v: u8) -> u8 {
    let c = v & 1;
    let r = (v >> 1) | ((cpu.regs.flag(Flags::C) as u8) << 7);
    set_flags(cpu, r == 0, false, false, c != 0);
    r
}

fn sla(cpu: &mut Cpu, v: u8) -> u8 {
    let c = v >> 7;
    let r = v << 1;
    set_flags(cpu, r == 0, false, false, c != 0);
    r
}

fn sra(cpu: &mut Cpu, v: u8) -> u8 {
    let c = v & 1;
    let r = (v >> 1) | (v & 0x80);
    set_flags(cpu, r == 0, false, false, c != 0);
    r
}

fn srl(cpu: &mut Cpu, v: u8) -> u8 {
    let c = v & 1;
    let r = v >> 1;
    set_flags(cpu, r == 0, false, false, c != 0);
    r
}

fn swap(cpu: &mut Cpu, v: u8) -> u8 {
    let r = v.rotate_left(4);
    set_flags(cpu, r == 0, false, false, false);
    r
}
