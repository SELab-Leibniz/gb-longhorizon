//! Joypad: the P1/JOYP register at FF00.
//!
//! Bits 4 and 5 (written by the game) select the direction or action group;
//! bits 0–3 read back the selected buttons, **active low** (0 = pressed).
//! A high→low transition on any of bits 0–3 requests the Joypad interrupt
//! (IF bit 4).

use crate::prelude::*;

/// Button state. `true` = pressed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Buttons {
    /// D-pad right.
    pub right: bool,
    /// D-pad left.
    pub left: bool,
    /// D-pad up.
    pub up: bool,
    /// D-pad down.
    pub down: bool,
    /// A.
    pub a: bool,
    /// B.
    pub b: bool,
    /// Select.
    pub select: bool,
    /// Start.
    pub start: bool,
}

impl Buttons {
    /// Parse a comma-separated list such as `A,START,LEFT` (case-insensitive).
    /// Unknown names are an error so input scripts fail loudly.
    pub fn parse_list(s: &str) -> Result<Self, String> {
        let mut b = Buttons::default();
        for name in s.split(',').map(str::trim).filter(|n| !n.is_empty()) {
            match name.to_ascii_uppercase().as_str() {
                "RIGHT" => b.right = true,
                "LEFT" => b.left = true,
                "UP" => b.up = true,
                "DOWN" => b.down = true,
                "A" => b.a = true,
                "B" => b.b = true,
                "SELECT" => b.select = true,
                "START" => b.start = true,
                other => return Err(format!("unknown button `{other}`")),
            }
        }
        Ok(b)
    }
}

/// P1 register + current buttons.
pub struct Joypad {
    /// Currently held buttons.
    pub buttons: Buttons,
    select: u8,
    /// Set when a high→low transition occurred; the MMU moves it into IF.
    pub pending_irq: bool,
}

impl Joypad {
    /// Post-boot state (P1 = 0xCF).
    pub fn new() -> Self {
        Self {
            buttons: Buttons::default(),
            select: 0x30,
            pending_irq: false,
        }
    }

    /// Replace button state. Returns IF bits to raise (bit 4) or 0.
    pub fn set_buttons(&mut self, buttons: Buttons) -> u8 {
        let before = self.direction_bits();
        self.buttons = buttons;
        let after = self.direction_bits();
        if before & !after != 0 {
            self.pending_irq = true;
            0x10
        } else {
            0
        }
    }

    /// Read FF00.
    pub fn read(&self) -> u8 {
        let inputs = self.direction_bits();
        let mut v = 0xC0 | (self.select & 0x30) | 0x0F;
        v = (v & 0xF0) | inputs;
        v
    }

    /// Write FF00 (only bits 4–5 are writable).
    pub fn write(&mut self, value: u8) {
        self.select = value & 0x30;
    }

    /// Low nibble (active low) of whichever groups are currently selected.
    fn direction_bits(&self) -> u8 {
        let mut inputs = 0x0F;
        if self.select & 0x10 == 0 {
            if self.buttons.right {
                inputs &= !0x01;
            }
            if self.buttons.left {
                inputs &= !0x02;
            }
            if self.buttons.up {
                inputs &= !0x04;
            }
            if self.buttons.down {
                inputs &= !0x08;
            }
        }
        if self.select & 0x20 == 0 {
            if self.buttons.a {
                inputs &= !0x01;
            }
            if self.buttons.b {
                inputs &= !0x02;
            }
            if self.buttons.select {
                inputs &= !0x04;
            }
            if self.buttons.start {
                inputs &= !0x08;
            }
        }
        inputs
    }

    /// Take and clear the pending interrupt flag.
    pub fn take_irq(&mut self) -> u8 {
        if self.pending_irq {
            self.pending_irq = false;
            0x10
        } else {
            0
        }
    }

    /// Append state to a save-state buffer.
    pub fn save_state(&self, out: &mut Vec<u8>) {
        out.push(self.select);
        out.push(self.pending_irq as u8);
        out.push(self.direction_bits());
    }

    /// Restore from a save-state buffer.
    pub fn load_state(
        &mut self,
        state: &[u8],
        cursor: &mut usize,
    ) -> Result<(), crate::emulator::StateError> {
        let b = state
            .get(*cursor..*cursor + 3)
            .ok_or(crate::emulator::StateError::Truncated)?;
        self.select = b[0];
        self.pending_irq = b[1] != 0;
        *cursor += 3;
        Ok(())
    }
}

impl Default for Joypad {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_list_accepts_case_insensitive_names() {
        let b = Buttons::parse_list("a, Start ,LEFT").unwrap();
        assert!(b.a && b.start && b.left && !b.b);
        assert!(Buttons::parse_list("X").is_err());
        assert_eq!(Buttons::parse_list("").unwrap(), Buttons::default());
    }
}
