//! Memory map and bus.
//!
//! The MMU owns every peripheral and routes reads/writes by address:
//!
//! ```text
//!   0000-7FFF  cartridge ROM (banked via MBC)
//!   8000-9FFF  VRAM                     → ppu
//!   A000-BFFF  cartridge RAM (banked)   → cartridge
//!   C000-DFFF  WRAM                     (owned here)
//!   E000-FDFF  echo of C000-DDFF
//!   FE00-FE9F  OAM                      → ppu
//!   FEA0-FEFF  unusable (reads 0xFF on DMG… mostly)
//!   FF00-FF7F  I/O registers            → joypad/serial/timer/apu/ppu/interrupts
//!   FF80-FFFE  HRAM                     (owned here)
//!   FFFF       IE                       → interrupts
//! ```
//!
//! `tick(cycles)` advances every peripheral in lock-step with the CPU; see
//! `DECISIONS.md` D1 for the cycle model. OAM DMA (write to FF46) is driven
//! from here because it touches both the cartridge/WRAM side and the PPU.

use crate::apu::Apu;
use crate::cartridge::Cartridge;
use crate::emulator::StateError;
use crate::interrupts::Interrupts;
use crate::joypad::Joypad;
use crate::ppu::Ppu;
use crate::serial::Serial;
use crate::timer::Timer;

/// The system bus and everything hanging off it.
pub struct Mmu {
    /// Game cartridge (ROM + optional RAM, behind an MBC).
    pub cartridge: Cartridge,
    /// Pixel processing unit (also owns VRAM and OAM).
    pub ppu: Ppu,
    /// Audio processing unit.
    pub apu: Apu,
    /// DIV/TIMA timer block.
    pub timer: Timer,
    /// Buttons / P1 register.
    pub joypad: Joypad,
    /// Serial link port.
    pub serial: Serial,
    /// IF / IE registers.
    pub interrupts: Interrupts,
    /// 8 KiB work RAM.
    pub wram: Vec<u8>,
    /// 127 bytes high RAM.
    pub hram: Vec<u8>,
}

impl Mmu {
    /// Assemble a bus with I/O registers in their post-boot-ROM state.
    pub fn new(cartridge: Cartridge) -> Self {
        Self {
            cartridge,
            ppu: Ppu::new(),
            apu: Apu::new(),
            timer: Timer::new(),
            joypad: Joypad::new(),
            serial: Serial::new(),
            interrupts: Interrupts::new(),
            wram: vec![0; 0x2000],
            hram: vec![0; 0x7F],
        }
    }

    /// Bus read.
    pub fn read(&self, _addr: u16) -> u8 {
        todo!("mmu::Mmu::read — address decoding")
    }

    /// Bus write.
    pub fn write(&mut self, _addr: u16, _value: u8) {
        todo!("mmu::Mmu::write — address decoding, OAM DMA trigger at FF46")
    }

    /// Advance every clocked peripheral by `cycles` T-cycles and collect the
    /// interrupts they raise into `interrupts`.
    pub fn tick(&mut self, _cycles: u32) {
        todo!("mmu::Mmu::tick — step ppu/apu/timer/serial/dma, gather IF bits")
    }

    /// Append bus + peripheral state to a save-state buffer.
    pub fn save_state(&self, _out: &mut Vec<u8>) {
        todo!("mmu::Mmu::save_state")
    }

    /// Restore from a save-state buffer, advancing `cursor`.
    pub fn load_state(&mut self, _state: &[u8], _cursor: &mut usize) -> Result<(), StateError> {
        todo!("mmu::Mmu::load_state")
    }
}
