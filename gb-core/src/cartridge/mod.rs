//! Cartridge: ROM image, optional external RAM, and the memory bank
//! controller that maps them into 0000–7FFF and A000–BFFF.
//!
//! Header parsing is implemented here. Banking behaviour is the agent's,
//! one MBC per file:
//! * `mbc1.rs` — up to 2 MiB ROM / 32 KiB RAM; two banking modes; the
//!   "bank 0x20/0x40/0x60 → +1" quirk. Mooneye `emulator-only/mbc1/*`.
//! * `mbc3.rs` — up to 2 MiB ROM / 32 KiB RAM; RTC registers may be stubbed
//!   (not in acceptance scope). Mooneye `emulator-only/mbc2|mbc5` style tests
//!   do not cover MBC3, so rely on games.
//! * `mbc5.rs` — up to 8 MiB ROM (9-bit bank number) / 128 KiB RAM.
//!   Mooneye `emulator-only/mbc5/*`.

use crate::prelude::*;

pub mod mbc1;
pub mod mbc3;
pub mod mbc5;

/// Why a ROM image could not be loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadError {
    /// Fewer than 0x150 bytes — no complete header.
    TooSmall(usize),
    /// Header byte 0x147 names a controller this emulator does not support.
    UnsupportedMapper(u8),
    /// Header byte 0x148 is not a recognised ROM size code.
    BadRomSize(u8),
    /// Header byte 0x149 is not a recognised RAM size code.
    BadRamSize(u8),
}

impl core::fmt::Display for LoadError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            LoadError::TooSmall(n) => write!(f, "ROM is {n} bytes; need at least 336"),
            LoadError::UnsupportedMapper(t) => write!(f, "unsupported cartridge type 0x{t:02X}"),
            LoadError::BadRomSize(c) => write!(f, "unknown ROM size code 0x{c:02X}"),
            LoadError::BadRamSize(c) => write!(f, "unknown RAM size code 0x{c:02X}"),
        }
    }
}

impl core::error::Error for LoadError {}

/// Which controller chip the cartridge uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mapper {
    /// 32 KiB ROM, no banking (type 0x00, 0x08, 0x09).
    None,
    /// Types 0x01–0x03.
    Mbc1,
    /// Types 0x0F–0x13.
    Mbc3,
    /// Types 0x19–0x1E.
    Mbc5,
}

/// Facts decoded from the 0x100–0x14F header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    /// ASCII title (up to 16 bytes, trailing zeros stripped).
    pub title: String,
    /// Controller chip.
    pub mapper: Mapper,
    /// Whether the cart has a battery (RAM should be persisted).
    pub battery: bool,
    /// ROM size in bytes.
    pub rom_size: usize,
    /// External RAM size in bytes (0 if none).
    pub ram_size: usize,
    /// Header checksum from 0x14D.
    pub header_checksum: u8,
    /// Whether the computed header checksum matches 0x14D.
    pub header_checksum_ok: bool,
}

impl Header {
    /// Decode the header. Only validates what we need to size buffers.
    pub fn parse(rom: &[u8]) -> Result<Self, LoadError> {
        if rom.len() < 0x150 {
            return Err(LoadError::TooSmall(rom.len()));
        }
        let title_bytes = &rom[0x134..0x144];
        let title = title_bytes
            .iter()
            .take_while(|&&b| b != 0)
            .map(|&b| {
                if b.is_ascii_graphic() || b == b' ' {
                    b as char
                } else {
                    '?'
                }
            })
            .collect::<String>();

        let cart_type = rom[0x147];
        let (mapper, battery) = match cart_type {
            0x00 | 0x08 => (Mapper::None, false),
            0x09 => (Mapper::None, true),
            0x01 | 0x02 => (Mapper::Mbc1, false),
            0x03 => (Mapper::Mbc1, true),
            0x0F | 0x10 | 0x13 => (Mapper::Mbc3, true),
            0x11 | 0x12 => (Mapper::Mbc3, false),
            0x19 | 0x1A | 0x1C | 0x1D => (Mapper::Mbc5, false),
            0x1B | 0x1E => (Mapper::Mbc5, true),
            other => return Err(LoadError::UnsupportedMapper(other)),
        };

        let rom_size = match rom[0x148] {
            code @ 0x00..=0x08 => (32 * 1024) << code,
            other => return Err(LoadError::BadRomSize(other)),
        };
        let ram_size = match rom[0x149] {
            0x00 => 0,
            0x01 => 2 * 1024, // unofficial but seen in homebrew
            0x02 => 8 * 1024,
            0x03 => 32 * 1024,
            0x04 => 128 * 1024,
            0x05 => 64 * 1024,
            other => return Err(LoadError::BadRamSize(other)),
        };

        let header_checksum = rom[0x14D];
        let computed = rom[0x134..=0x14C]
            .iter()
            .fold(0u8, |acc, &b| acc.wrapping_sub(b).wrapping_sub(1));

        Ok(Self {
            title,
            mapper,
            battery,
            rom_size,
            ram_size,
            header_checksum,
            header_checksum_ok: computed == header_checksum,
        })
    }
}

