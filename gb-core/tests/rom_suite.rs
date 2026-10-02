//! ROM-driven acceptance suite.
//!
//! This file is **harness code**: it is complete and must not be weakened.
//! It discovers test ROMs under `roms/test/` and runs each with the
//! pass/fail convention of its family:
//!
//! * `roms/test/blargg/**/*.gb`  — the ROM prints to the serial port; pass
//!   when the output contains "Passed", fail on "Failed" or on timeout.
//!   Blargg's `cpu_instrs` combined ROM takes ~55 s of emulated time, so
//!   the budget is generous.
//! * `roms/test/mooneye/**/*.gb` — the ROM executes `LD B,B` when done;
//!   pass when B,C,D,E,H,L == 3,5,8,13,21,34 at that moment.
//! * `roms/test/blargg-mem/**/*.gb` — Blargg ROMs that report through
//!   cartridge RAM (`dmg_sound`, `oam_bug`): once $A001-$A003 hold the
//!   signature DE B0 61, $A000 is the status (0x80 = running, 0 = passed,
//!   anything else = failed) and $A004.. is the zero-terminated text output.
//! * `roms/test/mooneye-cgb/**` — Mooneye ROMs for the Game Boy Color, run
//!   with `Model::Cgb`, same `LD B,B` protocol.
//! * `roms/test/blargg-mem-cgb/**` — Blargg `cgb_sound`, memory protocol, CGB.
//! * `roms/test/cgb-acid2/`, `roms/test/mealybug-dmg/`
//!   — screenshot tests: run to `LD B,B`, hash the last completed frame
//!   (shades on DMG, RGB555 on CGB) and compare with `<rom>.fnv` next to the
//!   ROM (derived from the test's own reference screenshot).
//!
//! Families whose directory is absent are skipped, so suites delivered later
//! (e.g. with a change request) simply start running when their ROMs appear.
//! * `roms/test/acid2/dmg-acid2.gb` — run a fixed number of frames and
//!   compare the framebuffer FNV-1a hash with `roms/test/acid2/expected.fnv`.
//!
//! A panic inside the emulator (a `todo!()`, an index out of range, …) is
//! caught and reported as a failure for that ROM, so one broken opcode never
//! hides the results of the other 100 ROMs.
//!
//! Run everything:          `cargo test --release -p gb-core --test rom_suite`
//! See per-ROM detail:      add `-- --nocapture`
//! Run a single family:     `-- blargg` / `-- blargg_mem` / `-- mooneye` / `-- acid2`
//!                           `-- mooneye_cgb` / `-- blargg_mem_cgb` / `-- cgb_acid2`
//!                           `-- mealybug_dmg`
//! Skip a family (e.g. in CI without ROMs): set `GB_SKIP_ROMS=1`.

use gb_core::{Emulator, Model, StepResult};
use std::fs;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

const FRAMES_PER_SECOND: u64 = 60;
const BLARGG_BUDGET_FRAMES: u64 = 120 * FRAMES_PER_SECOND;
const MOONEYE_BUDGET_FRAMES: u64 = 20 * FRAMES_PER_SECOND;
const ACID2_FRAMES: u64 = 2 * FRAMES_PER_SECOND;
const SCREENSHOT_BUDGET_FRAMES: u64 = 20 * FRAMES_PER_SECOND;

fn roms_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("roms")
        .join("test")
}

fn collect_roms(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(collect_roms(&path));
        } else if path.extension().is_some_and(|e| e == "gb" || e == "gbc") {
            out.push(path);
        }
    }
    out.sort();
    out
}

#[derive(Debug)]
enum Outcome {
    Pass,
    Fail(String),
    Panic(String),
}

fn run_guarded(name: &str, f: impl FnOnce() -> Outcome) -> Outcome {
    // Capture the panic location ourselves and keep the default hook quiet,
    // so each ROM produces one readable PANIC line instead of a backtrace.
    let location: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let loc2 = Arc::clone(&location);
    let previous = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        if let Some(l) = info.location() {
            *loc2.lock().unwrap() = Some(format!("{}:{}", l.file(), l.line()));
        }
    }));
    let result = panic::catch_unwind(AssertUnwindSafe(f));
    panic::set_hook(previous);
    match result {
        Ok(outcome) => outcome,
        Err(payload) => {
            let msg = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "non-string panic".into());
            let at = location
                .lock()
                .unwrap()
                .clone()
                .map(|l| format!(" (at {l})"))
                .unwrap_or_default();
            Outcome::Panic(format!("{name}: {msg}{at}"))
        }
    }
}

/// Run a ROM to completion using the Blargg serial protocol.
fn run_blargg(rom: &[u8]) -> Outcome {
    let mut emu = match Emulator::load(rom) {
        Ok(e) => e,
        Err(e) => return Outcome::Fail(format!("load error: {e}")),
    };
    let mut serial = Vec::new();
    for _ in 0..BLARGG_BUDGET_FRAMES {
        emu.step_frame();
        serial.extend(emu.take_serial());
        let text = String::from_utf8_lossy(&serial);
        if text.contains("Passed") {
            return Outcome::Pass;
        }
        if text.contains("Failed") {
            return Outcome::Fail(format!("serial output:\n{}", text.trim()));
        }
    }
    Outcome::Fail(format!(
        "timed out after {} frames; serial so far:\n{}",
        BLARGG_BUDGET_FRAMES,
        String::from_utf8_lossy(&serial).trim()
    ))
}

