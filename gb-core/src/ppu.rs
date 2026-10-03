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
                    self.bcps = 0x80 | ((self.bcps.wrapping_add(1)) & 0x3F);
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
    pub fn tick(&mut self, cycles: u32) -> u8 {
        if self.lcdc & 0x80 == 0 {
            return 0;
        }
        let mut irq = 0;
        for _ in 0..cycles {
            self.dot += 1;
            match self.mode {
                2 => {
                    if self.dot >= 80 {
                        self.mode = 3;
                        let ly = self.ly as usize;
                        if ly < SCREEN_HEIGHT {
                            self.render_scanline();
                        }
                    }
                }
                3 => {
                    if self.dot >= 252 {
                        self.mode = 0;
                    }
                }
                0 => {
                    if self.dot >= 456 {
                        self.dot = 0;
                        self.ly += 1;
                        if self.ly >= 144 {
                            self.mode = 1;
                            irq |= 0x01;
                            if self.lcdc & 0x10 != 0 {
                                irq |= 0x10; // mode-1 STAT visible in STAT reg
                            }
                        } else {
                            self.mode = 2;
                        }
                    }
                }
                _ => {
                    if self.dot >= 456 {
                        self.dot = 0;
                        if self.ly >= 153 {
                            self.ly = 0;
                            self.mode = 2;
                            self.window_line = 0;
                            self.frame_done = true;
                        } else {
                            self.ly += 1;
                        }
                    }
                }
            }
            irq |= self.update_stat();
        }
        irq
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
        let ly = self.ly as usize;
        let bg_enable = self.lcdc & 0x01 != 0 || self.is_cgb();
        let win_enable = self.lcdc & 0x20 != 0 && bg_enable;
        let wy = self.wy as i32;
        let wx = self.wx as i32;
        let win_active = win_enable && ly >= wy as usize && wx <= 166;
        let win_x_start = (wx - 7).max(0) as usize;

        let mut bg_color = [0u8; SCREEN_WIDTH];
        for (x, bg) in bg_color.iter_mut().enumerate() {
            let (color_id, _prio) = if win_active && x >= win_x_start {
                self.window_pixel(x, win_x_start)
            } else if bg_enable {
                self.bg_pixel(x)
            } else {
                (0, false)
            };
            let shade = (self.bgp >> (color_id * 2)) & 0x03;
            self.framebuffer[ly * SCREEN_WIDTH + x] = shade;
            self.framebuffer_rgb555[ly * SCREEN_WIDTH + x] = dmg_rgb(shade);
            *bg = color_id;
        }

        self.render_sprites(ly, &bg_color);

        if win_active && win_x_start < SCREEN_WIDTH {
            self.window_line = self.window_line.wrapping_add(1);
        }
    }

    fn bg_pixel(&self, x: usize) -> (u8, bool) {
        let map_x = (x as u8).wrapping_add(self.scx);
        let map_y = self.ly.wrapping_add(self.scy);
        let tile_x = (map_x / 8) as usize;
        let tile_y = (map_y / 8) as usize;
        let map_base = if self.lcdc & 0x08 != 0 {
            0x9C00
        } else {
            0x9800
        };
        let map_addr = map_base + tile_y * 32 + tile_x;
        let (tile_index, attr) = if self.is_cgb() {
            let index = self.vram[map_addr - 0x8000];
            let attr = self.vram[0x2000 + (map_addr - 0x8000)];
            (index, attr)
        } else {
            (self.vram[map_addr - 0x8000], 0u8)
        };
        let bank = if self.is_cgb() && attr & 0x08 != 0 {
            1
        } else {
            0
        };
        let mut row = (map_y % 8) as usize;
        if self.is_cgb() && attr & 0x40 != 0 {
            row = 7 - row;
        }
        let mut col = (map_x % 8) as usize;
        if self.is_cgb() && attr & 0x20 != 0 {
            col = 7 - col;
        }
        let addr = self.tile_data_addr(tile_index) - 0x8000 + bank * 0x2000 + row * 2;
        let lo = self.vram[addr];
        let hi = self.vram[addr + 1];
        let bit = 7 - col;
        let color_id = (((hi >> bit) & 1) << 1) | ((lo >> bit) & 1);
        let prio = attr & 0x80 != 0;
        (color_id, prio)
    }

    fn window_pixel(&self, x: usize, win_x_start: usize) -> (u8, bool) {
        let map_x = (x - win_x_start) as u16;
        let map_y = self.window_line as u16;
        let tile_x = (map_x / 8) as usize;
        let tile_y = (map_y / 8) as usize;
        let map_base = if self.lcdc & 0x40 != 0 {
            0x9C00
        } else {
            0x9800
        };
        let map_addr = map_base + tile_y * 32 + tile_x;
        let (tile_index, attr) = if self.is_cgb() {
            let index = self.vram[map_addr - 0x8000];
            let attr = self.vram[0x2000 + (map_addr - 0x8000)];
            (index, attr)
        } else {
            (self.vram[map_addr - 0x8000], 0u8)
        };
        let bank = if self.is_cgb() && attr & 0x08 != 0 {
            1
        } else {
            0
        };
        let mut row = (map_y % 8) as usize;
        if self.is_cgb() && attr & 0x40 != 0 {
            row = 7 - row;
        }
        let mut col = (map_x % 8) as usize;
        if self.is_cgb() && attr & 0x20 != 0 {
            col = 7 - col;
        }
        let addr = self.tile_data_addr(tile_index) - 0x8000 + bank * 0x2000 + row * 2;
        let lo = self.vram[addr];
        let hi = self.vram[addr + 1];
        let bit = 7 - col;
        let color_id = (((hi >> bit) & 1) << 1) | ((lo >> bit) & 1);
        (color_id, attr & 0x80 != 0)
    }

    fn render_sprites(&mut self, ly: usize, bg_color: &[u8; SCREEN_WIDTH]) {
        let height = if self.lcdc & 0x04 != 0 { 16 } else { 8 };
        let mut selected: Vec<(usize, i32)> = Vec::new();
        for i in 0..40 {
            let y = self.oam[i * 4] as i32 - 16;
            if (ly as i32) >= y && (ly as i32) < y + height {
                let x = self.oam[i * 4 + 1] as i32 - 8;
                selected.push((i, x));
                if selected.len() == 10 {
                    break;
                }
            }
        }
        // Priority order.
        if !self.is_cgb() || self.opri & 1 != 0 {
            selected.sort_by_key(|&(i, x)| (x, i));
        }

        for &(i, sx) in selected.iter().rev() {
            let attr = self.oam[i * 4 + 3];
            let mut tile = self.oam[i * 4 + 2];
            let y = self.oam[i * 4] as i32 - 16;
            let mut row = ly as i32 - y;
            if attr & 0x40 != 0 {
                row = (height - 1) - row;
            }
            if height == 16 {
                tile &= 0xFE;
                if row >= 8 {
                    tile |= 1;
                    row -= 8;
                }
            }
            let bank = if self.is_cgb() && attr & 0x08 != 0 {
                1
            } else {
                0
            };
            let addr = (tile as usize) * 16 + bank * 0x2000 + row as usize * 2;
            let lo = self.vram[addr];
            let hi = self.vram[addr + 1];
            let dmg_pal = if attr & 0x10 != 0 {
                self.obp1
            } else {
                self.obp0
            };
            let behind = attr & 0x80 != 0;
            for px in 0..8i32 {
                let x = sx + px;
                if !(0..SCREEN_WIDTH as i32).contains(&x) {
                    continue;
                }
                let col = if attr & 0x20 != 0 { 7 - px } else { px };
                let bit = 7 - col as usize;
                let color_id = (((hi >> bit) & 1) << 1) | ((lo >> bit) & 1);
                if color_id == 0 {
                    continue;
                }
                let x = x as usize;
                if behind && bg_color[x] != 0 {
                    continue;
                }
                let shade = if self.is_cgb() {
                    let pal = attr & 0x07;
                    let rgb = cgb_color(&self.obj_palette, pal, color_id);
                    self.framebuffer[ly * SCREEN_WIDTH + x] = color_id;
                    self.framebuffer_rgb555[ly * SCREEN_WIDTH + x] = rgb;
                    continue;
                } else {
                    (dmg_pal >> (color_id * 2)) & 0x03
                };
                self.framebuffer[ly * SCREEN_WIDTH + x] = shade;
                self.framebuffer_rgb555[ly * SCREEN_WIDTH + x] = dmg_rgb(shade);
            }
        }
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
    pub fn save_state(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&[
            self.lcdc,
            self.stat,
            self.scy,
            self.scx,
            self.ly,
            self.lyc,
            self.bgp,
            self.obp0,
            self.obp1,
            self.wy,
            self.wx,
            self.mode,
            self.window_line,
        ]);
        out.extend_from_slice(&self.dot.to_le_bytes());
        out.push(self.stat_line as u8);
        out.push(self.vbk);
        out.push(self.bcps);
        out.push(self.ocps);
        out.push(self.opri);
        out.extend_from_slice(&self.bg_palette);
        out.extend_from_slice(&self.obj_palette);
        out.extend_from_slice(&(self.vram.len() as u32).to_le_bytes());
        out.extend_from_slice(&self.vram);
        out.extend_from_slice(&self.oam);
    }

    /// Restore from a save-state buffer.
    pub fn load_state(&mut self, state: &[u8], cursor: &mut usize) -> Result<(), StateError> {
        let get = |n: usize, cursor: &mut usize| -> Result<&[u8], StateError> {
            let s = state
                .get(*cursor..*cursor + n)
                .ok_or(StateError::Truncated)?;
            *cursor += n;
            Ok(s)
        };
        let b = get(13, cursor)?;
        self.lcdc = b[0];
        self.stat = b[1];
        self.scy = b[2];
        self.scx = b[3];
        self.ly = b[4];
        self.lyc = b[5];
        self.bgp = b[6];
        self.obp0 = b[7];
        self.obp1 = b[8];
        self.wy = b[9];
        self.wx = b[10];
        self.mode = b[11];
        self.window_line = b[12];
        let b = get(2, cursor)?;
        self.dot = u16::from_le_bytes([b[0], b[1]]);
        let b = get(5, cursor)?;
        self.stat_line = b[0] != 0;
        self.vbk = b[1];
        self.bcps = b[2];
        self.ocps = b[3];
        self.opri = b[4];
        let b = get(64, cursor)?;
        self.bg_palette.copy_from_slice(b);
        let b = get(64, cursor)?;
        self.obj_palette.copy_from_slice(b);
        let b = get(4, cursor)?;
        let vlen = u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize;
        if vlen != self.vram.len() {
            return Err(StateError::Corrupt("VRAM size mismatch"));
        }
        let b = get(vlen, cursor)?;
        self.vram.copy_from_slice(b);
        let b = get(0xA0, cursor)?;
        self.oam.copy_from_slice(b);
        Ok(())
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
