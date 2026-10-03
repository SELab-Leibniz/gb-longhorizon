//! MBC1 — the most common controller.
//!
//! Registers (write-only, decoded by address range):
//! * 0000–1FFF  RAM enable (low nibble == 0xA)
//! * 2000–3FFF  ROM bank low 5 bits (0 is treated as 1)
//! * 4000–5FFF  2-bit "upper" register: RAM bank, or ROM bank bits 5–6
//! * 6000–7FFF  banking mode (0 = simple, 1 = advanced)
//!
//! In advanced mode the upper bits also affect the 0000–3FFF window on
//! large carts. Bank numbers are masked to the actual ROM size. The
//! Mooneye `emulator-only/mbc1` tests cover every one of these rules;
//! `multicart_rom_8Mb` is for MBC1M multicarts and is out of scope.

use super::Mbc;
use crate::emulator::StateError;
use crate::prelude::*;

/// MBC1 controller state.
pub struct Mbc1 {
    rom_banks: usize,
    ram_banks: usize,
    ram_enable: bool,
    bank_lo: u8,
    bank_hi: u8,
    mode: u8,
}

impl Mbc1 {
    /// Create for a cart with the given ROM/RAM sizes in bytes.
    pub fn new(rom_size: usize, ram_size: usize) -> Self {
        Self {
            rom_banks: (rom_size / 0x4000).max(1),
            ram_banks: (ram_size / 0x2000).max(if ram_size > 0 { 1 } else { 0 }),
            ram_enable: false,
            bank_lo: 1,
            bank_hi: 0,
            mode: 0,
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

    /// The bank the 4000–7FFF window shows (the low-5-bits-zero rule is
    /// applied here, before masking to the ROM size).
    fn rom_bank(&self) -> usize {
        let lo = (self.bank_lo & 0x1F) as usize;
        let bank = ((self.bank_hi as usize) << 5) | lo;
        if bank == 0 {
            1
        } else {
            bank
        }
    }

    fn rom_offset(&self, bank: usize, addr: u16) -> usize {
        (bank % self.rom_banks) * 0x4000 + (addr as usize & 0x3FFF)
    }

    fn ram_bank(&self) -> usize {
        if self.mode == 1 {
            self.bank_hi as usize
        } else {
            0
        }
    }

    fn ram_offset(&self, addr: u16) -> usize {
        (self.ram_bank() % self.ram_banks) * 0x2000 + (addr as usize - 0xA000)
    }
}

impl Mbc for Mbc1 {
    fn read(&self, rom: &[u8], ram: &[u8], addr: u16) -> u8 {
        match addr {
            0x0000..=0x3FFF => {
                // Only advanced mode exposes a non-zero bank here.
                let bank = if self.mode == 1 {
                    (self.bank_hi as usize) << 5
                } else {
                    0
                };
                rom[self.rom_offset(bank, addr)]
            }
            0x4000..=0x7FFF => rom[self.rom_offset(self.rom_bank(), addr)],
            0xA000..=0xBFFF if self.ram_enable && !ram.is_empty() => ram[self.ram_offset(addr)],
            _ => 0xFF,
        }
    }

    fn write(&mut self, ram: &mut [u8], addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => self.ram_enable = value & 0x0F == 0x0A,
            0x2000..=0x3FFF => self.bank_lo = value & 0x1F,
            0x4000..=0x5FFF => self.bank_hi = value & 0x03,
            0x6000..=0x7FFF => self.mode = value & 0x01,
            0xA000..=0xBFFF if self.ram_enable && !ram.is_empty() => {
                let off = self.ram_offset(addr);
                ram[off] = value;
            }
            _ => {}
        }
    }

    fn save_state(&self, out: &mut Vec<u8>) {
        out.push(self.ram_enable as u8);
        out.push(self.bank_lo);
        out.push(self.bank_hi);
        out.push(self.mode);
    }

    fn load_state(&mut self, state: &[u8], cursor: &mut usize) -> Result<(), StateError> {
        let b = state
            .get(*cursor..*cursor + 4)
            .ok_or(StateError::Truncated)?;
        self.ram_enable = b[0] != 0;
        self.bank_lo = b[1];
        self.bank_hi = b[2];
        self.mode = b[3];
        *cursor += 4;
        Ok(())
    }
}
