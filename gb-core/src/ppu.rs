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
//! 3. Pixel FIFO with SCX fine-scroll discard, window and sprite fetch
//!    stalls, and mid-scanline register changes taking effect at the right
//!    pixel — required by GEP 1 §4 (Mealybug Tearoom, Mooneye
//!    `acceptance/ppu/`).
//!
//! Interrupts raised: VBlank (IF bit 0) and STAT (IF bit 1, with the
//! "STAT blocking" quirk where multiple STAT conditions only fire once).
//!
//! CGB mode (Pan Docs "Palettes", "Tile Data", "VRAM Banks", "OAM"): two
//! VRAM banks (VBK, FF4F) with per-tile BG attributes in bank 1 (palette,
//! bank, flips, priority); eight BG and eight OBJ palettes of four RGB555
//! colours written through BCPS/BCPD and OCPS/OCPD (FF68–FF6B); the CGB
//! BG-to-OBJ priority rules (LCDC bit 0 becomes "master priority"); OBJ
//! priority by OAM index unless OPRI (FF6C) selects DMG ordering. Output goes
//! to the RGB555 framebuffer; in DMG mode fill it from the shades too.

use crate::emulator::{Model, StateError};
use crate::FRAME_PIXELS;

/// LCD controller + video memory.
pub struct Ppu {
    /// Which console this PPU belongs to.
    pub model: Model,
    /// Video RAM: 8 KiB on DMG, 16 KiB (two banks) on CGB.
    pub vram: Vec<u8>,
    /// 160 bytes sprite attribute memory.
    pub oam: Vec<u8>,
    /// Last completed frame, shades 0..=3 (DMG output).
    framebuffer: Vec<u8>,
    /// Last completed frame, RGB555 (both modes).
    framebuffer_rgb555: Vec<u16>,
    // TODO(agent): LCD registers, mode/dot counters, per-frame state.
}

impl Ppu {
    /// PPU in post-boot state (LCD on, LCDC=0x91, BGP=0xFC).
    pub fn new(model: Model) -> Self {
        Self {
            model,
            vram: vec![0; if model == Model::Cgb { 0x4000 } else { 0x2000 }],
            oam: vec![0; 0xA0],
            framebuffer: vec![0; FRAME_PIXELS],
            framebuffer_rgb555: vec![0x7FFF; FRAME_PIXELS],
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

    /// The last completed frame as shades 0..=3 (DMG mode).
    pub fn framebuffer(&self) -> &[u8] {
        &self.framebuffer
    }

    /// The last completed frame as RGB555 (both modes; DMG shades map to
    /// 0x7FFF, 0x56B5, 0x294A, 0x0000).
    pub fn framebuffer_rgb555(&self) -> &[u16] {
        &self.framebuffer_rgb555
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
        Self::new(Model::Dmg)
    }
}
