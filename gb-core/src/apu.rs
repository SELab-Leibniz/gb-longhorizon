//! Audio Processing Unit: registers FF10–FF26 and wave RAM FF30–FF3F.
//!
//! Four channels (pulse ×2, wave, noise) mixed into stereo samples. GEP 1
//! R-CORE-5 requires all of it (measured by Blargg `dmg_sound` and
//! `cgb_sound`); host audio output is not required. The register file and
//! the frame sequencer live here; the channel generators are added on top.

use crate::emulator::StateError;
use crate::prelude::*;

/// Read-back masks for FF10–FF2F (bits that always read as 1).
const READ_MASK: [u8; 0x20] = [
    0x80, 0x3F, 0x00, 0xFF, 0xBF, 0xFF, 0x3F, 0x00, 0xFF, 0xBF, 0x7F, 0xFF, 0x9F, 0xFF, 0xBF, 0xFF,
    0xFF, 0x00, 0x00, 0xBF, 0x00, 0x00, 0x70, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
];

/// Sound hardware.
pub struct Apu {
    regs: [u8; 0x20],
    wave: [u8; 16],
    power: bool,
    frame_counter: u32,
    frame_step: u8,
}

impl Apu {
    /// Post-boot state (NR52 = 0xF1, all channels as the boot ROM leaves them).
    pub fn new() -> Self {
        let mut regs = [0u8; 0x20];
        // The boot ROM sets NR52 = 0xF1: power on, all channels on.
        regs[0x16] = 0xF1;
        Self {
            regs,
            wave: [0u8; 16],
            power: true,
            frame_counter: 0,
            frame_step: 0,
        }
    }

    /// Read FF10–FF3F.
    pub fn read(&self, addr: u16) -> u8 {
        if (0xFF30..=0xFF3F).contains(&addr) {
            return if self.power {
                self.wave[(addr - 0xFF30) as usize]
            } else {
                0xFF
            };
        }
        if addr == 0xFF26 {
            let mut v = 0x70;
            if self.power {
                v |= 0x80;
            }
            // Channel-active bits 0-3 are cleared until generators exist.
            return v;
        }
        if !self.power {
            return 0x00;
        }
        self.regs[(addr - 0xFF10) as usize] | READ_MASK[(addr - 0xFF10) as usize]
    }

    /// Write FF10–FF3F.
    pub fn write(&mut self, addr: u16, value: u8) {
        if (0xFF30..=0xFF3F).contains(&addr) {
            if self.power {
                self.wave[(addr - 0xFF30) as usize] = value;
            }
            return;
        }
        if addr == 0xFF26 {
            let on = value & 0x80 != 0;
            if !on && self.power {
                self.power = false;
                self.regs = [0u8; 0x20];
                self.wave = [0u8; 16];
            } else if on && !self.power {
                self.power = true;
                self.frame_counter = 0;
                self.frame_step = 0;
            }
            return;
        }
        if !self.power {
            return;
        }
        self.regs[(addr - 0xFF10) as usize] = value;
    }

    /// Advance `cycles` T-cycles.
    pub fn tick(&mut self, mut cycles: u32) {
        if !self.power {
            return;
        }
        // Frame sequencer: 512 Hz (8192 T-cycles per step, 8 steps).
        while cycles > 0 {
            let step = (8192 - self.frame_counter).min(cycles);
            self.frame_counter += step;
            cycles -= step;
            if self.frame_counter >= 8192 {
                self.frame_counter = 0;
                self.frame_step = (self.frame_step + 1) & 7;
            }
        }
    }

    /// Drain mixed stereo samples (interleaved L,R) produced since the last
    /// call. Sample rate is up to the implementation; document it on the
    /// method when chosen. May be empty until Tier 4.
    pub fn take_samples(&mut self) -> Vec<i16> {
        Vec::new()
    }

    /// Append state to a save-state buffer.
    pub fn save_state(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.regs);
        out.extend_from_slice(&self.wave);
        out.push(self.power as u8);
        out.extend_from_slice(&self.frame_counter.to_le_bytes());
        out.push(self.frame_step);
    }

    /// Restore from a save-state buffer.
    pub fn load_state(&mut self, state: &[u8], cursor: &mut usize) -> Result<(), StateError> {
        let b = state
            .get(*cursor..*cursor + 0x20 + 16 + 1 + 4 + 1)
            .ok_or(StateError::Truncated)?;
        self.regs.copy_from_slice(&b[..0x20]);
        self.wave.copy_from_slice(&b[0x20..0x30]);
        self.power = b[0x30] != 0;
        self.frame_counter = u32::from_le_bytes([b[0x31], b[0x32], b[0x33], b[0x34]]);
        self.frame_step = b[0x35];
        *cursor += 0x36;
        Ok(())
    }
}

impl Default for Apu {
    fn default() -> Self {
        Self::new()
    }
}
