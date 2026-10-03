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
use crate::prelude::*;
use crate::serial::Serial;
use crate::timer::Timer;

/// In-flight OAM DMA transfer.
struct OamDma {
    source: u16,
    byte: u16,
    delay: u32,
}

/// One data-bus access observed while executing an instruction. Only loads,
/// stores, read-modify-write, pushes, pops and stack writes are recorded —
/// never opcode or operand fetches, and never debugger accesses. Used by
/// `gb-server` watchpoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataAccess {
    /// Address touched.
    pub addr: u16,
    /// Byte read from or written to it.
    pub value: u8,
    /// `true` for a write, `false` for a read.
    pub write: bool,
}

/// CGB general-purpose / HBlank DMA registers (FF51–FF55).
#[derive(Default)]
struct Hdma {
    src: u16,
    dst: u16,
    active: bool,
    hblank: bool,
    remaining: u8,
}

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
    key1: u8,
    svbk: u8,
    rp: u8,
    speed: bool,
    boot_disabled: bool,
    oam_dma_reg: u8,
    oam_dma: Option<OamDma>,
    hdma: Hdma,
    /// Monotonic count of real (4.19 MHz master-clock) T-cycles ticked since
    /// load. `Cpu::step` diffs this to learn how long an instruction took,
    /// which keeps the CPU's cycle accounting consistent with the bus.
    total_real: u64,
    /// "Gameboy Doctor" mode: reads of LY (`$FF44`) return `$90` so reference
    /// traces do not depend on PPU timing (`gb-trace --doctor`).
    doctor: bool,
    /// Data-bus accesses recorded during the instruction(s) since the last
    /// drain. Cleared by `Cpu::step`.
    pub(crate) accesses: Vec<DataAccess>,
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
            key1: 0,
            svbk: 1,
            rp: 0xFF,
            speed: false,
            boot_disabled: false,
            oam_dma_reg: 0xFF,
            oam_dma: None,
            hdma: Hdma::default(),
            total_real: 0,
            doctor: false,
            accesses: Vec::new(),
        }
    }

    /// Enable or disable "Gameboy Doctor" mode (LY reads `$90`).
    pub fn set_doctor(&mut self, on: bool) {
        self.doctor = on;
    }

    /// Take the data-bus accesses recorded since the last call.
    pub fn take_accesses(&mut self) -> Vec<DataAccess> {
        core::mem::take(&mut self.accesses)
    }

    /// Real master-clock T-cycles elapsed since load. Used by the CPU to
    /// measure an instruction's duration from the bus accesses it made.
    pub fn real_cycles(&self) -> u64 {
        self.total_real
    }

    /// True while running at double speed (CGB only).
    pub fn is_double_speed(&self) -> bool {
        self.speed
    }

    /// Perform a CGB speed switch (triggered by `STOP` with KEY1 armed).
    pub fn switch_speed(&mut self) {
        if self.model == Model::Cgb && self.key1 & 0x01 != 0 {
            self.speed = !self.speed;
            self.key1 = (self.key1 & !0x80) | if self.speed { 0x80 } else { 0 };
        }
    }

    fn wram_index(&self, addr: u16) -> usize {
        match addr {
            0xC000..=0xCFFF => (addr - 0xC000) as usize,
            0xD000..=0xDFFF => {
                let bank = if self.model == Model::Cgb {
                    let b = self.svbk & 0x07;
                    if b == 0 {
                        1
                    } else {
                        b
                    }
                } else {
                    1
                };
                bank as usize * 0x1000 + (addr - 0xD000) as usize
            }
            _ => 0,
        }
    }

    /// True once the OAM DMA controller has actually begun transferring: from
    /// that point it owns the OAM area until the transfer ends. During the
    /// initial two-M-cycle startup the CPU can still reach OAM, which is what
    /// Mooneye's `oam_dma_start` measures (one instruction executes from OAM
    /// before the lock is asserted).
    fn oam_bus_locked(&self) -> bool {
        matches!(&self.oam_dma, Some(d) if d.byte > 0 || d.delay == 0)
    }

    fn dma_allows(addr: u16) -> bool {
        // While an OAM DMA is in flight the DMA controller owns the OAM area:
        // reads there return 0xFF and writes are ignored. Every other address
        // (ROM, VRAM, cartridge RAM, WRAM and its echo, HRAM) stays reachable
        // by the CPU. Mooneye's timing tests both run from HRAM/WRAM and, like
        // `ret_timing`, run short procedures straight from ROM while the
        // transfer is in flight, so blocking the whole external bus would make
        // them execute garbage.
        !matches!(addr, 0xFE00..=0xFE9F)
    }

    /// Untimed bus read (no peripheral advance). For OAM DMA source reads,
    /// debugging and save states — the CPU uses `cycle_read`.
    pub fn read(&self, addr: u16) -> u8 {
        if self.oam_bus_locked() && !Self::dma_allows(addr) {
            return 0xFF;
        }
        self.read_internal(addr)
    }

    fn read_internal(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => self.cartridge.read(addr),
            0x8000..=0x9FFF => self.ppu.read(addr),
            0xA000..=0xBFFF => self.cartridge.read(addr),
            0xC000..=0xDFFF => self.wram[self.wram_index(addr)],
            0xE000..=0xFDFF => self.wram[self.wram_index(addr - 0x2000)],
            0xFE00..=0xFE9F => self.ppu.read(addr),
            0xFEA0..=0xFEFF => 0x00,
            0xFF00 => self.joypad.read(),
            0xFF01..=0xFF02 => self.serial.read(addr),
            0xFF04..=0xFF07 => self.timer.read(addr),
            0xFF0F => self.interrupts.flags | 0xE0,
            0xFF10..=0xFF3F => self.apu.read(addr),
            0xFF44 if self.doctor => 0x90,
            0xFF40..=0xFF45 | 0xFF47..=0xFF4B => self.ppu.read(addr),
            0xFF46 => self.oam_dma_reg,
            0xFF4D if self.model == Model::Cgb => self.key1 | 0x7E,
            0xFF4D => 0xFF,
            0xFF4F if self.model == Model::Cgb => self.ppu.read(addr),
            0xFF4F => 0xFF,
            0xFF50 => 0xFF,
            0xFF51..=0xFF55 if self.model == Model::Cgb => self.hdma_read(addr),
            0xFF51..=0xFF55 => 0xFF,
            0xFF56 if self.model == Model::Cgb => self.rp | 0x3C,
            0xFF56 => 0xFF,
            0xFF68..=0xFF6C if self.model == Model::Cgb => self.ppu.read(addr),
            0xFF68..=0xFF6C => 0xFF,
            0xFF70 if self.model == Model::Cgb => self.svbk | 0xF8,
            0xFF70 => 0xFF,
            0xFF80..=0xFFFE => self.hram[(addr - 0xFF80) as usize],
            0xFFFF => self.interrupts.enable,
            _ => 0xFF,
        }
    }

    /// Untimed bus write. The CPU uses `cycle_write`.
    pub fn write(&mut self, addr: u16, value: u8) {
        if self.oam_bus_locked() && !Self::dma_allows(addr) {
            return;
        }
        self.write_internal(addr, value);
    }

    fn write_internal(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x7FFF => self.cartridge.write(addr, value),
            0x8000..=0x9FFF => self.ppu.write(addr, value),
            0xA000..=0xBFFF => self.cartridge.write(addr, value),
            0xC000..=0xDFFF => {
                let i = self.wram_index(addr);
                self.wram[i] = value;
            }
            0xE000..=0xFDFF => {
                let i = self.wram_index(addr - 0x2000);
                self.wram[i] = value;
            }
            0xFE00..=0xFE9F => self.ppu.write(addr, value),
            0xFEA0..=0xFEFF => {}
            0xFF00 => self.joypad.write(value),
            0xFF01..=0xFF02 => self.serial.write(addr, value),
            0xFF04..=0xFF07 => self.timer.write(addr, value),
            0xFF0F => self.interrupts.flags = value & 0x1F,
            0xFF10..=0xFF3F => self.apu.write(addr, value),
            0xFF40..=0xFF45 | 0xFF47..=0xFF4B => self.ppu.write(addr, value),
            0xFF46 => self.start_oam_dma(value),
            0xFF4D if self.model == Model::Cgb => {
                self.key1 = (self.key1 & 0x80) | (value & 0x01);
            }
            0xFF4F if self.model == Model::Cgb => self.ppu.write(addr, value),
            0xFF50 => self.boot_disabled = value != 0,
            0xFF51..=0xFF55 if self.model == Model::Cgb => self.hdma_write(addr, value),
            0xFF56 if self.model == Model::Cgb => self.rp = value & 0xC1,
            0xFF68..=0xFF6C if self.model == Model::Cgb => self.ppu.write(addr, value),
            0xFF70 => {
                if self.model == Model::Cgb {
                    self.svbk = value & 0x07;
                }
            }
            0xFF80..=0xFFFE => self.hram[(addr - 0xFF80) as usize] = value,
            0xFFFF => self.interrupts.enable = value,
            _ => {}
        }
    }

    #[allow(unused_variables)]
    fn start_oam_dma(&mut self, value: u8) {
        todo!("start an OAM DMA from page `value` (write to $FF46)")
    }

    fn hdma_read(&self, addr: u16) -> u8 {
        match addr {
            0xFF51 => (self.hdma.src >> 8) as u8,
            0xFF52 => self.hdma.src as u8,
            0xFF53 => (self.hdma.dst >> 8) as u8,
            0xFF54 => self.hdma.dst as u8,
            0xFF55 if self.hdma.active => (self.hdma.remaining.wrapping_sub(1)) & 0x7F,
            _ => 0xFF,
        }
    }

    #[allow(unused_variables)]
    fn hdma_write(&mut self, addr: u16, value: u8) {
        todo!("CGB HDMA registers $FF51-$FF55: general-purpose and H-blank transfers (R-CGB-1)")
    }

    fn hdma_transfer_block(&mut self) {
        todo!("copy one 16-byte HDMA block from source to VRAM")
    }

    #[allow(unused_variables)]
    fn tick_dma(&mut self, real: u32) {
        todo!("advance OAM DMA and H-blank HDMA by `real` master-clock cycles")
    }

    /// One CPU memory-read M-cycle: advance all peripherals by 4 T-cycles and
    /// perform the read, in the order hardware does (see Pan Docs and the
    /// Mooneye timing tests for where within the M-cycle the access lands).
    pub fn cycle_read(&mut self, addr: u16) -> u8 {
        let value = self.cycle_read_fetch(addr);
        self.accesses.push(DataAccess {
            addr,
            value,
            write: false,
        });
        value
    }

    /// A bus read that is *not* a data access: an opcode or operand fetch.
    /// Same bus cycle as [`Self::cycle_read`] but not recorded for the
    /// debugger's watchpoints.
    pub fn cycle_read_fetch(&mut self, addr: u16) -> u8 {
        self.tick(4);
        self.read(addr)
    }

    /// One CPU memory-write M-cycle: advance all peripherals by 4 T-cycles
    /// and perform the write.
    ///
    /// TAC ($FF07) is latched part-way through the M-cycle: the timer samples
    /// the counter bit with the new enable value already in place *before* the
    /// final counter increment of the M-cycle. This is what lets the Mooneye
    /// `rapid_toggle` test observe the DIV-selected-bit falling edge that lands
    /// on the last T-cycle of the write; every other register is written at the
    /// end of its M-cycle.
    pub fn cycle_write(&mut self, addr: u16, value: u8) {
        if addr == 0xFF07 {
            self.tick(3);
            self.write(addr, value);
            self.tick(1);
        } else {
            self.tick(4);
            self.write(addr, value);
        }
        self.accesses.push(DataAccess {
            addr,
            value,
            write: true,
        });
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
    pub fn tick(&mut self, cycles: u32) {
        let real = if self.speed { cycles / 2 } else { cycles };
        self.total_real += real as u64;
        let mut irq = 0;
        irq |= self.timer.tick(cycles);
        irq |= self.serial.tick(cycles);
        irq |= self.ppu.tick(real);
        self.apu.tick(real);
        self.tick_dma(real);
        irq |= self.joypad.take_irq();
        self.interrupts.request_bits(irq);
    }

    /// Append bus + peripheral state to a save-state buffer.
    #[allow(unused_variables)]
    #[allow(clippy::ptr_arg)]
    pub fn save_state(&self, out: &mut Vec<u8>) {
        todo!("append the bus state (WRAM/HRAM, banks, DMA, I/O) and its peripherals' state to `out` (R-CORE-6)")
    }

    /// Restore from a save-state buffer, advancing `cursor`.
    #[allow(unused_variables)]
    pub fn load_state(&mut self, state: &[u8], cursor: &mut usize) -> Result<(), StateError> {
        todo!("restore the bus state written by save_state (R-CORE-6)")
    }
}
