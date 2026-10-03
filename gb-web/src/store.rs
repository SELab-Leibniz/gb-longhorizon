//! Game metadata, ROM validation, and on-disk library persistence.
//!
//! A game is fully in the library or not at all: the ROM is written to a
//! temporary file and atomically renamed into place, then the metadata file
//! (the commit point) is written the same way. The index is rebuilt by
//! scanning the metadata files at startup, so a crash mid-upload leaves
//! nothing behind.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// The Nintendo logo stored at `$104`–`$133`.
pub const NINTENDO_LOGO: [u8; 48] = [
    0xCE, 0xED, 0x66, 0x66, 0xCC, 0x0D, 0x00, 0x0B, 0x03, 0x73, 0x00, 0x83, 0x00, 0x0C, 0x00, 0x0D,
    0x00, 0x08, 0x11, 0x1F, 0x88, 0x89, 0x00, 0x0E, 0xDC, 0xCC, 0x6E, 0xE6, 0xDD, 0xDD, 0xD9, 0x99,
    0xBB, 0xBB, 0x67, 0x63, 0x6E, 0x0E, 0xEC, 0xCC, 0xDD, 0xDC, 0x99, 0x9F, 0xBB, 0xB9, 0x33, 0x3E,
];

/// Largest upload we accept (OI-1): 8 MiB.
pub const MAX_UPLOAD: usize = 8 * 1024 * 1024;

/// One game in the library, with everything the API reports (Appendix D.2).
#[derive(Debug, Clone)]
pub struct Game {
    pub id: String,
    pub title: String,
    pub filename: String,
    pub size: usize,
    pub cartridge_type: u8,
    pub mapper: &'static str,
    pub battery: bool,
    pub rom_banks: usize,
    pub ram_size: usize,
    pub cgb: &'static str,
    pub sgb: bool,
    pub header_checksum_ok: bool,
    pub global_checksum_ok: bool,
    pub playable: bool,
    pub added: u64,
}

/// Why an upload is not a Game Boy ROM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RomError {
    /// Too small, or the size does not match `$148`.
    Size,
    /// The Nintendo logo at `$104` is wrong.
    Logo,
    /// The header checksum at `$14D` is wrong.
    Checksum,
}

impl RomError {
    /// The API error code (always `not_a_rom`, 400).
    pub fn code(self) -> &'static str {
        "not_a_rom"
    }
}

/// The mapper name for a cartridge type byte (Appendix D.2).
fn mapper_name(cart_type: u8) -> &'static str {
    match cart_type {
        0x00 | 0x08 | 0x09 => "ROM",
        0x01..=0x03 => "MBC1",
        0x05 | 0x06 => "MBC2",
        0x0B..=0x0D => "MMM01",
        0x0F..=0x13 => "MBC3",
        0x19..=0x1E => "MBC5",
        0x20 => "MBC6",
        0x22 => "MBC7",
        0xFC => "CAMERA",
        0xFD => "TAMA5",
        0xFE => "HUC3",
        0xFF => "HUC1",
        _ => "UNKNOWN",
    }
}

/// Whether the emulator supports the cartridge type (R-CORE-3).
pub fn is_playable(cart_type: u8) -> bool {
    matches!(
        cart_type,
        0x00 | 0x01 | 0x02 | 0x03 | 0x08 | 0x09 | 0x0F..=0x13 | 0x19..=0x1E
    )
}

/// Whether the cartridge type is battery-backed (Appendix D.2).
fn is_battery(cart_type: u8) -> bool {
    matches!(
        cart_type,
        0x03 | 0x06 | 0x09 | 0x0D | 0x0F | 0x10 | 0x13 | 0x1B | 0x1E | 0x22 | 0xFF
    )
}

/// External RAM size from `$149` (Appendix D.2).
fn ram_size(code: u8) -> usize {
    match code {
        0x01 => 2048,
        0x02 => 8192,
        0x03 => 32768,
        0x04 => 131072,
        0x05 => 65536,
        _ => 0,
    }
}

/// Validate an upload as a Game Boy ROM (Appendix D.4).
pub fn validate_rom(rom: &[u8]) -> Result<(), RomError> {
    if rom.len() < 32 * 1024 || rom.len() != (32 * 1024usize) << rom[0x148].min(9) {
        return Err(RomError::Size);
    }
    if rom[0x148] > 8 {
        return Err(RomError::Size);
    }
    if rom[0x104..0x134] != NINTENDO_LOGO {
        return Err(RomError::Logo);
    }
    let computed = rom[0x134..=0x14C]
        .iter()
        .fold(0u8, |acc, &b| acc.wrapping_sub(b).wrapping_sub(1));
    if computed != rom[0x14D] {
        return Err(RomError::Checksum);
    }
    Ok(())
}

