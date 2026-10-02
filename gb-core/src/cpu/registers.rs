//! Register file for the SM83.
//!
//! The 8-bit registers pair up into 16-bit ones (AF, BC, DE, HL). The low
//! nibble of F is always zero on hardware; keep that invariant in `set_af`.

/// The four condition flags held in register F.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flags {
    /// Zero (bit 7).
    Z = 0x80,
    /// Subtract / BCD negative (bit 6).
    N = 0x40,
    /// Half-carry (bit 5).
    H = 0x20,
    /// Carry (bit 4).
    C = 0x10,
}

/// All CPU registers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Registers {
    /// Accumulator.
    pub a: u8,
    /// Flags; low nibble always 0.
    pub f: u8,
    /// General purpose.
    pub b: u8,
    /// General purpose.
    pub c: u8,
    /// General purpose.
    pub d: u8,
    /// General purpose.
    pub e: u8,
    /// General purpose / address high byte.
    pub h: u8,
    /// General purpose / address low byte.
    pub l: u8,
    /// Stack pointer.
    pub sp: u16,
    /// Program counter.
    pub pc: u16,
}

impl Registers {
    /// State after the DMG boot ROM.
    pub fn post_boot() -> Self {
        Self {
            a: 0x01,
            f: 0xB0,
            b: 0x00,
            c: 0x13,
            d: 0x00,
            e: 0xD8,
            h: 0x01,
            l: 0x4D,
            sp: 0xFFFE,
            pc: 0x0100,
        }
    }

    /// Read a flag.
    pub fn flag(&self, flag: Flags) -> bool {
        self.f & (flag as u8) != 0
    }

    /// Set or clear a flag.
    pub fn set_flag(&mut self, flag: Flags, on: bool) {
        if on {
            self.f |= flag as u8;
        } else {
            self.f &= !(flag as u8);
        }
    }

    /// 16-bit AF.
    pub fn af(&self) -> u16 {
        u16::from_be_bytes([self.a, self.f])
    }
    /// 16-bit BC.
    pub fn bc(&self) -> u16 {
        u16::from_be_bytes([self.b, self.c])
    }
    /// 16-bit DE.
    pub fn de(&self) -> u16 {
        u16::from_be_bytes([self.d, self.e])
    }
    /// 16-bit HL.
    pub fn hl(&self) -> u16 {
        u16::from_be_bytes([self.h, self.l])
    }

    /// Write AF, masking the low nibble of F.
    pub fn set_af(&mut self, v: u16) {
        let [a, f] = v.to_be_bytes();
        self.a = a;
        self.f = f & 0xF0;
    }
    /// Write BC.
    pub fn set_bc(&mut self, v: u16) {
        [self.b, self.c] = v.to_be_bytes();
    }
    /// Write DE.
    pub fn set_de(&mut self, v: u16) {
        [self.d, self.e] = v.to_be_bytes();
    }
    /// Write HL.
    pub fn set_hl(&mut self, v: u16) {
        [self.h, self.l] = v.to_be_bytes();
    }

    /// The Mooneye test suite signals success by loading the Fibonacci
    /// sequence 3,5,8,13,21,34 into B,C,D,E,H,L before executing `LD B,B`.
    /// A failure loads 0x42 into every register instead.
    pub fn is_mooneye_pass(&self) -> bool {
        (self.b, self.c, self.d, self.e, self.h, self.l) == (3, 5, 8, 13, 21, 34)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairs_round_trip() {
        let mut r = Registers::default();
        r.set_bc(0x1234);
        assert_eq!((r.b, r.c), (0x12, 0x34));
        assert_eq!(r.bc(), 0x1234);
        r.set_af(0xABCF);
        assert_eq!(r.f, 0xC0, "low nibble of F must be masked");
    }

    #[test]
    fn mooneye_signature() {
        let mut r = Registers::default();
        assert!(!r.is_mooneye_pass());
        (r.b, r.c, r.d, r.e, r.h, r.l) = (3, 5, 8, 13, 21, 34);
        assert!(r.is_mooneye_pass());
    }
}
