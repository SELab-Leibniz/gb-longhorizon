//! Interrupt flags: IF (FF0F) and IE (FFFF).
//!
//! Bit layout (both registers): 0 VBlank, 1 LCD STAT, 2 Timer, 3 Serial,
//! 4 Joypad. Vectors are 0x40, 0x48, 0x50, 0x58, 0x60 respectively; lower
//! bit = higher priority. Unused upper bits of IF read as 1.
//!
//! Dispatch itself (push PC, clear IME and the IF bit, jump) lives in the
//! CPU; this module only holds the flags and answers "what is pending?".

/// The five interrupt sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Interrupt {
    /// Bit 0, vector 0x40.
    VBlank = 0,
    /// Bit 1, vector 0x48.
    Stat = 1,
    /// Bit 2, vector 0x50.
    Timer = 2,
    /// Bit 3, vector 0x58.
    Serial = 3,
    /// Bit 4, vector 0x60.
    Joypad = 4,
}

impl Interrupt {
    /// Address the CPU jumps to when servicing this interrupt.
    pub fn vector(self) -> u16 {
        0x40 + 8 * (self as u16)
    }
}

/// IF and IE.
#[derive(Debug, Clone, Copy, Default)]
pub struct Interrupts {
    /// Interrupt Flag (requested).
    pub flags: u8,
    /// Interrupt Enable.
    pub enable: u8,
}

impl Interrupts {
    /// Post-boot: IF = 0xE1, IE = 0x00.
    pub fn new() -> Self {
        Self {
            flags: 0xE1,
            enable: 0x00,
        }
    }

    /// Request an interrupt (set its IF bit).
    pub fn request(&mut self, irq: Interrupt) {
        self.flags |= 1 << (irq as u8);
    }

    /// OR a raw IF bitmask into the flags (what peripherals' `tick` return).
    pub fn request_bits(&mut self, bits: u8) {
        self.flags |= bits & 0x1F;
    }

    /// Highest-priority interrupt that is both requested and enabled, if any.
    pub fn pending(&self) -> Option<Interrupt> {
        let both = self.flags & self.enable & 0x1F;
        if both == 0 {
            return None;
        }
        Some(match 7 - both.leading_zeros() {
            0 => Interrupt::VBlank,
            1 => Interrupt::Stat,
            2 => Interrupt::Timer,
            3 => Interrupt::Serial,
            _ => Interrupt::Joypad,
        })
    }

    /// Acknowledge (clear) an interrupt's IF bit.
    pub fn acknowledge(&mut self, irq: Interrupt) {
        self.flags &= !(1 << (irq as u8));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vectors() {
        assert_eq!(Interrupt::VBlank.vector(), 0x40);
        assert_eq!(Interrupt::Joypad.vector(), 0x60);
    }
}
