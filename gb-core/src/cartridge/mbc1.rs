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

/// MBC1 controller state.
pub struct Mbc1 {
    rom_banks: usize,
    ram_banks: usize,
    // TODO(agent): ram_enable, bank_lo (5 bits), bank_hi (2 bits), mode.
}

impl Mbc1 {
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

impl Mbc for Mbc1 {
    fn read(&self, _rom: &[u8], _ram: &[u8], _addr: u16) -> u8 {
        todo!("cartridge::mbc1::Mbc1::read")
    }

    fn write(&mut self, _ram: &mut [u8], _addr: u16, _value: u8) {
        todo!("cartridge::mbc1::Mbc1::write")
    }

    fn save_state(&self, _out: &mut Vec<u8>) {
        todo!("cartridge::mbc1::Mbc1::save_state")
    }

    fn load_state(&mut self, _state: &[u8], _cursor: &mut usize) -> Result<(), StateError> {
        todo!("cartridge::mbc1::Mbc1::load_state")
    }
}