/// Read the zero-terminated text a Blargg ROM writes at $A004.
fn blargg_mem_text(emu: &Emulator) -> String {
    let mut out = Vec::new();
    for addr in 0xA004u16..0xBFFF {
        let b = emu.peek(addr);
        if b == 0 {
            break;
        }
        out.push(b);
    }
    String::from_utf8_lossy(&out).trim().to_string()
}

/// Run a ROM to completion using Blargg's memory protocol.
fn run_blargg_mem(rom: &[u8], model: Model) -> Outcome {
    let mut emu = match Emulator::load_with_model(rom, model) {
        Ok(e) => e,
        Err(e) => return Outcome::Fail(format!("load error: {e}")),
    };
    for _ in 0..BLARGG_BUDGET_FRAMES {
        emu.step_frame();
        let signed = (emu.peek(0xA001), emu.peek(0xA002), emu.peek(0xA003)) == (0xDE, 0xB0, 0x61);
        if !signed {
            continue;
        }
        match emu.peek(0xA000) {
            0x80 => {}
            0x00 => return Outcome::Pass,
            code => {
                return Outcome::Fail(format!("status {code:#04x}:\n{}", blargg_mem_text(&emu)));
            }
        }
    }
    Outcome::Fail(format!(
        "timed out after {} frames; text so far:\n{}",
        BLARGG_BUDGET_FRAMES,
        blargg_mem_text(&emu)
    ))
}

/// Run a ROM to completion using the Mooneye `LD B,B` protocol.
fn run_mooneye(rom: &[u8], model: Model) -> Outcome {
    let mut emu = match Emulator::load_with_model(rom, model) {
        Ok(e) => e,
        Err(e) => return Outcome::Fail(format!("load error: {e}")),
    };
    let mut frames = 0u64;
    let mut cycles_in_frame = 0u32;
    while frames < MOONEYE_BUDGET_FRAMES {
        match emu.step_instruction() {
            StepResult::Ran(c) => {
                cycles_in_frame += c;
                if cycles_in_frame >= gb_core::CYCLES_PER_FRAME {
                    cycles_in_frame -= gb_core::CYCLES_PER_FRAME;
                    frames += 1;
                }
            }
            StepResult::Breakpoint => {
                let r = emu.registers();
                return if r.is_mooneye_pass() {
                    Outcome::Pass
                } else {
                    Outcome::Fail(format!(
                        "LD B,B reached with B={} C={} D={} E={} H={} L={} (want 3 5 8 13 21 34)",
                        r.b, r.c, r.d, r.e, r.h, r.l
                    ))
                };
            }
        }
    }
    Outcome::Fail(format!("no LD B,B within {MOONEYE_BUDGET_FRAMES} frames"))
}

/// The bytes a frame hash covers: shades (DMG) or RGB555 little-endian (CGB).
fn frame_bytes(emu: &Emulator) -> Vec<u8> {
    match emu.model() {
        Model::Dmg => emu.framebuffer().to_vec(),
        Model::Cgb => gb_core::util::rgb555_bytes(emu.framebuffer_rgb555()),
    }
}

/// Run to the `LD B,B` breakpoint and compare the last completed frame with
/// the expected hash stored next to the ROM as `<stem>.fnv`.
fn run_screenshot(path: &Path, rom: &[u8], model: Model) -> Outcome {
    let expected_path = path.with_extension("fnv");
    let expected = match fs::read_to_string(&expected_path)
        .ok()
        .and_then(|t| u64::from_str_radix(t.trim(), 16).ok())
    {
        Some(h) => h,
        None => return Outcome::Fail(format!("missing or bad {}", expected_path.display())),
    };
    let mut emu = match Emulator::load_with_model(rom, model) {
        Ok(e) => e,
        Err(e) => return Outcome::Fail(format!("load error: {e}")),
    };
    let mut frames = 0u64;
    let mut cycles_in_frame = 0u32;
    while frames < SCREENSHOT_BUDGET_FRAMES {
        match emu.step_instruction() {
            StepResult::Ran(c) => {
                cycles_in_frame += c;
                if cycles_in_frame >= gb_core::CYCLES_PER_FRAME {
                    cycles_in_frame -= gb_core::CYCLES_PER_FRAME;
                    frames += 1;
                }
            }
            StepResult::Breakpoint => {
                let got = gb_core::util::fnv1a64(&frame_bytes(&emu));
                return if got == expected {
                    Outcome::Pass
                } else {
                    Outcome::Fail(format!("frame hash {got:016x}, expected {expected:016x}"))
                };
            }
        }
    }
    Outcome::Fail(format!(
        "no LD B,B within {SCREENSHOT_BUDGET_FRAMES} frames"
    ))
}

