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
pub fn execute_cb(cpu: &mut Cpu, mmu: &mut Mmu, opcode: u8) -> u32 {
    let start = mmu.real_cycles();
    let idx = opcode & 7;
    let group = opcode >> 6;
    let bit = (opcode >> 3) & 7;

    match group {
        0 => {
            // Rotate / shift / swap.
            let v = read_operand(cpu, mmu, idx);
            let r = match bit {
                0 => rlc(cpu, v),
                1 => rrc(cpu, v),
                2 => rl(cpu, v),
                3 => rr(cpu, v),
                4 => sla(cpu, v),
                5 => sra(cpu, v),
                6 => swap(cpu, v),
                _ => srl(cpu, v),
            };
            write_operand(cpu, mmu, idx, r);
        }
        1 => {
            // BIT b, r
            let v = read_operand(cpu, mmu, idx);
            cpu.regs.set_flag(Flags::Z, (v >> bit) & 1 == 0);
            cpu.regs.set_flag(Flags::N, false);
            cpu.regs.set_flag(Flags::H, true);
        }
        2 => {
            // RES b, r
            let v = read_operand(cpu, mmu, idx);
            write_operand(cpu, mmu, idx, v & !(1 << bit));
        }
        _ => {
            // SET b, r
            let v = read_operand(cpu, mmu, idx);
            write_operand(cpu, mmu, idx, v | (1 << bit));
        }
    }
    (mmu.real_cycles() - start) as u32
}

