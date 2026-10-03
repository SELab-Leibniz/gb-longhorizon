//! MBC5 — the simplest large controller: no quirks, 9-bit ROM bank, bank 0
//! really is bank 0.
//!
//! * 0000–1FFF  RAM enable (low nibble == 0xA)
//! * 2000–2FFF  ROM bank low 8 bits
//! * 3000–3FFF  ROM bank bit 8
//! * 4000–5FFF  RAM bank 0–15 (bit 3 is rumble on rumble carts; ignore)
//!
//! Mooneye `emulator-only/mbc5/rom_*` checks the bank mask for each ROM size.

use super::Mbc;
use crate::emulator::StateError;
use crate::prelude::*;

/// MBC5 controller state.
pub struct Mbc5 {
    rom_banks: usize,
    ram_banks: usize,
    ram_enable: bool,
    rom_bank: u16,
    ram_bank: u8,
}

impl Mbc5 {
    /// Create for a cart with the given ROM/RAM sizes in bytes.
    pub fn new(rom_size: usize, ram_size: usize) -> Self {
        Self {
            rom_banks: (rom_size / 0x4000).max(1),
            ram_banks: (ram_size / 0x2000).max(if ram_size > 0 { 1 } else { 0 }),
            ram_enable: false,
            rom_bank: 1,
            ram_bank: 0,
        }
    }

    /// Number of 16 KiB ROM banks.
    pub fn rom_banks(&self) -> usize {
        self.rom_banks
    }

    /// Number of 8 KiB RAM banks (0 if no RAM).
    pub fn ram_banks(&self) -> usize {
        self.ram_banks
    }
}

impl Mbc for Mbc5 {
    fn read(&self, rom: &[u8], ram: &[u8], addr: u16) -> u8 {
        match addr {
            0x0000..=0x3FFF => rom[addr as usize],
            0x4000..=0x7FFF => {
                let bank = (self.rom_bank as usize) % self.rom_banks;
                rom[bank * 0x4000 + (addr as usize - 0x4000)]
            }
            0xA000..=0xBFFF if self.ram_enable && !ram.is_empty() => {
                let bank = (self.ram_bank as usize & 0x0F) % self.ram_banks;
                ram[bank * 0x2000 + (addr as usize - 0xA000)]
            }
            _ => 0xFF,
        }
    }

    fn write(&mut self, ram: &mut [u8], addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => self.ram_enable = value & 0x0F == 0x0A,
            0x2000..=0x2FFF => self.rom_bank = (self.rom_bank & 0x100) | value as u16,
            0x3000..=0x3FFF => {
                self.rom_bank = (self.rom_bank & 0x0FF) | (((value & 0x01) as u16) << 8)
            }
            0x4000..=0x5FFF => self.ram_bank = value & 0x0F,
            0xA000..=0xBFFF if self.ram_enable && !ram.is_empty() => {
                let bank = (self.ram_bank as usize & 0x0F) % self.ram_banks;
                ram[bank * 0x2000 + (addr as usize - 0xA000)] = value;
            }
            _ => {}
        }
    }

    fn save_state(&self, out: &mut Vec<u8>) {
        out.push(self.ram_enable as u8);
        out.extend_from_slice(&self.rom_bank.to_le_bytes());
        out.push(self.ram_bank);
    }

    fn load_state(&mut self, state: &[u8], cursor: &mut usize) -> Result<(), StateError> {
        let b = state
            .get(*cursor..*cursor + 4)
            .ok_or(StateError::Truncated)?;
        self.ram_enable = b[0] != 0;
        self.rom_bank = u16::from_le_bytes([b[1], b[2]]);
        self.ram_bank = b[3];
        *cursor += 4;
        Ok(())
    }
}
