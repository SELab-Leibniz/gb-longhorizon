//! `gb` — headless command-line runner.
//!
//! This binary is **harness code**: complete, and the interface the
//! grading scripts call. Do not change flag names or output formats.
//!
//! ```text
//! gb --rom GAME.gb [--frames N] [--input-script FILE] [--dump-frame PATH]
//!    [--dump-every N --dump-dir DIR] [--serial-stdout] [--hash]
//!    [--mooneye] [--blargg-mem] [--save-state PATH] [--load-state PATH]
//!    [--model dmg|cgb|auto]
//! ```
//!
//! DMG frames are 2-bit shades (PGM dumps, hash over the shade bytes); CGB
//! frames are RGB555 (PPM dumps, hash over the little-endian u16 pixels).
//!
//! Exit codes: 0 ok · 1 usage/IO error · 2 emulator panic ·
//! 10 Mooneye pass · 11 Mooneye fail (only with --mooneye) ·
//! 20 Blargg pass · 21 Blargg fail (only with --blargg-mem).
//!
//! Input script format (one directive per line, `#` comments):
//! ```text
//! 120 START          # from frame 120, hold START
//! 130                # from frame 130, release everything
//! 200 A,RIGHT        # hold A and RIGHT together
//! ```
//! Frames are absolute, must be ascending. Buttons stay held until the next
//! line. Names: UP DOWN LEFT RIGHT A B SELECT START (case-insensitive).

use gb_core::util::{fnv1a64, framebuffer_rgb555_to_ppm, framebuffer_to_pgm, rgb555_bytes};
use gb_core::{Buttons, Emulator, Model, StepResult, SCREEN_HEIGHT, SCREEN_WIDTH};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

struct Args {
    rom: PathBuf,
    frames: u64,
    input_script: Option<PathBuf>,
    dump_frame: Option<PathBuf>,
    dump_every: Option<u64>,
    dump_dir: PathBuf,
    serial_stdout: bool,
    hash: bool,
    mooneye: bool,
    blargg_mem: bool,
    model: String,
    save_state: Option<PathBuf>,
    load_state: Option<PathBuf>,
}

const USAGE: &str = "\
usage: gb --rom FILE [options]
  --frames N           run N frames (default 600)
  --input-script FILE  scripted button presses (see source for format)
  --dump-frame PATH    write final frame as PGM
  --dump-every N       write a PGM every N frames into --dump-dir
  --dump-dir DIR       directory for --dump-every (default .)
  --serial-stdout      echo serial-port output to stdout
  --hash               print FNV-1a hash of each dumped frame + final frame
  --mooneye            stop at LD B,B; exit 10 on pass, 11 on fail
  --blargg-mem         stop when a Blargg ROM reports via $A000; exit 20 pass, 21 fail
  --model M            dmg (default), cgb, or auto (CGB if the cartridge supports it)
  --save-state PATH    write save state after the run
  --load-state PATH    restore save state before the run
";

fn parse_args() -> Result<Args, String> {
    let mut a = Args {
        rom: PathBuf::new(),
        frames: 600,
        input_script: None,
        dump_frame: None,
        dump_every: None,
        dump_dir: PathBuf::from("."),
        serial_stdout: false,
        hash: false,
        mooneye: false,
        blargg_mem: false,
        model: "dmg".to_string(),
        save_state: None,
        load_state: None,
    };
    let mut it = std::env::args().skip(1);
    let mut have_rom = false;
    while let Some(flag) = it.next() {
        let mut value = |name: &str| -> Result<String, String> {
            it.next().ok_or_else(|| format!("{name} needs a value"))
        };
        match flag.as_str() {
            "--rom" => {
                a.rom = value("--rom")?.into();
                have_rom = true;
            }
            "--frames" => {
                a.frames = value("--frames")?
                    .parse()
                    .map_err(|e| format!("--frames: {e}"))?
            }
            "--input-script" => a.input_script = Some(value("--input-script")?.into()),
            "--dump-frame" => a.dump_frame = Some(value("--dump-frame")?.into()),
            "--dump-every" => {
                a.dump_every = Some(
                    value("--dump-every")?
                        .parse()
                        .map_err(|e| format!("--dump-every: {e}"))?,
                )
            }
            "--dump-dir" => a.dump_dir = value("--dump-dir")?.into(),
            "--serial-stdout" => a.serial_stdout = true,
            "--hash" => a.hash = true,
            "--mooneye" => a.mooneye = true,
            "--blargg-mem" => a.blargg_mem = true,
            "--model" => a.model = value("--model")?,
            "--save-state" => a.save_state = Some(value("--save-state")?.into()),
            "--load-state" => a.load_state = Some(value("--load-state")?.into()),
            "-h" | "--help" => return Err(USAGE.to_string()),
            other => return Err(format!("unknown flag `{other}`\n{USAGE}")),
        }
    }
    if !have_rom {
        return Err(format!("--rom is required\n{USAGE}"));
    }
    Ok(a)
}

