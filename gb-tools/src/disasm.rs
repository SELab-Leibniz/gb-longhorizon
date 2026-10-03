//! SM83 disassembler (upper-case, `$` hex, per GEP Appendix B).
//!
//! The decode follows the operand tables in `docs/opcodes.json`: upper-case
//! mnemonics, operands separated by `, `, memory operands in `[...]`, numbers
//! as `$` hex, relative jumps shown as their absolute destination, `LDH`
//! addresses in full and signed operands as signed decimal.
//!
//! ```text
//! NOP            JP $0150            LD A, [HL+]      LD [$C000], A
//! LDH [$FF44], A   LD A, [$FF00+C]   JR NZ, $0203     BIT 7, H
//! RST $38        ADD SP, -2          LD HL, SP+5
//! ```

/// 8-bit register names by their 3-bit encoding (index 6 is memory at `HL`).
const R8: [&str; 8] = ["B", "C", "D", "E", "H", "L", "[HL]", "A"];
/// 16-bit register names by their 2-bit encoding.
const R16: [&str; 4] = ["BC", "DE", "HL", "SP"];
/// 16-bit stack register names, encoding 3 is `AF`.
const R16STK: [&str; 4] = ["BC", "DE", "HL", "AF"];
/// ALU operation names by their 3-bit encoding.
const ALU: [&str; 8] = ["ADD", "ADC", "SUB", "SBC", "AND", "XOR", "OR", "CP"];
/// Conditional names by their 2-bit encoding.
const COND: [&str; 4] = ["NZ", "Z", "NC", "C"];
/// CB-prefixed rotate/shift names by their 3-bit encoding.
const ROT: [&str; 8] = ["RLC", "RRC", "RL", "RR", "SLA", "SRA", "SWAP", "SRL"];

/// Format a byte as `$XX`.
fn hex8(v: u8) -> String {
    format!("${v:02X}")
}

/// Format a word as `$XXXX`.
fn hex16(v: u16) -> String {
    format!("${v:04X}")
}

fn imm8<F: Fn(u16) -> u8>(read: &F, pc: u16, off: u16) -> u8 {
    read(pc.wrapping_add(off))
}

fn imm16<F: Fn(u16) -> u8>(read: &F, pc: u16, off: u16) -> u16 {
    let lo = read(pc.wrapping_add(off)) as u16;
    let hi = read(pc.wrapping_add(off + 1)) as u16;
    lo | (hi << 8)
}

/// The absolute destination of a relative jump at `pc` with offset `e`.
fn jr_dest(pc: u16, e: u8) -> u16 {
    pc.wrapping_add(2).wrapping_add(e as i8 as u16)
}

/// Disassemble the instruction at `pc`, reading bytes through `read`.
/// Returns the instruction length in bytes and its textual form.
#[allow(unused_variables)]
pub fn disassemble<F: Fn(u16) -> u8>(read: F, pc: u16) -> (u8, String) {
    todo!("decode the instruction at `pc` into (length, text) in the GEP 1 Appendix B syntax")
}

/// Instruction length in bytes for a base opcode.
fn length(op: u8) -> u8 {
    match op {
        0x01 | 0x11 | 0x21 | 0x31 | 0x08 | 0xC2 | 0xC3 | 0xC4 | 0xCA | 0xCC | 0xCD | 0xD2
        | 0xD4 | 0xDA | 0xDC | 0xEA | 0xFA => 3,
        0x06 | 0x0E | 0x10 | 0x16 | 0x18 | 0x1E | 0x20 | 0x26 | 0x28 | 0x2E | 0x30 | 0x36
        | 0x38 | 0x3E | 0xC6 | 0xCE | 0xD6 | 0xDE | 0xE0 | 0xE6 | 0xE8 | 0xEE | 0xF0 | 0xF6
        | 0xF8 | 0xFE => 2,
        _ => 1,
    }
}

fn bracket16(v: u16) -> String {
    format!("[{}]", hex16(v))
}

/// A signed byte as signed decimal (e.g. `-2`, `5`).
fn signed(v: u8) -> String {
    format!("{}", v as i8)
}

/// The `SP+e8` tail: `+5` for positive, `-2` for negative.
fn sp_offset(v: u8) -> String {
    let s = v as i8;
    if s < 0 {
        format!("{s}")
    } else {
        format!("+{s}")
    }
}

/// Disassemble a `CB`-prefixed opcode.
fn disassemble_cb(cb: u8) -> String {
    let reg = R8[(cb & 7) as usize];
    match cb >> 6 {
        0 => format!("{} {}", ROT[((cb >> 3) & 7) as usize], reg),
        1 => format!("BIT {}, {}", (cb >> 3) & 7, reg),
        2 => format!("RES {}, {}", (cb >> 3) & 7, reg),
        _ => format!("SET {}, {}", (cb >> 3) & 7, reg),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Disassemble the bytes at the front of `bytes`, padding with `0xFF`.
    fn text(bytes: &[u8]) -> String {
        let read = |addr: u16| {
            let i = addr as usize;
            if i < bytes.len() {
                bytes[i]
            } else {
                0xFF
            }
        };
        disassemble(read, 0).1
    }

    fn len(bytes: &[u8]) -> u8 {
        let read = |addr: u16| {
            let i = addr as usize;
            if i < bytes.len() {
                bytes[i]
            } else {
                0xFF
            }
        };
        disassemble(read, 0).0
    }

    #[test]
    fn documented_examples() {
        assert_eq!(text(&[0x00]), "NOP");
        assert_eq!(text(&[0xC3, 0x50, 0x01]), "JP $0150");
        assert_eq!(text(&[0x2A]), "LD A, [HL+]");
        assert_eq!(text(&[0xEA, 0x00, 0xC0]), "LD [$C000], A");
        assert_eq!(text(&[0xE0, 0x44]), "LDH [$FF44], A");
        assert_eq!(text(&[0xF2]), "LD A, [$FF00+C]");
        assert_eq!(text(&[0x20, 0x00]), "JR NZ, $0002");
        assert_eq!(text(&[0xCB, 0x7C]), "BIT 7, H");
        assert_eq!(text(&[0xFF]), "RST $38");
        assert_eq!(text(&[0xE8, 0xFE]), "ADD SP, -2");
        assert_eq!(text(&[0xF8, 0x05]), "LD HL, SP+5");
    }

    #[test]
    fn lengths() {
        assert_eq!(len(&[0xC3, 0x00, 0x01]), 3);
        assert_eq!(len(&[0x21, 0x00, 0x40]), 3);
        assert_eq!(len(&[0xCB, 0x00]), 2);
        assert_eq!(len(&[0x00]), 1);
        assert_eq!(len(&[0x36, 0x00]), 2);
    }

    #[test]
    fn relative_jump_backwards() {
        // At pc = 0 -> destination = 0 + 2 + (-3) = 0xFF; printed from 0.
        assert_eq!(text(&[0x18, 0xFD]), "JR $FFFF");
    }

    #[test]
    fn alu_and_ld_forms() {
        assert_eq!(text(&[0x80]), "ADD A, B");
        assert_eq!(text(&[0x47]), "LD B, A");
        assert_eq!(text(&[0x36, 0xAB]), "LD [HL], $AB");
        assert_eq!(text(&[0x34]), "INC [HL]");
        assert_eq!(text(&[0xE9]), "JP HL");
        assert_eq!(text(&[0xF9]), "LD SP, HL");
    }
}