/// Run dmg-acid2 and compare the frame hash.
fn run_acid2(rom: &[u8], expected: u64) -> Outcome {
    let mut emu = match Emulator::load(rom) {
        Ok(e) => e,
        Err(e) => return Outcome::Fail(format!("load error: {e}")),
    };
    for _ in 0..ACID2_FRAMES {
        emu.step_frame();
    }
    let got = gb_core::util::fnv1a64(emu.framebuffer());
    if got == expected {
        Outcome::Pass
    } else {
        Outcome::Fail(format!("frame hash {got:016x}, expected {expected:016x}"))
    }
}

struct Summary {
    passed: Vec<String>,
    failed: Vec<(String, String)>,
}

fn run_family(family: &str, runner: impl Fn(&Path, &[u8]) -> Outcome) -> Option<Summary> {
    if std::env::var_os("GB_SKIP_ROMS").is_some() {
        eprintln!("[{family}] skipped (GB_SKIP_ROMS set)");
        return None;
    }
    let dir = roms_root().join(family);
    let roms = collect_roms(&dir);
    if roms.is_empty() {
        eprintln!(
            "[{family}] no ROMs found under {} — run harness/scripts/fetch_assets.sh",
            dir.display()
        );
        return None;
    }

    let mut summary = Summary {
        passed: Vec::new(),
        failed: Vec::new(),
    };
    let started = Instant::now();
    for path in &roms {
        let rel = path
            .strip_prefix(&dir)
            .unwrap_or(path)
            .display()
            .to_string();
        let bytes = fs::read(path).expect("read ROM");
        let t = Instant::now();
        let outcome = run_guarded(&rel, || runner(path, &bytes));
        let secs = t.elapsed().as_secs_f32();
        match outcome {
            Outcome::Pass => {
                eprintln!("  PASS  {rel}  ({secs:.1}s)");
                summary.passed.push(rel);
            }
            Outcome::Fail(why) => {
                eprintln!(
                    "  FAIL  {rel}  ({secs:.1}s)\n        {}",
                    why.replace('\n', "\n        ")
                );
                summary.failed.push((rel, why));
            }
            Outcome::Panic(why) => {
                eprintln!(
                    "  PANIC {rel}  ({secs:.1}s)\n        {}",
                    why.replace('\n', "\n        ")
                );
                summary.failed.push((rel, format!("panic: {why}")));
            }
        }
    }
    eprintln!(
        "[{family}] {}/{} passed in {:.1}s",
        summary.passed.len(),
        roms.len(),
        started.elapsed().as_secs_f32()
    );
    Some(summary)
}

fn assert_all_passed(family: &str, summary: Option<Summary>) {
    let Some(s) = summary else { return };
    assert!(
        s.failed.is_empty(),
        "[{family}] {} ROM(s) failed:\n{}",
        s.failed.len(),
        s.failed
            .iter()
            .map(|(name, _)| format!("  - {name}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn blargg() {
    assert_all_passed("blargg", run_family("blargg", |_, rom| run_blargg(rom)));
}

#[test]
fn blargg_mem() {
    assert_all_passed(
        "blargg-mem",
        run_family("blargg-mem", |_, rom| run_blargg_mem(rom, Model::Dmg)),
    );
}

#[test]
fn mooneye() {
    assert_all_passed(
        "mooneye",
        run_family("mooneye", |_, rom| run_mooneye(rom, Model::Dmg)),
    );
}

#[test]
fn mooneye_cgb() {
    assert_all_passed(
        "mooneye-cgb",
        run_family("mooneye-cgb", |_, rom| run_mooneye(rom, Model::Cgb)),
    );
}

#[test]
fn blargg_mem_cgb() {
    assert_all_passed(
        "blargg-mem-cgb",
        run_family("blargg-mem-cgb", |_, rom| run_blargg_mem(rom, Model::Cgb)),
    );
}

#[test]
fn cgb_acid2() {
    assert_all_passed(
        "cgb-acid2",
        run_family("cgb-acid2", |p, rom| run_screenshot(p, rom, Model::Cgb)),
    );
}

#[test]
fn mealybug_dmg() {
    assert_all_passed(
        "mealybug-dmg",
        run_family("mealybug-dmg", |p, rom| run_screenshot(p, rom, Model::Dmg)),
    );
}
    let dir = roms_root().join("acid2");
    let rom_path = dir.join("dmg-acid2.gb");
    let expected_path = dir.join("expected.fnv");
    if !rom_path.exists() || !expected_path.exists() {
        eprintln!(
            "[acid2] missing {} or {} — run harness/scripts/fetch_assets.sh",
            rom_path.display(),
            expected_path.display()
        );
        return;
    }
    let expected = u64::from_str_radix(fs::read_to_string(&expected_path).unwrap().trim(), 16)
        .expect("expected.fnv holds a 16-hex-digit FNV-1a hash");
    let rom = fs::read(&rom_path).unwrap();
    match run_guarded("dmg-acid2", || run_acid2(&rom, expected)) {
        Outcome::Pass => eprintln!("[acid2] PASS"),
        Outcome::Fail(why) | Outcome::Panic(why) => panic!("[acid2] FAIL: {why}"),
    }
}
