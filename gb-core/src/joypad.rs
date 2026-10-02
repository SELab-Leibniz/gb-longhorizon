//! Joypad: the P1/JOYP register at FF00.
//!
//! Bits 4 and 5 (written by the game) select the direction or action group;
//! bits 0–3 read back the selected buttons, **active low** (0 = pressed).
//! A high→low transition on any of bits 0–3 requests the Joypad interrupt
//! (IF bit 4).

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
    // TODO(agent): P1 select bits, previous-state for edge detection.
}

impl Joypad {
    /// Post-boot state (P1 = 0xCF).
    pub fn new() -> Self {
        Self {
            buttons: Buttons::default(),
        }
    }

    /// Replace button state. Returns IF bits to raise (bit 4) or 0.
    pub fn set_buttons(&mut self, buttons: Buttons) -> u8 {
        self.buttons = buttons;
        // TODO(agent): edge detection → Joypad interrupt.
        0
    }

    /// Read FF00.
    pub fn read(&self) -> u8 {
        todo!("joypad::Joypad::read — active-low, group select")
    }

    /// Write FF00 (only bits 4–5 are writable).
    pub fn write(&mut self, _value: u8) {
        todo!("joypad::Joypad::write")
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
