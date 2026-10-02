//! MBC3 — ROM bank is a full 7 bits (no "upper register" games), RAM bank
//! register 0–3 selects RAM, 8–C selects an RTC register.
//!
//! * 0000–1FFF  RAM/RTC enable (low nibble == 0xA)
//! * 2000–3FFF  ROM bank, 7 bits (0 → 1)
//! * 4000–5FFF  RAM bank 0–3 or RTC register 08–0C
//! * 6000–7FFF  latch clock data (write 0 then 1)
//!
//! RTC is **not** in acceptance scope: returning a constant time from the
//! RTC registers is fine, but reads/writes must not panic because some
//! games (and homebrew) touch them.

use super::Mbc;
use crate::emulator::StateError;

/// MBC3 controller state.
pub struct Mbc3 {
    rom_banks: usize,
    ram_banks: usize,
    // TODO(agent): ram_enable, rom_bank (7 bits), ram_or_rtc select, RTC stub.
}

impl Mbc3 {
    /// Create for a cart with the given ROM/RAM sizes in bytes.
    pub fn new(rom_size: usize, ram_size: usize) -> Self {
        Self {
            rom_banks: (rom_size / 0x4000).max(1),
            ram_banks: (ram_size / 0x2000).max(if ram_size > 0 { 1 } else { 0 }),
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

impl Mbc for Mbc3 {
    fn read(&self, _rom: &[u8], _ram: &[u8], _addr: u16) -> u8 {
        todo!("cartridge::mbc3::Mbc3::read")
    }

    fn write(&mut self, _ram: &mut [u8], _addr: u16, _value: u8) {
        todo!("cartridge::mbc3::Mbc3::write")
    }

    fn save_state(&self, _out: &mut Vec<u8>) {
        todo!("cartridge::mbc3::Mbc3::save_state")
    }

    fn load_state(&mut self, _state: &[u8], _cursor: &mut usize) -> Result<(), StateError> {
        todo!("cartridge::mbc3::Mbc3::load_state")
    }
}
