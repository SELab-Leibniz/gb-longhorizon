//! Timer block: DIV (FF04), TIMA (FF05), TMA (FF06), TAC (FF07).
//!
//! Model it around a 16-bit internal counter whose upper byte is DIV.
//! TIMA increments on a falling edge of the counter bit selected by TAC
//! (bits 9/3/5/7 for frequencies 4096/262144/65536/16384 Hz). Writing DIV
//! resets the counter and can therefore cause a spurious TIMA tick — the
//! Mooneye `div_write`, `rapid_toggle`, `tima_reload`, `tima_write_reloading`
//! and `tma_write_reloading` tests exercise exactly these edges.
//!
//! Overflow takes two M-cycles (cycle A then cycle B): TIMA reads $00
//! throughout cycle A, and TMA is copied into TIMA (raising IF bit 2) one
//! M-cycle — four T-cycles — after the overflow. A TIMA write during cycle A
//! cancels the overflow entirely; a TIMA write during cycle B is dropped, and
//! a TMA write during cycle B is copied into TIMA immediately. See Pan Docs
//! "Timer obscure behaviour".

use crate::emulator::StateError;
use crate::prelude::*;

/// Number of T-cycles TIMA stays $00 after overflowing (cycle A).
const CYCLE_A_TICKS: u8 = 4;
/// Number of T-cycles of the reload cycle (cycle B), during which TIMA
/// writes are ignored and TMA writes reach TIMA.
const CYCLE_B_TICKS: u8 = 4;

/// DIV/TIMA/TMA/TAC.
pub struct Timer {
    counter: u16,
    tima: u8,
    tma: u8,
    tac: u8,
    /// True from the overflow until cycle B ends.
    reload_pending: bool,
    /// While `reload_pending` and this is non-zero, TIMA is $00 and we are in
    /// cycle A (the reload is this many T-cycles away).
    reload_delay: u8,
    /// While `reload_pending` and `reload_delay` is zero, TMA has been
    /// loaded and we are in cycle B; this counts the remaining cycle-B ticks.
    reload_b: u8,
}

impl Timer {
    /// Post-boot state (DIV counter = 0xABCC on DMG).
    pub fn new() -> Self {
        Self {
            counter: 0xABCC,
            tima: 0,
            tma: 0,
            tac: 0xF8,
            reload_pending: false,
            reload_delay: 0,
            reload_b: 0,
        }
    }

    /// Read FF04–FF07.
    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            0xFF04 => (self.counter >> 8) as u8,
            0xFF05 => self.tima,
            0xFF06 => self.tma,
            0xFF07 => self.tac | 0xF8,
            _ => 0xFF,
        }
    }

    /// Write FF04–FF07.
    pub fn write(&mut self, addr: u16, value: u8) {
        match addr {
            0xFF04 => {
                // Reset the counter; if the selected bit falls, TIMA ticks.
                self.set_counter(0);
            }
            0xFF05 => {
                if self.reload_pending {
                    if self.reload_delay > 0 {
                        // Cycle A: the overflow is cancelled — the written
                        // value stays and no interrupt is raised.
                        self.reload_pending = false;
                        self.reload_delay = 0;
                        self.reload_b = 0;
                        self.tima = value;
                    }
                    // Cycle B: the write is overwritten by the reload, so it
                    // is dropped here.
                } else {
                    self.tima = value;
                }
            }
            0xFF06 => {
                self.tma = value;
                if self.reload_pending && self.reload_delay == 0 {
                    // Cycle B: TIMA constantly copies its input, so a TMA
                    // write reaches TIMA on the same cycle.
                    self.tima = value;
                }
            }
            0xFF07 => {
                let old = self.tac;
                self.tac = value;
                // Changing TAC can disable the timer or switch the selected
                // bit, either of which is a falling edge.
                if let Some(old_bit) = Self::selected_bit(old) {
                    if (self.counter >> old_bit) & 1 == 1 {
                        match Self::selected_bit(self.tac) {
                            Some(new_bit) if new_bit == old_bit => {}
                            _ => self.increment_tima(),
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// Advance `cycles` T-cycles. Returns IF bits to raise (bit 2) or 0.
    pub fn tick(&mut self, cycles: u32) -> u8 {
        let mut irq = 0;
        for _ in 0..cycles {
            if self.tick_t() {
                irq |= 0x04;
            }
        }
        irq
    }

    /// Temporary debug: expose the raw system counter.
    pub fn debug_counter(&self) -> u16 {
        self.counter
    }

    /// Temporary debug: expose reload state (pending, cycle-A countdown,
    /// cycle-B countdown).
    pub fn debug_reload(&self) -> (bool, u8) {
        (self.reload_pending, self.reload_delay)
    }

    fn selected_bit(tac: u8) -> Option<u8> {
        if tac & 0x04 == 0 {
            None
        } else {
            Some(match tac & 0x03 {
                0 => 9,
                1 => 3,
                2 => 5,
                _ => 7,
            })
        }
    }

    fn set_counter(&mut self, value: u16) {
        if let Some(bit) = Self::selected_bit(self.tac) {
            let old = (self.counter >> bit) & 1;
            let new = (value >> bit) & 1;
            if old == 1 && new == 0 {
                self.increment_tima();
            }
        }
        self.counter = value;
    }

    fn increment_tima(&mut self) {
        if self.reload_pending {
            return;
        }
        if self.tima == 0xFF {
            // Cycle A: TIMA is $00 until the reload one M-cycle later.
            self.tima = 0x00;
            self.reload_pending = true;
            self.reload_delay = CYCLE_A_TICKS;
            self.reload_b = 0;
        } else {
            self.tima = self.tima.wrapping_add(1);
        }
    }

    fn tick_t(&mut self) -> bool {
        let mut irq = false;
        // The overflow/reload state machine runs before the counter edge, so
        // the reload lands exactly CYCLE_A_TICKS T-cycles after the overflow.
        if self.reload_pending {
            if self.reload_delay > 0 {
                self.reload_delay -= 1;
                if self.reload_delay == 0 {
                    self.tima = self.tma;
                    self.reload_b = CYCLE_B_TICKS;
                    irq = true;
                }
            } else {
                self.reload_b -= 1;
                if self.reload_b == 0 {
                    self.reload_pending = false;
                }
            }
        }
        if let Some(b) = Self::selected_bit(self.tac) {
            let old = self.counter;
            self.counter = self.counter.wrapping_add(1);
            if (old >> b) & 1 == 1 && (self.counter >> b) & 1 == 0 {
                self.increment_tima();
            }
        } else {
            self.counter = self.counter.wrapping_add(1);
        }
        irq
    }

    /// Append state to a save-state buffer.
    pub fn save_state(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.counter.to_le_bytes());
        out.push(self.tima);
        out.push(self.tma);
        out.push(self.tac);
        out.push(self.reload_pending as u8);
        out.push(self.reload_delay);
        out.push(self.reload_b);
    }

    /// Restore from a save-state buffer.
    pub fn load_state(&mut self, state: &[u8], cursor: &mut usize) -> Result<(), StateError> {
        let b = state
            .get(*cursor..*cursor + 8)
            .ok_or(StateError::Truncated)?;
        self.counter = u16::from_le_bytes([b[0], b[1]]);
        self.tima = b[2];
        self.tma = b[3];
        self.tac = b[4];
        self.reload_pending = b[5] != 0;
        self.reload_delay = b[6];
        self.reload_b = b[7];
        *cursor += 8;
        Ok(())
    }
}

impl Default for Timer {
    fn default() -> Self {
        Self::new()
    }
}