/// The title per Appendix D.2 and OI-5: header bytes `$134`–`$143`
/// (`$134`–`$142` when `$143` has bit 7 set), cut at the first `$00`, bytes
/// outside `0x20`–`0x7E` replaced by `?`, trailing spaces removed; an empty
/// result falls back to the file name without its extension.
pub fn title_of(rom: &[u8], filename: &str) -> String {
    let end = if rom.get(0x143).is_some_and(|b| b & 0x80 != 0) {
        0x142
    } else {
        0x143
    };
    let mut title = String::new();
    for &b in &rom[0x134..=end] {
        if b == 0 {
            break;
        }
        title.push(if (0x20..=0x7F).contains(&b) {
            b as char
        } else {
            '?'
        });
    }
    let trimmed = title.trim_end_matches(' ');
    if trimmed.is_empty() {
        let stem = filename.rsplit('/').next().unwrap_or(filename);
        let stem = stem.rsplit_once('.').map_or(stem, |(s, _)| s);
        return stem.to_string();
    }
    trimmed.to_string()
}

impl Game {
    /// Build the metadata for a ROM that has passed [`validate_rom`].
    pub fn from_rom(rom: &[u8], filename: &str, added: u64) -> Game {
        let cart_type = rom[0x147];
        let flag = rom[0x143];
        let cgb = if flag & 0x80 == 0 {
            "none"
        } else if flag == 0xC0 {
            "only"
        } else {
            "dual"
        };
        let global_sum: u16 = rom
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != 0x14E && *i != 0x14F)
            .fold(0u16, |acc, (_, &b)| acc.wrapping_add(u16::from(b)));
        let global_expected = u16::from_be_bytes([rom[0x14E], rom[0x14F]]);
        let header_ok = rom[0x134..=0x14C]
            .iter()
            .fold(0u8, |acc, &b| acc.wrapping_sub(b).wrapping_sub(1))
            == rom[0x14D];
        Game {
            id: crate::sha256::hex_digest(rom),
            title: title_of(rom, filename),
            filename: filename.to_string(),
            size: rom.len(),
            cartridge_type: cart_type,
            mapper: mapper_name(cart_type),
            battery: is_battery(cart_type),
            rom_banks: rom.len() / 16384,
            ram_size: ram_size(rom[0x149]),
            cgb,
            sgb: rom[0x146] == 0x03,
            header_checksum_ok: header_ok,
            global_checksum_ok: global_sum == global_expected,
            playable: is_playable(cart_type),
            added,
        }
    }

    /// The metadata file body (line-oriented, `key=value`).
    pub fn to_meta(&self) -> String {
        format!(
            "id={}\ntitle={}\nfilename={}\nsize={}\ncartridge_type={}\nbattery={}\nram_size={}\ncgb={}\nsgb={}\nheader_checksum_ok={}\nglobal_checksum_ok={}\nplayable={}\nadded={}\n",
            self.id,
            self.title,
            self.filename,
            self.size,
            self.cartridge_type,
            u8::from(self.battery),
            self.ram_size,
            self.cgb,
            u8::from(self.sgb),
            u8::from(self.header_checksum_ok),
            u8::from(self.global_checksum_ok),
            u8::from(self.playable),
            self.added,
        )
    }

    /// Parse a metadata file body written by [`Self::to_meta`].
    pub fn from_meta(text: &str) -> Option<Game> {
        let mut id = None;
        let mut title = None;
        let mut filename = None;
        let mut size = None;
        let mut cartridge_type = None;
        let mut battery = None;
        let mut ram_size = None;
        let mut cgb = None;
        let mut sgb = None;
        let mut header_ok = None;
        let mut global_ok = None;
        let mut playable = None;
        let mut added = None;
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            match k {
                "id" => id = Some(v.to_string()),
                "title" => title = Some(v.to_string()),
                "filename" => filename = Some(v.to_string()),
                "size" => size = v.parse().ok(),
                "cartridge_type" => cartridge_type = v.parse().ok(),
                "battery" => battery = Some(v == "1"),
                "ram_size" => ram_size = v.parse().ok(),
                "cgb" => {
                    cgb = Some(match v {
                        "none" => "none",
                        "only" => "only",
                        _ => "dual",
                    })
                }
                "sgb" => sgb = Some(v == "1"),
                "header_checksum_ok" => header_ok = Some(v == "1"),
                "global_checksum_ok" => global_ok = Some(v == "1"),
                "playable" => playable = Some(v == "1"),
                "added" => added = v.parse().ok(),
                _ => {}
            }
        }
        let id: String = id?;
        let cartridge_type: u8 = cartridge_type?;
        Some(Game {
            id,
            title: title?,
            filename: filename?,
            size: size?,
            cartridge_type,
            mapper: mapper_name(cartridge_type),
            battery: battery?,
            rom_banks: size? / 16384,
            ram_size: ram_size?,
            cgb: cgb?,
            sgb: sgb?,
            header_checksum_ok: header_ok?,
            global_checksum_ok: global_ok?,
            playable: playable?,
            added: added?,
        })
    }

    /// The JSON object for this game (Appendix D.2).
    pub fn to_json(&self) -> String {
        format!(
            "{{\"id\":\"{}\",\"title\":\"{}\",\"filename\":\"{}\",\"size\":{},\"cartridge_type\":{},\"mapper\":\"{}\",\"battery\":{},\"rom_banks\":{},\"ram_size\":{},\"cgb\":\"{}\",\"sgb\":{},\"header_checksum_ok\":{},\"global_checksum_ok\":{},\"playable\":{},\"added\":{}}}",
            crate::json::escape(&self.id),
            crate::json::escape(&self.title),
            crate::json::escape(&self.filename),
            self.size,
            self.cartridge_type,
            self.mapper,
            self.battery,
            self.rom_banks,
            self.ram_size,
            self.cgb,
            self.sgb,
            self.header_checksum_ok,
            self.global_checksum_ok,
            self.playable,
            self.added,
        )
    }
}

