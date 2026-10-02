//! `gb` — headless command-line runner.
//!
//! This binary is **harness code**: complete, and the interface the
//! grading scripts call. Do not change flag names or output formats.
//!
//! ```text
//! gb --rom GAME.gb [--frames N] [--input-script FILE] [--dump-frame PATH]
//!    [--dump-every N --dump-dir DIR] [--serial-stdout] [--hash]
//!    [--mooneye] [--save-state PATH] [--load-state PATH]
//! ```
//!
//! Exit codes: 0 ok · 1 usage/IO error · 2 emulator panic ·
//! 10 Mooneye pass · 11 Mooneye fail (only with --mooneye).
//!
//! Input script format (one directive per line, `#` comments):
//! ```text
//! 120 START          # from frame 120, hold START
//! 130                # from frame 130, release everything
//! 200 A,RIGHT        # hold A and RIGHT together
//! ```
//! Frames are absolute, must be ascending. Buttons stay held until the next
//! line. Names: UP DOWN LEFT RIGHT A B SELECT START (case-insensitive).

use gb_core::util::{fnv1a64, framebuffer_to_pgm};
use gb_core::{Buttons, Emulator, StepResult, SCREEN_HEIGHT, SCREEN_WIDTH};
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

fn write_pgm(path: &Path, fb: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
    }
    fs::write(path, framebuffer_to_pgm(fb, SCREEN_WIDTH, SCREEN_HEIGHT))
        .map_err(|e| format!("{}: {e}", path.display()))
}

fn run(args: Args) -> Result<u8, String> {
    let rom = fs::read(&args.rom).map_err(|e| format!("{}: {e}", args.rom.display()))?;
    let mut emu = Emulator::load(&rom).map_err(|e| format!("{}: {e}", args.rom.display()))?;

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

        if args.serial_stdout {
            let bytes = emu.take_serial();
            if !bytes.is_empty() {
                out.write_all(&bytes).ok();
                out.flush().ok();
            }
        }

        if let Some(every) = args.dump_every {
            if every > 0 && (frame + 1) % every == 0 {
                let path = args.dump_dir.join(format!("frame_{:06}.pgm", frame + 1));
                write_pgm(&path, emu.framebuffer())?;
                if args.hash {
                    writeln!(
                        out,
                        "frame {:06} {:016x}",
                        frame + 1,
                        fnv1a64(emu.framebuffer())
                    )
                    .ok();
                }
            }
        }
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
        write_pgm(p, emu.framebuffer())?;
    }
    if args.hash {
        writeln!(out, "final {:016x}", fnv1a64(emu.framebuffer())).ok();
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
