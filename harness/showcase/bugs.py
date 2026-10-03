#!/usr/bin/env python3
"""Planted bugs for the v2 showcase: exact, unique one-place replacements.

    bugs.py REPO [BUG_ID ...]      apply the listed bugs (default: all)
    bugs.py REPO --docs            apply only DOC_EDITS (comments and a unit test
                                   that gave planted bugs away)

Every bug is in the code from the start of the run; its issue is filed in the
wave given in WAVE (0 = at the start, 1-4 = released during the run, see
waves/schedule.json). No bug sits in a file that also holds a stub.
"""
import sys
from pathlib import Path

BUGS = {
    # ---- wave 0: reported at the start ------------------------------------
    "B01": ("gb-core/src/cartridge/mbc5.rs",
            "self.rom_bank = (self.rom_bank & 0x0FF) | (((value & 0x01) as u16) << 8)",
            "self.rom_bank = (self.rom_bank & 0x0FF) | (((value & 0x01) as u16) << 7)"),
    "B04": ("gb-core/src/interrupts.rs",
            "Some(match both.trailing_zeros() {",
            "Some(match 7 - both.leading_zeros() {"),
    "B06": ("gb-web/src/png.rs",
            "b = (b + a) % 65521;",
            "b = (b + a) % 65520;"),
    "B07": ("gb-web/src/store.rs",
            "title.push(if (0x20..=0x7E).contains(&b) {",
            "title.push(if (0x20..=0x7F).contains(&b) {"),
    "B08": ("gb-tools/src/bin/gb-trace.rs",
            "entries.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));",
            "entries.sort_by(|a, b| b.1.cmp(&a.1).then(b.0.cmp(&a.0)));"),
    "B11": ("gb-core/src/cartridge/mbc1.rs",
            "        let lo = (self.bank_lo & 0x1F) as usize;\n        let lo = if lo == 0 { 1 } else { lo };\n        ((self.bank_hi as usize) << 5) | lo",
            "        let lo = (self.bank_lo & 0x1F) as usize;\n        let bank = ((self.bank_hi as usize) << 5) | lo;\n        if bank == 0 {\n            1\n        } else {\n            bank\n        }"),
    "B13": ("gb-core/src/emulator.rs",
            "Some(flag) if flag & 0x80 != 0 => Model::Cgb,",
            "Some(flag) if *flag == 0xC0 => Model::Cgb,"),
    # ---- wave 1 -------------------------------------------------------------
    # joypad interrupt on release instead of press: a HALT-until-key loop resumes late
    "B14": ("gb-core/src/joypad.rs",
            "        if before & !after != 0 {",
            "        if after & !before != 0 {"),
    # a double quote in a title is not escaped: one upload breaks every listing
    "B15": ("gb-web/src/json.rs",
            "            '\"' => out.push_str(\"\\\\\\\"\"),\n            '\\\\' =>",
            "            '\\\\' =>"),
    # ---- wave 2 -------------------------------------------------------------
    # stored metadata split at the last '=': a game whose file name or title contains '='
    # loses that field when the library is read back, and drops out of every listing
    "B16": ("gb-web/src/store.rs",
            "            let Some((k, v)) = line.split_once('=') else {",
            "            let Some((k, v)) = line.rsplit_once('=') else {"),
    # deleting a game leaves its battery save: re-uploading the game brings the old save back
    "B17": ("gb-web/src/store.rs",
            "        remove_if_exists(&self.rom_path(id))?;\n        remove_if_exists(&self.save_path(id))?;\n        Ok(())",
            "        remove_if_exists(&self.rom_path(id))?;\n        Ok(())"),
    # ---- wave 3 -------------------------------------------------------------
    # a short read timeout: uploads over a slow link are cut off
    "B18": ("gb-web/src/http.rs",
            "const READ_TIMEOUT: Duration = Duration::from_secs(60);",
            "const READ_TIMEOUT: Duration = Duration::from_millis(750);"),
    # a %XX escape in the last three bytes of a query is not decoded: searching "C++" finds nothing
    "B19": ("gb-web/src/http.rs",
            "b'%' if i + 2 < bytes.len() =>",
            "b'%' if i + 3 < bytes.len() =>"),
    # ---- wave 4 -------------------------------------------------------------
    # 64 KiB stored-deflate blocks: LEN wraps to 0, so PNGs over 64 KiB (CGB screenshots) are corrupt
    "B20": ("gb-web/src/png.rs",
            "let mut chunks = data.chunks(65535).peekable();",
            "let mut chunks = data.chunks(65536).peekable();"),
}

WAVE = {"B01": 0, "B04": 0, "B06": 0, "B07": 0, "B08": 0, "B11": 0, "B13": 0,
        "B14": 1, "B15": 1, "B16": 2, "B17": 2, "B18": 3, "B19": 3, "B20": 4}

# Comments in the inherited code that stated the correct behaviour right next
# to a planted bug, and unit tests that would have pointed straight at one
# (removed, or made to miss the edge case).
# Applied once to the inherited tree; not part of any bug.
DOC_EDITS = [
    ("gb-core/src/emulator.rs",
     "    /// The model a cartridge asks for: CGB if header byte 0x143 has bit 7 set\n"
     "    /// (0x80 = works on both, 0xC0 = CGB only), otherwise DMG.\n",
     "    /// The model a cartridge asks for (`--model auto`, GEP 1 Appendix D.6).\n"),
    ("gb-web/src/store.rs",
     "    /// Delete a game and its save.\n",
     "    /// Delete a game.\n"),
    ("gb-web/src/http.rs",
     '        let params = parse_query("q=hello+world&cgb=none&x=%41%2f");\n',
     '        let params = parse_query("q=hello+world&cgb=none&x=%41%2fB");\n'),
    ("gb-web/src/http.rs",
     '        assert_eq!(params[2], ("x".to_string(), "A/".to_string()));\n',
     '        assert_eq!(params[2], ("x".to_string(), "A/B".to_string()));\n'),
    ("gb-web/src/json.rs",
     "\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn escaping() {\n"
     "        assert_eq!(escape(\"a\\\"b\\\\c\"), \"a\\\\\\\"b\\\\\\\\c\");\n"
     "        assert_eq!(escape(\"x\\u{1}\"), \"x\\\\u0001\");\n"
     "        assert_eq!(\n            error_body(\"bad\", \"bad_request\"),\n"
     "            \"{\\\"error\\\":\\\"bad\\\",\\\"code\\\":\\\"bad_request\\\"}\"\n        );\n    }\n}\n",
     ""),
]


def replace_once(p: Path, old: str, new: str, what: str):
    s = p.read_text()
    if s.count(old) != 1:
        raise SystemExit(f"{what}: expected exactly one match in {p}, found {s.count(old)}")
    p.write_text(s.replace(old, new))


def apply(repo: Path, ids):
    for bid in ids:
        rel, old, new = BUGS[bid]
        replace_once(repo / rel, old, new, bid)


def apply_docs(repo: Path):
    for rel, old, new in DOC_EDITS:
        replace_once(repo / rel, old, new, f"doc edit in {rel}")


if __name__ == "__main__":
    repo = Path(sys.argv[1])
    if sys.argv[2:] == ["--docs"]:
        apply_docs(repo)
        print("doc edits applied")
    else:
        ids = sys.argv[2:] or list(BUGS)
        apply(repo, ids)
        print("applied:", " ".join(ids))
