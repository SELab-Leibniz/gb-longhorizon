//! Audio Processing Unit: registers FF10–FF26 and wave RAM FF30–FF3F.
//!
//! Four channels (pulse ×2, wave, noise) mixed into stereo samples. GEP 1
//! R-CORE-5 requires all of it (measured by Blargg `dmg_sound` and
//! `cgb_sound`); host audio output is not required. Registers must behave
//! from the start (Blargg's `cpu_instrs` touches NR52) and games must not
//! crash because the APU is incomplete, so a sensible first version is:
//! store writes, return them with the read-back masks from Pan Docs "Audio
//! Registers", emit silence.

use crate::emulator::StateError;

/// Sound hardware.
pub struct Apu {
    // TODO(agent): channel state, frame sequencer, sample accumulator.
}

impl Apu {
    /// Post-boot state (NR52 = 0xF1, all channels as the boot ROM leaves them).
    pub fn new() -> Self {
        Self {}
    }

    /// Read FF10–FF3F.
    pub fn read(&self, _addr: u16) -> u8 {
        todo!("apu::Apu::read")
    }

    /// Write FF10–FF3F.
    pub fn write(&mut self, _addr: u16, _value: u8) {
        todo!("apu::Apu::write")
    }

    /// Advance `cycles` T-cycles.
    pub fn tick(&mut self, _cycles: u32) {
        todo!("apu::Apu::tick")
    }

    /// Drain mixed stereo samples (interleaved L,R) produced since the last
    /// call. Sample rate is up to the implementation; document it on the
    /// method when chosen. May be empty until Tier 4.
    pub fn take_samples(&mut self) -> Vec<i16> {
        Vec::new()
    }

    /// Append state to a save-state buffer.
    pub fn save_state(&self, _out: &mut Vec<u8>) {
        todo!("apu::Apu::save_state")
    }

    /// Restore from a save-state buffer.
    pub fn load_state(&mut self, _state: &[u8], _cursor: &mut usize) -> Result<(), StateError> {
        todo!("apu::Apu::load_state")
    }
}

impl Default for Apu {
    fn default() -> Self {
        Self::new()
    }
}