/// (frame, buttons) pairs, ascending by frame.
fn parse_input_script(path: &Path) -> Result<Vec<(u64, Buttons)>, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut out = Vec::new();
    let mut last = None;
    for (lineno, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let (frame_s, buttons_s) = match line.split_once(char::is_whitespace) {
            Some((f, b)) => (f, b.trim()),
            None => (line, ""),
        };
        let frame: u64 = frame_s.parse().map_err(|_| {
            format!(
                "{}:{}: bad frame number `{frame_s}`",
                path.display(),
                lineno + 1
            )
        })?;
        if last.is_some_and(|l| frame <= l) {
            return Err(format!(
                "{}:{}: frames must be strictly ascending",
                path.display(),
                lineno + 1
            ));
        }
        last = Some(frame);
        let buttons = Buttons::parse_list(buttons_s)
            .map_err(|e| format!("{}:{}: {e}", path.display(), lineno + 1))?;
        out.push((frame, buttons));
    }
    Ok(out)
}

/// The bytes a frame hash is computed over: shades (DMG) or RGB555 LE (CGB).
fn frame_bytes(emu: &Emulator) -> Vec<u8> {
    match emu.model() {
        Model::Dmg => emu.framebuffer().to_vec(),
        Model::Cgb => rgb555_bytes(emu.framebuffer_rgb555()),
    }
}

/// File extension for frame dumps: PGM (DMG shades) or PPM (CGB colour).
fn frame_ext(emu: &Emulator) -> &'static str {
    match emu.model() {
        Model::Dmg => "pgm",
        Model::Cgb => "ppm",
    }
}

fn write_frame(path: &Path, emu: &Emulator) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
    }
    let data = match emu.model() {
        Model::Dmg => framebuffer_to_pgm(emu.framebuffer(), SCREEN_WIDTH, SCREEN_HEIGHT),
        Model::Cgb => {
            framebuffer_rgb555_to_ppm(emu.framebuffer_rgb555(), SCREEN_WIDTH, SCREEN_HEIGHT)
        }
    };
    fs::write(path, data).map_err(|e| format!("{}: {e}", path.display()))
}

