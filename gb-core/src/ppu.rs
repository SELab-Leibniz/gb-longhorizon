//! Pixel Processing Unit.
//!
//! Owns VRAM (8 KiB) and OAM (160 bytes), the LCD registers
//! (LCDC, STAT, SCY, SCX, LY, LYC, BGP, OBP0, OBP1, WY, WX), and produces
//! a 160×144 framebuffer of 2-bit shades once per frame.
//!
//! Accuracy ladder the test ROMs climb:
//! 1. Scanline renderer with correct mode timing (OAM scan 80 dots, draw
//!    172–289 dots, HBlank, VBlank from LY=144) — enough for most games and
//!    for the STAT/LY Mooneye tests.
//! 2. `dmg-acid2` checks: window enable/disable mid-frame, 8×16 sprites,
//!    sprite priority and X-ordering, BG/OBJ palette handling, LCDC bit 0
//!    behaviour. Pass this and the framebuffer hash will match reference.
//! 3. Pixel-FIFO with SCX fine scroll penalties and sprite fetch stalls —
//!    only needed for the strictest Mooneye PPU timing tests.
//!
//! Interrupts raised: VBlank (IF bit 0) and STAT (IF bit 1, with the
//! "STAT blocking" quirk where multiple STAT conditions only fire once).

use crate::emulator::StateError;
use crate::FRAME_PIXELS;

/// LCD controller + video memory.
pub struct Ppu {
    /// 8 KiB video RAM.
    pub vram: Vec<u8>,
    /// 160 bytes sprite attribute memory.
    pub oam: Vec<u8>,
    /// Back buffer written during the frame, swapped at VBlank.
    framebuffer: Vec<u8>,
    // TODO(agent): LCD registers, mode/dot counters, per-frame state.
}

impl Ppu {
    /// PPU in post-boot state (LCD on, LCDC=0x91, BGP=0xFC).
    pub fn new() -> Self {
        Self {
            vram: vec![0; 0x2000],
            oam: vec![0; 0xA0],
            framebuffer: vec![0; FRAME_PIXELS],
        }
    }

    /// Read a PPU-owned address (VRAM, OAM, or an LCD register in FF40–FF4B).
    pub fn read(&self, _addr: u16) -> u8 {
        todo!("ppu::Ppu::read")
    }

    /// Write a PPU-owned address.
    pub fn write(&mut self, _addr: u16, _value: u8) {
        todo!("ppu::Ppu::write")
    }

    /// Advance `cycles` dots. Returns the IF bits to raise
    /// (bit 0 = VBlank, bit 1 = STAT), or 0.
    pub fn tick(&mut self, _cycles: u32) -> u8 {
        todo!("ppu::Ppu::tick — mode state machine + scanline rendering")
    }

    /// The last completed frame.
    pub fn framebuffer(&self) -> &[u8] {
        &self.framebuffer
    }

    /// Append PPU state to a save-state buffer.
    pub fn save_state(&self, _out: &mut Vec<u8>) {
        todo!("ppu::Ppu::save_state")
    }

    /// Restore from a save-state buffer.
    pub fn load_state(&mut self, _state: &[u8], _cursor: &mut usize) -> Result<(), StateError> {
        todo!("ppu::Ppu::load_state")
    }
}

impl Default for Ppu {
    fn default() -> Self {
        Self::new()
    }
}
