//! Shared helpers for the `gb-trace` and `gb-server` developer tools.
//!
//! Both binaries are zero-dependency: the JSON codec and the SM83
//! disassembler live here so they can be unit-tested once.

pub mod disasm;
pub mod json;

use gb_core::cpu::Registers;
use gb_core::Model;

/// Parse a `--model` / JSON `model` value. `auto` is handled by the callers
/// that have a ROM to inspect (it is not a real core model).
pub fn parse_model(name: &str) -> Option<Model> {
    match name.to_ascii_lowercase().as_str() {
        "dmg" => Some(Model::Dmg),
        "cgb" => Some(Model::Cgb),
        _ => None,
    }
}

/// Parse an integer that may be decimal or `0x`-prefixed hex. Returns `None`
/// for anything else (including an empty string).
pub fn parse_number(text: &str) -> Option<i64> {
    let t = text.trim();
    if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        i64::from_str_radix(hex, 16).ok()
    } else {
        t.parse::<i64>().ok()
    }
}

/// Render one `gb-trace` line for the register file `regs` and the four bytes
/// `mem` read (side-effect-free) from `PC` … `PC+3`.
pub fn trace_line(regs: &Registers, mem: [u8; 4]) -> String {
    format!(
        "A:{:02X} F:{:02X} B:{:02X} C:{:02X} D:{:02X} E:{:02X} \
         H:{:02X} L:{:02X} SP:{:04X} PC:{:04X} PCMEM:{:02X},{:02X},{:02X},{:02X}",
        regs.a,
        regs.f,
        regs.b,
        regs.c,
        regs.d,
        regs.e,
        regs.h,
        regs.l,
        regs.sp,
        regs.pc,
        mem[0],
        mem[1],
        mem[2],
        mem[3]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_numbers() {
        assert_eq!(parse_number("255"), Some(255));
        assert_eq!(parse_number("0xFF"), Some(255));
        assert_eq!(parse_number("0x10"), Some(16));
        assert_eq!(parse_number("nope"), None);
        assert_eq!(parse_number(""), None);
    }

    #[test]
    fn models() {
        assert_eq!(parse_model("dmg"), Some(Model::Dmg));
        assert_eq!(parse_model("CGB"), Some(Model::Cgb));
        assert_eq!(parse_model("auto"), None);
    }
}
