//! MBC3 — ROM bank is a full 7 bits (no "upper register" games), RAM bank
//! register 0–3 selects RAM, 8–C selects an RTC register.
//!
//! * 0000–1FFF  RAM/RTC enable (low nibble == 0xA)
//! * 2000–3FFF  ROM bank, 7 bits (0 → 1)
//! * 4000–5FFF  RAM bank 0–3 or RTC register 08–0C
//! * 6000–7FFF  latch clock data (write 0 then 1)
//!
//! RTC is **not** in acceptance scope: the RTC registers read back a fixed
//! constant, but reads/writes must not panic because some games touch them.

use super::Mbc;
use crate::emulator::StateError;
use crate::prelude::*;

/// MBC3 controller state.
pub struct Mbc3 {
    rom_banks: usize,
    ram_banks: usize,
    ram_enable: bool,
    rom_bank: u8,
    ram_bank: u8,
}

impl Mbc3 {
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

    fn current_rom_bank(&self) -> usize {
        let b = (self.rom_bank & 0x7F) as usize;
        if b == 0 {
            1
        } else {
            b
        }
    }
}

impl Mbc for Mbc3 {
    fn read(&self, rom: &[u8], ram: &[u8], addr: u16) -> u8 {
        match addr {
            0x0000..=0x3FFF => rom[addr as usize],
            0x4000..=0x7FFF => {
                let bank = self.current_rom_bank() % self.rom_banks;
                rom[bank * 0x4000 + (addr as usize - 0x4000)]
            }
            0xA000..=0xBFFF => {
                if !self.ram_enable {
                    return 0xFF;
                }
                match self.ram_bank {
                    0x00..=0x03 => {
                        if ram.is_empty() {
                            0xFF
                        } else {
                            let bank = (self.ram_bank as usize) % self.ram_banks;
                            ram[bank * 0x2000 + (addr as usize - 0xA000)]
                        }
                    }
                    // RTC seconds/minutes/hours/days/latch — stubbed constant.
                    0x08..=0x0C => 0,
                    _ => 0xFF,
                }
            }
            _ => 0xFF,
        }
    }

    fn write(&mut self, ram: &mut [u8], addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => self.ram_enable = value & 0x0F == 0x0A,
            0x2000..=0x3FFF => self.rom_bank = value & 0x7F,
            0x4000..=0x5FFF => self.ram_bank = value,
            0x6000..=0x7FFF => {} // RTC latch — ignored with a stubbed clock
            0xA000..=0xBFFF if self.ram_enable && self.ram_bank <= 0x03 && !ram.is_empty() => {
                let bank = (self.ram_bank as usize) % self.ram_banks;
                ram[bank * 0x2000 + (addr as usize - 0xA000)] = value;
            }
            _ => {}
        }
    }

    fn save_state(&self, out: &mut Vec<u8>) {
        out.push(self.ram_enable as u8);
        out.push(self.rom_bank);
        out.push(self.ram_bank);
    }

    fn load_state(&mut self, state: &[u8], cursor: &mut usize) -> Result<(), StateError> {
        let b = state
            .get(*cursor..*cursor + 3)
            .ok_or(StateError::Truncated)?;
        self.ram_enable = b[0] != 0;
        self.rom_bank = b[1];
        self.ram_bank = b[2];
        *cursor += 3;
        Ok(())
    }
}
