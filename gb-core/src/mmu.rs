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
//! The CPU drives time (`DECISIONS.md` D1): every bus access it makes goes
//! through `cycle_read` / `cycle_write`, which advance all peripherals by one
//! M-cycle as part of the access, and internal delay cycles call
//! `idle_cycle`. `read` / `write` are untimed. OAM DMA (write to FF46) is
//! driven from `tick` because it touches both the cartridge/WRAM side and the
//! PPU.

use crate::apu::Apu;
use crate::cartridge::Cartridge;
use crate::emulator::Model;
use crate::emulator::StateError;
use crate::interrupts::Interrupts;
use crate::joypad::Joypad;
use crate::ppu::Ppu;
use crate::serial::Serial;
use crate::timer::Timer;

/// The system bus and everything hanging off it.
///
/// CGB mode adds (Pan Docs "CGB Registers"): KEY1 speed switch (FF4D, armed
/// here, performed by `STOP`), VBK VRAM bank (FF4F, in the PPU), SVBK WRAM
/// bank 1–7 (FF70), HDMA1–5 general/HBlank DMA (FF51–FF55), RP (FF56, may be
/// stubbed), and OPRI (FF6C, in the PPU). In double-speed mode the CPU, DIV,
/// timer and serial run twice as fast while PPU and APU keep real time
/// (DECISIONS.md D7).
pub struct Mmu {
    /// Which console the bus belongs to.
    pub model: Model,
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
    /// Work RAM: 8 KiB on DMG, 32 KiB (8 banks of 4 KiB) on CGB.
    pub wram: Vec<u8>,
    /// 127 bytes high RAM.
    pub hram: Vec<u8>,
}

impl Mmu {
    /// Assemble a bus with I/O registers in their post-boot-ROM state.
    pub fn new(cartridge: Cartridge, model: Model) -> Self {
        Self {
            model,
            cartridge,
            ppu: Ppu::new(model),
            apu: Apu::new(),
            timer: Timer::new(),
            joypad: Joypad::new(),
            serial: Serial::new(),
            interrupts: Interrupts::new(),
            wram: vec![0; if model == Model::Cgb { 0x8000 } else { 0x2000 }],
            hram: vec![0; 0x7F],
        }
    }

    /// Untimed bus read (no peripheral advance). For OAM DMA source reads,
    /// debugging and save states — the CPU uses `cycle_read`.
    pub fn read(&self, _addr: u16) -> u8 {
        todo!("mmu::Mmu::read — address decoding")
    }

    /// Untimed bus write. The CPU uses `cycle_write`.
    pub fn write(&mut self, _addr: u16, _value: u8) {
        todo!("mmu::Mmu::write — address decoding, OAM DMA trigger at FF46")
    }

    /// One CPU memory-read M-cycle: advance all peripherals by 4 T-cycles and
    /// perform the read, in the order hardware does (see Pan Docs and the
    /// Mooneye timing tests for where within the M-cycle the access lands).
    pub fn cycle_read(&mut self, _addr: u16) -> u8 {
        todo!("mmu::Mmu::cycle_read — tick(4) + read, correctly ordered")
    }

    /// One CPU memory-write M-cycle: advance all peripherals by 4 T-cycles
    /// and perform the write.
    pub fn cycle_write(&mut self, _addr: u16, _value: u8) {
        todo!("mmu::Mmu::cycle_write — tick(4) + write, correctly ordered")
    }

    /// One CPU M-cycle with no bus access (internal delay, e.g. the extra
    /// cycle of `PUSH`, a taken `JR`, or 16-bit `INC`). Written for normal
    /// speed; CGB double speed halves the real time of an M-cycle (D7).
    pub fn idle_cycle(&mut self) {
        self.tick(4);
    }

    /// Advance every clocked peripheral by `cycles` T-cycles and collect the
    /// interrupts they raise into `interrupts`. Called via the three methods
    /// above; the facade never calls it directly.
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
