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

/// SB/SC + captured output.
pub struct Serial {
    output: Vec<u8>,
    // TODO(agent): SB, SC, transfer countdown.
}

impl Serial {
    /// Post-boot state (SB = 0x00, SC = 0x7E).
    pub fn new() -> Self {
        Self { output: Vec::new() }
    }

    /// Read FF01–FF02.
    pub fn read(&self, _addr: u16) -> u8 {
        todo!("serial::Serial::read")
    }

    /// Write FF01–FF02. Writing 0x81 to SC starts a transfer.
    pub fn write(&mut self, _addr: u16, _value: u8) {
        todo!("serial::Serial::write")
    }

    /// Advance `cycles`. Returns IF bits to raise (bit 3) or 0.
    pub fn tick(&mut self, _cycles: u32) -> u8 {
        todo!("serial::Serial::tick")
    }

    /// Record a byte the game sent out. Call this when a transfer completes.
    pub fn push_output(&mut self, byte: u8) {
        self.output.push(byte);
    }

    /// Drain captured output.
    pub fn take_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.output)
    }
}

impl Default for Serial {
    fn default() -> Self {
        Self::new()
    }
}