fn dispatch(cpu: &mut Cpu, mmu: &mut Mmu, opcode: u8) {
    match opcode {
        // ---- 0x00–0x3F: miscellaneous, 8-bit and 16-bit loads -------------
        0x00 => {}                  // NOP
        0x10 => mmu.switch_speed(), // STOP (speed switch on CGB)
        0x76 => halt(cpu, mmu),     // HALT

        0x01 | 0x11 | 0x21 | 0x31 => {
            let rr = (opcode >> 4) & 3;
            let nn = fetch16(cpu, mmu);
            set16(cpu, rr, nn);
        }
        0x02 => mmu.cycle_write(cpu.regs.bc(), cpu.regs.a),
        0x0A => cpu.regs.a = mmu.cycle_read(cpu.regs.bc()),
        0x12 => mmu.cycle_write(cpu.regs.de(), cpu.regs.a),
        0x1A => cpu.regs.a = mmu.cycle_read(cpu.regs.de()),

        0x22 => {
            let hl = cpu.regs.hl();
            mmu.cycle_write(hl, cpu.regs.a);
            cpu.regs.set_hl(hl.wrapping_add(1));
        }
        0x2A => {
            let hl = cpu.regs.hl();
            cpu.regs.a = mmu.cycle_read(hl);
            cpu.regs.set_hl(hl.wrapping_add(1));
        }
        0x32 => {
            let hl = cpu.regs.hl();
            mmu.cycle_write(hl, cpu.regs.a);
            cpu.regs.set_hl(hl.wrapping_sub(1));
        }
        0x3A => {
            let hl = cpu.regs.hl();
            cpu.regs.a = mmu.cycle_read(hl);
            cpu.regs.set_hl(hl.wrapping_sub(1));
        }

        0x03 | 0x13 | 0x23 | 0x33 => {
            let rr = (opcode >> 4) & 3;
            mmu.idle_cycle();
            set16(cpu, rr, get16(cpu, rr).wrapping_add(1));
        }
        0x0B | 0x1B | 0x2B | 0x3B => {
            let rr = (opcode >> 4) & 3;
            mmu.idle_cycle();
            set16(cpu, rr, get16(cpu, rr).wrapping_sub(1));
        }

        0x04 | 0x0C | 0x14 | 0x1C | 0x24 | 0x2C | 0x34 | 0x3C => {
            let idx = (opcode >> 3) & 7;
            let v = inc8(cpu, read_operand(cpu, mmu, idx));
            write_operand(cpu, mmu, idx, v);
        }
        0x05 | 0x0D | 0x15 | 0x1D | 0x25 | 0x2D | 0x35 | 0x3D => {
            let idx = (opcode >> 3) & 7;
            let v = dec8(cpu, read_operand(cpu, mmu, idx));
            write_operand(cpu, mmu, idx, v);
        }

        0x06 | 0x0E | 0x16 | 0x1E | 0x26 | 0x2E | 0x36 | 0x3E => {
            let idx = (opcode >> 3) & 7;
            let n = fetch8(cpu, mmu);
            write_operand(cpu, mmu, idx, n);
        }

        0x07 => {
            let a = cpu.regs.a;
            let c = a >> 7;
            cpu.regs.a = (a << 1) | c;
            set_flags(cpu, false, false, false, c != 0);
        }
        0x0F => {
            let a = cpu.regs.a;
            let c = a & 1;
            cpu.regs.a = (a >> 1) | (c << 7);
            set_flags(cpu, false, false, false, c != 0);
        }
        0x17 => {
            let a = cpu.regs.a;
            let c = a >> 7;
            cpu.regs.a = (a << 1) | cpu.regs.flag(Flags::C) as u8;
            set_flags(cpu, false, false, false, c != 0);
        }
        0x1F => {
            let a = cpu.regs.a;
            let c = a & 1;
            cpu.regs.a = (a >> 1) | ((cpu.regs.flag(Flags::C) as u8) << 7);
            set_flags(cpu, false, false, false, c != 0);
        }

        0x08 => {
            let nn = fetch16(cpu, mmu);
            let sp = cpu.regs.sp;
            mmu.cycle_write(nn, sp as u8);
            mmu.cycle_write(nn.wrapping_add(1), (sp >> 8) as u8);
        }
        0x09 | 0x19 | 0x29 | 0x39 => {
            let rr = (opcode >> 4) & 3;
            let v = get16(cpu, rr);
            mmu.idle_cycle();
            add_hl(cpu, v);
        }

        0x18 => {
            let e = fetch8(cpu, mmu) as i8;
            mmu.idle_cycle();
            jump_relative(cpu, e);
        }
        0x20 | 0x28 | 0x30 | 0x38 => {
            let cc = (opcode >> 3) & 3;
            let e = fetch8(cpu, mmu) as i8;
            if cond(cpu, cc) {
                mmu.idle_cycle();
                jump_relative(cpu, e);
            }
        }

        0x27 => daa(cpu),
        0x2F => {
            cpu.regs.a = !cpu.regs.a;
            cpu.regs.set_flag(Flags::N, true);
            cpu.regs.set_flag(Flags::H, true);
        }
        0x37 => set_flags(cpu, cpu.regs.flag(Flags::Z), false, false, true),
        0x3F => {
            let c = !cpu.regs.flag(Flags::C);
            set_flags(cpu, cpu.regs.flag(Flags::Z), false, false, c);
        }

        // ---- 0x40–0x7F: LD r,r' (0x76 HALT handled above) ----------------
        0x40..=0x7F => {
            let dst = (opcode >> 3) & 7;
            let src = opcode & 7;
            let v = read_operand(cpu, mmu, src);
            write_operand(cpu, mmu, dst, v);
        }

        // ---- 0x80–0xBF: ALU A,r ------------------------------------------
        0x80..=0xBF => {
            let v = read_operand(cpu, mmu, opcode & 7);
            alu(cpu, (opcode >> 3) & 7, v);
        }

        // ---- 0xC0–0xFF: branches, stack, immediate ALU -------------------
        0xC0 | 0xC8 | 0xD0 | 0xD8 => {
            let cc = (opcode >> 3) & 3;
            if cond(cpu, cc) {
                mmu.idle_cycle();
                let addr = pop16(cpu, mmu);
                mmu.idle_cycle();
                cpu.regs.pc = addr;
            } else {
                mmu.idle_cycle();
            }
        }
        0xC1 | 0xD1 | 0xE1 | 0xF1 => {
            let rr = (opcode >> 4) & 3;
            let v = pop16(cpu, mmu);
            set16_af(cpu, rr, v);
        }
        0xC2 | 0xCA | 0xD2 | 0xDA => {
            let cc = (opcode >> 3) & 3;
            let nn = fetch16(cpu, mmu);
            if cond(cpu, cc) {
                mmu.idle_cycle();
                cpu.regs.pc = nn;
            }
        }
        0xC3 => {
            let nn = fetch16(cpu, mmu);
            mmu.idle_cycle();
            cpu.regs.pc = nn;
        }
        0xC4 | 0xCC | 0xD4 | 0xDC => {
            let cc = (opcode >> 3) & 3;
            let nn = fetch16(cpu, mmu);
            if cond(cpu, cc) {
                let ret = cpu.regs.pc;
                push16(cpu, mmu, ret);
                cpu.regs.pc = nn;
            }
        }
        0xC5 | 0xD5 | 0xE5 | 0xF5 => {
            let rr = (opcode >> 4) & 3;
            let v = get16_af(cpu, rr);
            push16(cpu, mmu, v);
        }
        0xC6 | 0xCE | 0xD6 | 0xDE | 0xE6 | 0xEE | 0xF6 | 0xFE => {
            let v = fetch8(cpu, mmu);
            alu(cpu, (opcode >> 3) & 7, v);
        }
        0xC7 | 0xCF | 0xD7 | 0xDF | 0xE7 | 0xEF | 0xF7 | 0xFF => {
            let target = (opcode & 0x38) as u16;
            let ret = cpu.regs.pc;
            push16(cpu, mmu, ret);
            cpu.regs.pc = target;
        }
        0xC9 => {
            let addr = pop16(cpu, mmu);
            mmu.idle_cycle();
            cpu.regs.pc = addr;
        }
        0xD9 => {
            let addr = pop16(cpu, mmu);
            mmu.idle_cycle();
            cpu.regs.pc = addr;
            cpu.ime = true;
            cpu.ime_pending = false;
        }
        0xCD => {
            let nn = fetch16(cpu, mmu);
            let ret = cpu.regs.pc;
            push16(cpu, mmu, ret);
            cpu.regs.pc = nn;
        }

        0xE0 => {
            let n = fetch8(cpu, mmu);
            let a = cpu.regs.a;
            mmu.cycle_write(0xFF00 | n as u16, a);
        }
        0xF0 => {
            let n = fetch8(cpu, mmu);
            cpu.regs.a = mmu.cycle_read(0xFF00 | n as u16);
        }
        0xE2 => mmu.cycle_write(0xFF00 | cpu.regs.c as u16, cpu.regs.a),
        0xF2 => cpu.regs.a = mmu.cycle_read(0xFF00 | cpu.regs.c as u16),

        0xE8 => {
            let e = fetch8(cpu, mmu) as i8;
            mmu.idle_cycle();
            mmu.idle_cycle();
            cpu.regs.sp = add_sp_e(cpu, e);
        }
        0xF8 => {
            let e = fetch8(cpu, mmu) as i8;
            mmu.idle_cycle();
            let v = add_sp_e(cpu, e);
            cpu.regs.set_hl(v);
        }
        0xE9 => cpu.regs.pc = cpu.regs.hl(),
        0xF9 => {
            let hl = cpu.regs.hl();
            mmu.idle_cycle();
            cpu.regs.sp = hl;
        }

        0xEA => {
            let nn = fetch16(cpu, mmu);
            let a = cpu.regs.a;
            mmu.cycle_write(nn, a);
        }
        0xFA => {
            let nn = fetch16(cpu, mmu);
            cpu.regs.a = mmu.cycle_read(nn);
        }

        0xF3 => {
            cpu.ime = false;
            cpu.ime_pending = false;
        }
        0xFB => cpu.ime_pending = true,

        // Illegal opcodes: behave as a 1-M-cycle no-op rather than locking up
        // (keeps the emulator panic-free on arbitrary input).
        _ => {}
    }
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
    let h = (a & 0x0F) as u16 + (v & 0x0F) as u16 + c > 0x0F;
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
    if c || (!n && a > 0x99) {
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