/// The MBC interface every controller implements.
pub trait Mbc {
    /// Read from 0000–7FFF (ROM) or A000–BFFF (RAM).
    fn read(&self, rom: &[u8], ram: &[u8], addr: u16) -> u8;
    /// Write to 0000–7FFF (control registers) or A000–BFFF (RAM).
    fn write(&mut self, ram: &mut [u8], addr: u16, value: u8);
    /// Serialise controller registers.
    fn save_state(&self, out: &mut Vec<u8>);
    /// Restore controller registers.
    fn load_state(
        &mut self,
        state: &[u8],
        cursor: &mut usize,
    ) -> Result<(), crate::emulator::StateError>;
}

/// A loaded cartridge.
pub struct Cartridge {
    /// Decoded header.
    pub header: Header,
    rom: Vec<u8>,
    ram: Vec<u8>,
    mbc: Box<dyn Mbc>,
}

impl Cartridge {
    /// Parse the header and pick a controller.
    pub fn from_bytes(rom: &[u8]) -> Result<Self, LoadError> {
        let header = Header::parse(rom)?;
        // A controller that supports external RAM but a header that declares
        // none still gets the standard 8 KiB: test ROMs and homebrew in the
        // wild rely on the RAM being present.
        let ram_size = if header.ram_size == 0
            && matches!(header.mapper, Mapper::Mbc1 | Mapper::Mbc3 | Mapper::Mbc5)
        {
            8 * 1024
        } else {
            header.ram_size
        };
        let mbc: Box<dyn Mbc> = match header.mapper {
            Mapper::None => Box::new(NoMbc),
            Mapper::Mbc1 => Box::new(mbc1::Mbc1::new(header.rom_size, ram_size)),
            Mapper::Mbc3 => Box::new(mbc3::Mbc3::new(header.rom_size, ram_size)),
            Mapper::Mbc5 => Box::new(mbc5::Mbc5::new(header.rom_size, ram_size)),
        };
        // Pad short images so bank arithmetic never indexes out of range.
        let mut rom_vec = rom.to_vec();
        if rom_vec.len() < header.rom_size {
            rom_vec.resize(header.rom_size, 0xFF);
        }
        Ok(Self {
            ram: vec![0; ram_size],
            rom: rom_vec,
            mbc,
            header,
        })
    }

    /// Read 0000–7FFF or A000–BFFF.
    pub fn read(&self, addr: u16) -> u8 {
        self.mbc.read(&self.rom, &self.ram, addr)
    }

    /// Write 0000–7FFF or A000–BFFF.
    pub fn write(&mut self, addr: u16, value: u8) {
        self.mbc.write(&mut self.ram, addr, value)
    }

    /// External RAM, if present.
    pub fn ram(&self) -> Option<&[u8]> {
        (!self.ram.is_empty()).then_some(self.ram.as_slice())
    }

    /// Mutable external RAM, if present (used by the WebAssembly host to
    /// write a battery save into linear memory).
    pub fn ram_mut(&mut self) -> Option<&mut [u8]> {
        (!self.ram.is_empty()).then_some(self.ram.as_mut_slice())
    }