fn run(args: Args) -> Result<u8, String> {
    let rom = fs::read(&args.rom).map_err(|e| format!("{}: {e}", args.rom.display()))?;
    let model = match args.model.as_str() {
        "dmg" => Model::Dmg,
        "cgb" => Model::Cgb,
        "auto" => Model::for_rom(&rom),
        other => return Err(format!("--model must be dmg, cgb or auto (got `{other}`)")),
    };
    let mut emu = Emulator::load_with_model(&rom, model)
        .map_err(|e| format!("{}: {e}", args.rom.display()))?;

    if let Some(p) = &args.load_state {
        let state = fs::read(p).map_err(|e| format!("{}: {e}", p.display()))?;
        emu.load_state(&state)
            .map_err(|e| format!("{}: {e}", p.display()))?;
    }

    let script = match &args.input_script {
        Some(p) => parse_input_script(p)?,
        None => Vec::new(),
    };
    let mut script_idx = 0usize;

    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let mut exit = 0u8;

    for frame in 0..args.frames {
        while script_idx < script.len() && script[script_idx].0 == frame {
            emu.set_buttons(script[script_idx].1);
            script_idx += 1;
        }

        if args.mooneye {
            // Step instruction-by-instruction so the breakpoint is exact.
            let mut cycles = 0u32;
            let mut done = false;
            while cycles < gb_core::CYCLES_PER_FRAME {
                match emu.step_instruction() {
                    StepResult::Ran(c) => cycles += c,
                    StepResult::Breakpoint => {
                        let r = emu.registers();
                        let pass = r.is_mooneye_pass();
                        writeln!(
                            out,
                            "mooneye: LD B,B at frame {frame} B={} C={} D={} E={} H={} L={} → {}",
                            r.b,
                            r.c,
                            r.d,
                            r.e,
                            r.h,
                            r.l,
                            if pass { "PASS" } else { "FAIL" }
                        )
                        .ok();
                        // Screenshot-based tests (Mealybug Tearoom, cgb-acid2)
                        // compare the last completed frame at this breakpoint.
                        writeln!(
                            out,
                            "mooneye: frame-hash {:016x}",
                            fnv1a64(&frame_bytes(&emu))
                        )
                        .ok();
                        if let Some(p) = &args.dump_frame {
                            write_frame(p, &emu)?;
                        }
                        exit = if pass { 10 } else { 11 };
                        done = true;
                        break;
                    }
                }
            }
            if done {
                break;
            }
        } else {
            emu.step_frame();
        }

        if args.blargg_mem
            && (emu.peek(0xA001), emu.peek(0xA002), emu.peek(0xA003)) == (0xDE, 0xB0, 0x61)
        {
            let status = emu.peek(0xA000);
            if status != 0x80 {
                let mut text = Vec::new();
                for addr in 0xA004u16..0xBFFF {
                    match emu.peek(addr) {
                        0 => break,
                        b => text.push(b),
                    }
                }
                writeln!(
                    out,
                    "blargg-mem: status {status:#04x} at frame {frame} → {}\n{}",
                    if status == 0 { "PASS" } else { "FAIL" },
                    String::from_utf8_lossy(&text).trim()
                )
                .ok();
                exit = if status == 0 { 20 } else { 21 };
                break;
            }
        }

        if args.serial_stdout {
            let bytes = emu.take_serial();
            if !bytes.is_empty() {
                out.write_all(&bytes).ok();
                out.flush().ok();
            }
        }

        if let Some(every) = args.dump_every {
            if every > 0 && (frame + 1) % every == 0 {
                let path =
                    args.dump_dir
                        .join(format!("frame_{:06}.{}", frame + 1, frame_ext(&emu)));
                write_frame(&path, &emu)?;
                if args.hash {
                    writeln!(
                        out,
                        "frame {:06} {:016x}",
                        frame + 1,
                        fnv1a64(&frame_bytes(&emu))
                    )
                    .ok();
                }
            }
        }
    }

    if args.blargg_mem && exit == 0 {
        writeln!(
            out,
            "blargg-mem: no result within {} frames → FAIL",
            args.frames
        )
        .ok();
        exit = 21;
    }

    if args.mooneye && exit == 0 {
        writeln!(
            out,
            "mooneye: no LD B,B within {} frames → FAIL",
            args.frames
        )
        .ok();
        exit = 11;
    }

    if let Some(p) = &args.dump_frame {
        write_frame(p, &emu)?;
    }
    if args.hash {
        writeln!(out, "final {:016x}", fnv1a64(&frame_bytes(&emu))).ok();
    }
    if let Some(p) = &args.save_state {
        fs::write(p, emu.save_state()).map_err(|e| format!("{}: {e}", p.display()))?;
    }
    Ok(exit)
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(msg) => {
            eprintln!("{msg}");
            return ExitCode::from(1);
        }
    };
    match std::panic::catch_unwind(|| run(args)) {
        Ok(Ok(code)) => ExitCode::from(code),
        Ok(Err(msg)) => {
            eprintln!("error: {msg}");
            ExitCode::from(1)
        }
        Err(_) => {
            eprintln!("error: emulator panicked (see message above)");
            ExitCode::from(2)
        }
    }
}
