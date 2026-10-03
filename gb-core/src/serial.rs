//! Serial port: SB (FF01) data and SC (FF02) control.
//!
//! There is no link-cable partner in this emulator. A transfer started with
//! the internal clock (SC = 0x81) completes after 8 bits at 8192 Hz
//! (4096 T-cycles), shifting in 0xFF, clearing SC bit 7 and raising the
//! Serial interrupt (IF bit 3). The byte that was in SB when the transfer
//! started is appended to [`Serial::take_output`].
//!
//! Blargg's test ROMs print "Passed"/"Failed" plus details through this
//! port; the harness greps it.

use crate::emulator::StateError;
use crate::prelude::*;

/// SB/SC + captured output.
pub struct Serial {
    output: Vec<u8>,
    sb: u8,
    sc: u8,
    countdown: u32,
    out_byte: u8,
}

impl Serial {
    /// Post-boot state (SB = 0x00, SC = 0x7E).
    pub fn new() -> Self {
        Self {
            output: Vec::new(),
            sb: 0x00,
            sc: 0x7E,
            countdown: 0,
            out_byte: 0,
        }
    }

    /// Read FF01–FF02.
    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            0xFF01 => self.sb,
            0xFF02 => self.sc | 0x7E,
            _ => 0xFF,
        }
    }

    /// Write FF01–FF02. Writing 0x81 to SC starts a transfer.
    pub fn write(&mut self, addr: u16, value: u8) {
        match addr {
            0xFF01 => self.sb = value,
            0xFF02 => {
                self.sc = value & 0x83;
                if self.sc & 0x80 != 0 && self.sc & 0x01 != 0 {
                    self.begin_transfer();
                }
            }
            _ => {}
        }
    }

    fn begin_transfer(&mut self) {
        self.countdown = 4096;
        self.out_byte = self.sb;
    }

    /// Advance `cycles`. Returns IF bits to raise (bit 3) or 0.
    pub fn tick(&mut self, cycles: u32) -> u8 {
        if self.countdown == 0 {
            return 0;
        }
        if cycles >= self.countdown {
            self.countdown = 0;
            self.push_output(self.out_byte);
            self.sb = 0xFF;
            self.sc &= !0x80;
            0x08
        } else {
            self.countdown -= cycles;
            0
        }
    }

    /// Record a byte the game sent out. Call this when a transfer completes.
    pub fn push_output(&mut self, byte: u8) {
        self.output.push(byte);
    }

    /// Drain captured output.
    pub fn take_output(&mut self) -> Vec<u8> {
        core::mem::take(&mut self.output)
    }

    /// Append state to a save-state buffer.
    pub fn save_state(&self, out: &mut Vec<u8>) {
        out.push(self.sb);
        out.push(self.sc);
        out.extend_from_slice(&self.countdown.to_le_bytes());
        out.push(self.out_byte);
        out.extend_from_slice(&(self.output.len() as u32).to_le_bytes());
        out.extend_from_slice(&self.output);
    }

    /// Restore from a save-state buffer.
    pub fn load_state(&mut self, state: &[u8], cursor: &mut usize) -> Result<(), StateError> {
        let head = state
            .get(*cursor..*cursor + 11)
            .ok_or(StateError::Truncated)?;
        self.sb = head[0];
        self.sc = head[1];
        self.countdown = u32::from_le_bytes([head[2], head[3], head[4], head[5]]);
        self.out_byte = head[6];
        let len = u32::from_le_bytes([head[7], head[8], head[9], head[10]]) as usize;
        *cursor += 11;
        let data = state
            .get(*cursor..*cursor + len)
            .ok_or(StateError::Truncated)?;
        self.output = data.to_vec();
        *cursor += len;
        Ok(())
    }
}

impl Default for Serial {
    fn default() -> Self {
        Self::new()
    }
}
