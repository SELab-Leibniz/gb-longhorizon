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

/// MBC5 controller state.
pub struct Mbc5 {
    rom_banks: usize,
    ram_banks: usize,
    // TODO(agent): ram_enable, rom_bank (9 bits), ram_bank (4 bits).
}

impl Mbc5 {
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

impl Mbc for Mbc5 {
    fn read(&self, _rom: &[u8], _ram: &[u8], _addr: u16) -> u8 {
        todo!("cartridge::mbc5::Mbc5::read")
    }

    fn write(&mut self, _ram: &mut [u8], _addr: u16, _value: u8) {
        todo!("cartridge::mbc5::Mbc5::write")
    }

    fn save_state(&self, _out: &mut Vec<u8>) {
        todo!("cartridge::mbc5::Mbc5::save_state")
    }

    fn load_state(&mut self, _state: &[u8], _cursor: &mut usize) -> Result<(), StateError> {
        todo!("cartridge::mbc5::Mbc5::load_state")
    }
}
