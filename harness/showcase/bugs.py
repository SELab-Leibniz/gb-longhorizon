#!/usr/bin/env python3
"""Planted bugs for the v2 showcase: exact, unique one-place replacements.

    bugs.py REPO [BUG_ID ...]      apply the listed bugs (default: all)
"""
import sys
from pathlib import Path

BUGS = {
    "B01": ("gb-core/src/cartridge/mbc5.rs",
            "self.rom_bank = (self.rom_bank & 0x0FF) | (((value & 0x01) as u16) << 8)",
            "self.rom_bank = (self.rom_bank & 0x0FF) | (((value & 0x01) as u16) << 7)"),
    "B02": ("gb-core/src/cpu/opcodes.rs",
            "let h = (a & 0x0F) as u16 + (v & 0x0F) as u16 + c > 0x0F;",
            "let h = (a & 0x0F) as u16 + (v & 0x0F) as u16 > 0x0F;"),
    "B03": ("gb-core/src/cpu/opcodes.rs",
            "if c || (!n && a > 0x99) {",
            "if c || (!n && a >= 0x99) {"),
    "B04": ("gb-core/src/interrupts.rs",
            "Some(match both.trailing_zeros() {",
            "Some(match 7 - both.leading_zeros() {"),
    "B05": ("gb-core/src/ppu.rs",
            "self.bcps = 0x80 | ((self.bcps.wrapping_add(1)) & 0x3F);",
            "self.bcps = 0x80 | ((self.bcps.wrapping_add(1)) & 0x1F);"),
    "B06": ("gb-web/src/png.rs",
            "b = (b + a) % 65521;",
            "b = (b + a) % 65520;"),
    "B07": ("gb-web/src/store.rs",
            "title.push(if (0x20..=0x7E).contains(&b) {",
            "title.push(if (0x20..=0x7F).contains(&b) {"),
    "B08": ("gb-tools/src/bin/gb-trace.rs",
            "entries.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));",
            "entries.sort_by(|a, b| b.1.cmp(&a.1).then(b.0.cmp(&a.0)));"),
    "B09": ("gb-tools/src/bin/gb-server.rs",
            "bytes.push(emu.peek(addr.wrapping_add(i as u16)));",
            "bytes.push(emu.peek(addr.saturating_add(i as u16)));"),
    "B10": ("gb-wasm/src/lib.rs",
            "        a: mask & 0x10 != 0,\n        b: mask & 0x20 != 0,",
            "        a: mask & 0x20 != 0,\n        b: mask & 0x10 != 0,"),
    "B11": ("gb-core/src/cartridge/mbc1.rs",
            "        let lo = (self.bank_lo & 0x1F) as usize;\n        let lo = if lo == 0 { 1 } else { lo };\n        ((self.bank_hi as usize) << 5) | lo",
            "        let lo = (self.bank_lo & 0x1F) as usize;\n        let bank = ((self.bank_hi as usize) << 5) | lo;\n        if bank == 0 {\n            1\n        } else {\n            bank\n        }"),
    "B12": ("gb-web/src/main.rs",
            "if request.body_truncated || request.body.len() != game.ram_size {",
            "if request.body_truncated || request.body.len() < game.ram_size {"),
    "B13": ("gb-core/src/emulator.rs",
            "Some(flag) if flag & 0x80 != 0 => Model::Cgb,",
            "Some(flag) if *flag == 0xC0 => Model::Cgb,"),
}


def apply(repo: Path, ids):
    for bid in ids:
        rel, old, new = BUGS[bid]
        p = repo / rel
        s = p.read_text()
        if s.count(old) != 1:
            raise SystemExit(f"{bid}: expected exactly one match in {rel}, found {s.count(old)}")
        p.write_text(s.replace(old, new))


if __name__ == "__main__":
    repo = Path(sys.argv[1])
    ids = sys.argv[2:] or list(BUGS)
    apply(repo, ids)
    print("applied:", " ".join(ids))
