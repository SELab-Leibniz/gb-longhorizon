#!/usr/bin/env python3
"""Crafted Game Boy ROMs for the library-service and front-end tests.

Every ROM has a correct Nintendo logo and header checksum unless asked
otherwise, boots to `NOP; JP $0150` and loops on `JR -2`, and carries a
`tag` in its body so ROMs are distinct (distinct SHA-256).
"""
from __future__ import annotations

import hashlib

LOGO = bytes.fromhex(
    "CEED6666CC0D000B03730083000C000D0008111F8889000EDCCC6EE6DDDDD999BBBB67636E0EECCCDDDC999FBBB9333E")


def header_checksum(rom: bytes | bytearray) -> int:
    x = 0
    for b in rom[0x134:0x14D]:
        x = (x - b - 1) & 0xFF
    return x


def global_checksum(rom: bytes | bytearray) -> int:
    return (sum(rom) - rom[0x14E] - rom[0x14F]) & 0xFFFF


def make_rom(title: bytes = b"TEST", cart_type: int = 0x00, size_code: int = 0, ram_code: int = 0,
             cgb: int = 0x00, sgb: int = 0x00, tag: bytes = b"", logo: bool = True,
             good_header: bool = True, good_global: bool = True) -> bytes:
    rom = bytearray(b"\xff" * ((32 * 1024) << size_code))
    rom[0x100:0x104] = bytes([0x00, 0xC3, 0x50, 0x01])        # NOP; JP $0150
    rom[0x150:0x152] = bytes([0x18, 0xFE])                    # JR -2
    rom[0x104:0x134] = LOGO if logo else bytes(48)
    t = title[:16]
    rom[0x134:0x144] = t + bytes(16 - len(t))
    if cgb:
        rom[0x143] = cgb
    rom[0x146] = sgb
    rom[0x147] = cart_type
    rom[0x148] = size_code
    rom[0x149] = ram_code
    rom[0x200:0x200 + len(tag)] = tag
    rom[0x14D] = header_checksum(rom) ^ (0 if good_header else 0x5A)
    g = global_checksum(rom) ^ (0 if good_global else 0x1234)
    rom[0x14E], rom[0x14F] = g >> 8, g & 0xFF
    return bytes(rom)


def sha256(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


MAPPERS = [((0x00, 0x08, 0x09), "ROM"), ((0x01, 0x02, 0x03), "MBC1"), ((0x05, 0x06), "MBC2"),
           ((0x0B, 0x0C, 0x0D), "MMM01"), (tuple(range(0x0F, 0x14)), "MBC3"),
           (tuple(range(0x19, 0x1F)), "MBC5"), ((0x20,), "MBC6"), ((0x22,), "MBC7"),
           ((0xFC,), "CAMERA"), ((0xFD,), "TAMA5"), ((0xFE,), "HUC3"), ((0xFF,), "HUC1")]
BATTERY = {0x03, 0x06, 0x09, 0x0D, 0x0F, 0x10, 0x13, 0x1B, 0x1E, 0x22, 0xFF}
PLAYABLE = {0x00, 0x01, 0x02, 0x03, 0x08, 0x09, 0x0F, 0x10, 0x11, 0x12, 0x13,
            0x19, 0x1A, 0x1B, 0x1C, 0x1D, 0x1E}
RAM_SIZES = {0: 0, 1: 2048, 2: 8192, 3: 32768, 4: 131072, 5: 65536}


def expected_game(rom: bytes, filename: str) -> dict:
    """The game object GEP 1 Appendix D.2 (+ the OI-5 answer) prescribes, minus `added`."""
    t = rom[0x134:0x143] if rom[0x143] & 0x80 else rom[0x134:0x144]
    t = t.split(b"\x00", 1)[0].rstrip(b" ")
    title = "".join(chr(b) if 0x20 <= b <= 0x7E else "?" for b in t)
    if not title:
        title = filename.rsplit(".", 1)[0] if "." in filename else filename
    ct = rom[0x147]
    mapper = next((name for types, name in MAPPERS if ct in types), "UNKNOWN")
    cgbf = rom[0x143]
    return {
        "id": sha256(rom), "title": title, "filename": filename, "size": len(rom),
        "cartridge_type": ct, "mapper": mapper, "battery": ct in BATTERY,
        "rom_banks": len(rom) // 16384, "ram_size": RAM_SIZES.get(rom[0x149], 0),
        "cgb": "none" if not cgbf & 0x80 else ("only" if cgbf == 0xC0 else "dual"),
        "sgb": rom[0x146] == 0x03,
        "header_checksum_ok": header_checksum(rom) == rom[0x14D],
        "global_checksum_ok": global_checksum(rom) == (rom[0x14E] << 8 | rom[0x14F]),
        "playable": ct in PLAYABLE,
    }


if __name__ == "__main__":
    r = make_rom(b"HELLO", tag=b"x")
    assert len(r) == 32768 and header_checksum(r) == r[0x14D] and global_checksum(r) == (r[0x14E] << 8 | r[0x14F])
    print(expected_game(r, "hello.gb"))