    /// Replace external RAM contents (loading a battery save).
    pub fn set_ram(&mut self, data: &[u8]) {
        let n = data.len().min(self.ram.len());
        self.ram[..n].copy_from_slice(&data[..n]);
    }

    /// Serialise RAM + controller state (ROM is not included).
    pub fn save_state(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&(self.ram.len() as u32).to_le_bytes());
        out.extend_from_slice(&self.ram);
        self.mbc.save_state(out);
    }

    /// Restore RAM + controller state.
    pub fn load_state(
        &mut self,
        state: &[u8],
        cursor: &mut usize,
    ) -> Result<(), crate::emulator::StateError> {
        use crate::emulator::StateError;
        let len_bytes = state
            .get(*cursor..*cursor + 4)
            .ok_or(StateError::Truncated)?;
        let len = u32::from_le_bytes(len_bytes.try_into().unwrap()) as usize;
        *cursor += 4;
        if len != self.ram.len() {
            return Err(StateError::Corrupt("cartridge RAM size mismatch"));
        }
        let ram = state
            .get(*cursor..*cursor + len)
            .ok_or(StateError::Truncated)?;
        self.ram.copy_from_slice(ram);
        *cursor += len;
        self.mbc.load_state(state, cursor)
    }
}

/// 32 KiB cartridges with no controller: ROM is mapped flat, RAM (if any) at A000.
struct NoMbc;

impl Mbc for NoMbc {
    fn read(&self, rom: &[u8], ram: &[u8], addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => rom.get(addr as usize).copied().unwrap_or(0xFF),
            0xA000..=0xBFFF => ram.get((addr - 0xA000) as usize).copied().unwrap_or(0xFF),
            _ => 0xFF,
        }
    }
    fn write(&mut self, ram: &mut [u8], addr: u16, value: u8) {
        if let 0xA000..=0xBFFF = addr {
            if let Some(slot) = ram.get_mut((addr - 0xA000) as usize) {
                *slot = value;
            }
        }
    }
    fn save_state(&self, _out: &mut Vec<u8>) {}
    fn load_state(
        &mut self,
        _state: &[u8],
        _cursor: &mut usize,
    ) -> Result<(), crate::emulator::StateError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rom_with(cart_type: u8, rom_code: u8, ram_code: u8) -> Vec<u8> {
        let mut rom = vec![0u8; 0x8000];
        rom[0x134..0x134 + 4].copy_from_slice(b"TEST");
        rom[0x147] = cart_type;
        rom[0x148] = rom_code;
        rom[0x149] = ram_code;
        let sum = rom[0x134..=0x14C]
            .iter()
            .fold(0u8, |acc, &b| acc.wrapping_sub(b).wrapping_sub(1));
        rom[0x14D] = sum;
        rom
    }

    #[test]
    fn parses_header() {
        let h = Header::parse(&rom_with(0x03, 0x01, 0x02)).unwrap();
        assert_eq!(h.title, "TEST");
        assert_eq!(h.mapper, Mapper::Mbc1);
        assert!(h.battery);
        assert_eq!(h.rom_size, 64 * 1024);
        assert_eq!(h.ram_size, 8 * 1024);
        assert!(h.header_checksum_ok);
    }

    #[test]
    fn rejects_small_and_unknown() {
        assert_eq!(Header::parse(&[0; 10]), Err(LoadError::TooSmall(10)));
        assert_eq!(
            Header::parse(&rom_with(0xFD, 0, 0)).unwrap_err(),
            LoadError::UnsupportedMapper(0xFD)
        );
    }

    #[test]
    fn no_mbc_reads_flat() {
        let mut rom = rom_with(0x00, 0x01, 0x00);
        rom[0x4000] = 0xAB;
        let cart = Cartridge::from_bytes(&rom).unwrap();
        assert_eq!(cart.read(0x4000), 0xAB);
        assert_eq!(cart.read(0xA000), 0xFF, "no RAM reads open bus");
    }
}