/// The library's on-disk layout.
pub struct Store {
    root: PathBuf,
}

impl Store {
    /// Open (creating if needed) the library directory.
    pub fn open(root: &Path) -> io::Result<Store> {
        fs::create_dir_all(root.join("games"))?;
        fs::create_dir_all(root.join("saves"))?;
        Ok(Store {
            root: root.to_path_buf(),
        })
    }

    fn rom_path(&self, id: &str) -> PathBuf {
        self.root.join("games").join(format!("{id}.rom"))
    }

    fn meta_path(&self, id: &str) -> PathBuf {
        self.root.join("games").join(format!("{id}.meta"))
    }

    fn save_path(&self, id: &str) -> PathBuf {
        self.root.join("saves").join(format!("{id}.sav"))
    }

    /// Whether a game with this id already exists.
    pub fn has(&self, id: &str) -> bool {
        self.meta_path(id).is_file()
    }

    /// Load every game in the library.
    pub fn games(&self) -> Vec<Game> {
        let mut out = Vec::new();
        let dir = self.root.join("games");
        let Ok(entries) = fs::read_dir(&dir) else {
            return out;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("meta") {
                continue;
            }
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            if let Some(game) = Game::from_meta(&text) {
                // A metadata file without its ROM is not a complete game.
                if self.rom_path(&game.id).is_file() {
                    out.push(game);
                }
            }
        }
        out
    }

    /// Atomically store a new game (ROM first, then metadata as the commit).
    pub fn insert(&self, game: &Game, rom: &[u8]) -> io::Result<()> {
        write_atomic(&self.rom_path(&game.id), rom)?;
        write_atomic(&self.meta_path(&game.id), game.to_meta().as_bytes())?;
        Ok(())
    }

    /// Read a game's ROM.
    pub fn rom(&self, id: &str) -> Option<Vec<u8>> {
        fs::read(self.rom_path(id)).ok()
    }

    /// Delete a game and its save.
    pub fn delete(&self, id: &str) -> io::Result<()> {
        remove_if_exists(&self.meta_path(id))?;
        remove_if_exists(&self.rom_path(id))?;
        remove_if_exists(&self.save_path(id))?;
        Ok(())
    }

    /// Read a stored battery save.
    pub fn save(&self, id: &str) -> Option<Vec<u8>> {
        fs::read(self.save_path(id)).ok()
    }

    /// Store a battery save.
    pub fn put_save(&self, id: &str, data: &[u8]) -> io::Result<()> {
        write_atomic(&self.save_path(id), data)
    }

    /// Delete a battery save.
    pub fn delete_save(&self, id: &str) -> io::Result<bool> {
        if self.save_path(id).is_file() {
            remove_if_exists(&self.save_path(id))?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

fn write_atomic(path: &Path, data: &[u8]) -> io::Result<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, data)?;
    fs::rename(&tmp, path)
}

fn remove_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

/// Reduce a user-supplied file name to a safe display name (Security
/// Considerations). Returns `upload.gb` if nothing survives.
pub fn sanitize_filename(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let cleaned: String = base
        .chars()
        .filter(|c| !c.is_control() && *c != '\u{7f}')
        .collect();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        "upload.gb".to_string()
    } else {
        cleaned.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize() {
        assert_eq!(sanitize_filename("../etc/passwd"), "passwd");
        assert_eq!(sanitize_filename("a\\b\\c.gb"), "c.gb");
        assert_eq!(sanitize_filename("  "), "upload.gb");
        assert_eq!(sanitize_filename(""), "upload.gb");
    }

    #[test]
    fn title_fallbacks() {
        let mut rom = vec![0u8; 32 * 1024];
        rom[0x134] = b'A';
        rom[0x135] = b'B';
        assert_eq!(title_of(&rom, "x.gb"), "AB");
        rom[0x134] = 0x01; // non-printable -> '?'
        rom[0x135] = 0x20; // trailing space -> trimmed
        assert_eq!(title_of(&rom, "x.gb"), "?");
        rom[0x134] = 0;
        rom[0x135] = 0;
        assert_eq!(title_of(&rom, "dir/My Game.gbc"), "My Game");
    }
}
