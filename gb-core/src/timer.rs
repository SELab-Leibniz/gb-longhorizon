//! Timer block: DIV (FF04), TIMA (FF05), TMA (FF06), TAC (FF07).
//!
//! Model it around a 16-bit internal counter whose upper byte is DIV.
//! TIMA increments on a falling edge of the counter bit selected by TAC
//! (bits 9/3/5/7 for frequencies 4096/262144/65536/16384 Hz). Writing DIV
//! resets the counter and can therefore cause a spurious TIMA tick — the
//! Mooneye `div_write`, `rapid_toggle`, `tima_reload`, `tima_write_reloading`
//! and `tma_write_reloading` tests exercise exactly these edges.
//!
//! Interrupt raised: Timer (IF bit 2), one M-cycle *after* TIMA overflows.

use crate::emulator::StateError;

/// DIV/TIMA/TMA/TAC.
pub struct Timer {
    // TODO(agent): internal 16-bit counter, TIMA, TMA, TAC, overflow latch.
}

impl Timer {
    /// Post-boot state (DIV counter = 0xABCC on DMG).
    pub fn new() -> Self {
        Self {}
    }

    /// Read FF04–FF07.
    pub fn read(&self, _addr: u16) -> u8 {
        todo!("timer::Timer::read")
    }

    /// Write FF04–FF07.
    pub fn write(&mut self, _addr: u16, _value: u8) {
        todo!("timer::Timer::write")
    }

    /// Advance `cycles` T-cycles. Returns IF bits to raise (bit 2) or 0.
    pub fn tick(&mut self, _cycles: u32) -> u8 {
        todo!("timer::Timer::tick")
    }

    /// Append state to a save-state buffer.
    pub fn save_state(&self, _out: &mut Vec<u8>) {
        todo!("timer::Timer::save_state")
    }

    /// Restore from a save-state buffer.
    pub fn load_state(&mut self, _state: &[u8], _cursor: &mut usize) -> Result<(), StateError> {
        todo!("timer::Timer::load_state")
    }
}

impl Default for Timer {
    fn default() -> Self {
        Self::new()
    }
}
