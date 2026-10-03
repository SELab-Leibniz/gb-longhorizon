//! Pixel Processing Unit.
//!
//! Owns VRAM (8 KiB) and OAM (160 bytes), the LCD registers
//! (LCDC, STAT, SCY, SCX, LY, LYC, BGP, OBP0, OBP1, WY, WX), and produces
//! a 160×144 framebuffer of 2-bit shades once per frame.
//!
//! Rendering is scanline-based: each visible line is drawn at the start of
//! mode 3 using the register state at that moment. Mode timing follows the
//! hardware (OAM scan 80 dots, draw 172 dots, HBlank, VBlank from LY=144).

use crate::emulator::{Model, StateError};
use crate::prelude::*;
use crate::FRAME_PIXELS;

const SCREEN_WIDTH: usize = 160;
const SCREEN_HEIGHT: usize = 144;

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

    lcdc: u8,
    stat: u8,
    scy: u8,
    scx: u8,
    ly: u8,
    lyc: u8,
    bgp: u8,
    obp0: u8,
    obp1: u8,
    wy: u8,
    wx: u8,

    dot: u16,
    mode: u8,
    window_line: u8,
    stat_line: bool,
    frame_done: bool,

    // CGB
    vbk: u8,
    bcps: u8,
    ocps: u8,
    bg_palette: [u8; 64],
    obj_palette: [u8; 64],
    opri: u8,
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
            lcdc: 0x91,
            stat: 0x85,
            scy: 0,
            scx: 0,
            ly: 0,
            lyc: 0,
            bgp: 0xFC,
            obp0: 0xFF,
            obp1: 0xFF,
            wy: 0,
            wx: 0,
            dot: 0,
            mode: 0,
            window_line: 0,
            stat_line: false,
            frame_done: false,
            vbk: 0,
            bcps: 0,
            ocps: 0,
            bg_palette: [0xFF; 64],
            obj_palette: [0xFF; 64],
            opri: 0,
        }
    }

    fn is_cgb(&self) -> bool {
        self.model == Model::Cgb
    }

    /// Read a PPU-owned address (VRAM, OAM, or an LCD register in FF40–FF4B).
    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0x9FFF => {
                if self.is_cgb() {
                    let bank = (self.vbk & 1) as usize;
                    self.vram[bank * 0x2000 + (addr - 0x8000) as usize]
                } else {
                    self.vram[(addr - 0x8000) as usize]
                }
            }
            0xFE00..=0xFE9F => self.oam[(addr - 0xFE00) as usize],
            0xFF40 => self.lcdc,
            0xFF41 => self.stat | 0x80,
            0xFF42 => self.scy,
            0xFF43 => self.scx,
            0xFF44 => self.ly,
            0xFF45 => self.lyc,
            0xFF47 => self.bgp,
            0xFF48 => self.obp0,
            0xFF49 => self.obp1,
            0xFF4A => self.wy,
            0xFF4B => self.wx,
            0xFF4F => self.vbk | 0xFE,
            0xFF68 => self.bcps | 0x40,
            0xFF69 => self.bg_palette[(self.bcps & 0x3F) as usize],
            0xFF6A => self.ocps | 0x40,
            0xFF6B => self.obj_palette[(self.ocps & 0x3F) as usize],
            0xFF6C => self.opri | 0xFE,
            _ => 0xFF,
        }
    }

    /// Write a PPU-owned address.
    pub fn write(&mut self, addr: u16, value: u8) {
        match addr {
            0x8000..=0x9FFF => {
                if self.is_cgb() {
                    let bank = (self.vbk & 1) as usize;
                    self.vram[bank * 0x2000 + (addr - 0x8000) as usize] = value;
                } else {
                    self.vram[(addr - 0x8000) as usize] = value;
                }
            }
            0xFE00..=0xFE9F => self.oam[(addr - 0xFE00) as usize] = value,
            0xFF40 => {
                let was_on = self.lcdc & 0x80 != 0;
                self.lcdc = value;
                let is_on = value & 0x80 != 0;
                if !was_on && is_on {
                    self.dot = 0;
                    self.ly = 0;
                    self.mode = 2;
                    self.window_line = 0;
                } else if was_on && !is_on {
                    self.dot = 0;
                    self.ly = 0;
                    self.mode = 0;
                    self.blan_k();
                }
            }
            0xFF41 => self.stat = (self.stat & 0x07) | (value & 0x78),
            0xFF42 => self.scy = value,
            0xFF43 => self.scx = value,
            0xFF44 => {}
            0xFF45 => self.lyc = value,
            0xFF47 => self.bgp = value,
            0xFF48 => self.obp0 = value,
            0xFF49 => self.obp1 = value,
            0xFF4A => self.wy = value,
            0xFF4B => self.wx = value,
            0xFF4F => self.vbk = value & 1,
            0xFF68 => self.bcps = value & 0xBF,
            0xFF69 => {
                let i = (self.bcps & 0x3F) as usize;
                self.bg_palette[i] = value;
                if self.bcps & 0x80 != 0 {
                    self.bcps = 0x80 | ((self.bcps.wrapping_add(1)) & 0x1F);
                }
            }
            0xFF6A => self.ocps = value & 0xBF,
            0xFF6B => {
                let i = (self.ocps & 0x3F) as usize;
                self.obj_palette[i] = value;
                if self.ocps & 0x80 != 0 {
                    self.ocps = 0x80 | ((self.ocps.wrapping_add(1)) & 0x3F);
                }
            }
            0xFF6C => self.opri = value & 1,
            _ => {}
        }
    }

    /// Write one byte straight into OAM (used by OAM DMA).
    pub fn write_oam(&mut self, index: usize, value: u8) {
        if index < self.oam.len() {
            self.oam[index] = value;
        }
    }

    /// Write one byte into a VRAM bank at a 0x8000-relative offset (HDMA).
    pub fn write_vram_bank(&mut self, bank: u16, offset: usize, value: u8) {
        let base = if self.is_cgb() {
            (bank as usize & 1) * 0x2000
        } else {
            0
        };
        if let Some(slot) = self.vram.get_mut(base + (offset & 0x1FFF)) {
            *slot = value;
        }
    }

    /// Advance `cycles` dots. Returns the IF bits to raise
    /// (bit 0 = VBlank, bit 1 = STAT), or 0.
    #[allow(unused_variables)]
    pub fn tick(&mut self, cycles: u32) -> u8 {
        todo!("advance the PPU by `cycles` dots: mode 2/3/0/1 state machine, LY/LYC, STAT and VBlank interrupts (R-CORE-4)")
    }

    fn update_stat(&mut self) -> u8 {
        let mode = self.mode;
        let lyc_eq = self.ly == self.lyc;
        self.stat = (self.stat & !0x07) | (mode & 3) | if lyc_eq { 0x04 } else { 0 };
        let cond = (mode == 0 && self.stat & 0x08 != 0)
            || (mode == 1 && self.stat & 0x10 != 0)
            || (mode == 2 && self.stat & 0x20 != 0)
            || (lyc_eq && self.stat & 0x40 != 0);
        let mut irq = 0;
        if cond && !self.stat_line {
            irq |= 0x02;
        }
        self.stat_line = cond;
        irq
    }

    fn blan_k(&mut self) {
        for px in self.framebuffer.iter_mut() {
            *px = 0;
        }
        for px in self.framebuffer_rgb555.iter_mut() {
            *px = 0x7FFF;
        }
    }

    fn tile_data_addr(&self, index: u8) -> usize {
        if self.lcdc & 0x10 != 0 {
            0x8000 + (index as usize) * 16
        } else {
            (0x9000 + (index as i8 as i16 as isize) * 16) as usize
        }
    }

    fn render_scanline(&mut self) {
        todo!("draw the current line into the framebuffer(s): background, window, sprites (R-CORE-4, R-CGB-1)")
    }

    #[allow(unused_variables)]
    fn bg_pixel(&self, x: usize) -> (u8, bool) {
        todo!("background colour/priority for pixel x on the current line")
    }

    #[allow(unused_variables)]
    fn window_pixel(&self, x: usize, win_x_start: usize) -> (u8, bool) {
        todo!("window colour/priority for pixel x on the current line")
    }

    #[allow(unused_variables)]
    fn render_sprites(&mut self, ly: usize, bg_color: &[u8; SCREEN_WIDTH]) {
        todo!("draw up to 10 sprites on line `ly` with DMG/CGB priority rules")
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

    /// Whether a frame has been completed since the flag was last cleared.
    pub fn take_frame_done(&mut self) -> bool {
        core::mem::take(&mut self.frame_done)
    }

    /// Append PPU state to a save-state buffer.
    #[allow(unused_variables)]
    #[allow(clippy::ptr_arg)]
    pub fn save_state(&self, out: &mut Vec<u8>) {
        todo!("append the PPU's state to `out` (Emulator::save_state, R-CORE-6)")
    }

    /// Restore from a save-state buffer.
    #[allow(unused_variables)]
    pub fn load_state(&mut self, state: &[u8], cursor: &mut usize) -> Result<(), StateError> {
        todo!("restore the PPU's state written by save_state (R-CORE-6)")
    }
}

fn dmg_rgb(shade: u8) -> u16 {
    match shade & 3 {
        0 => 0x7FFF,
        1 => 0x56B5,
        2 => 0x294A,
        _ => 0x0000,
    }
}

fn cgb_color(palette: &[u8; 64], pal: u8, color_id: u8) -> u16 {
    let idx = (pal as usize & 7) * 8 + (color_id as usize & 3) * 2;
    let lo = palette[idx];
    let hi = palette[idx + 1];
    let r = (lo & 0x1F) as u16;
    let g = (((lo >> 5) & 0x07) as u16) | (((hi & 0x03) as u16) << 3);
    let b = ((hi >> 2) & 0x1F) as u16;
    r | (g << 5) | (b << 10)
}

impl Default for Ppu {
    fn default() -> Self {
        Self::new(Model::Dmg)
    }
}
