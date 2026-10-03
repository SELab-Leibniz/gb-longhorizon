//! `gb-trace` — record or profile the first N executed instructions.
//!
//! ```text
//! gb-trace --rom PATH [--model dmg|cgb] --instructions N [--doctor] [--output FILE]
//! gb-trace --rom PATH [--model dmg|cgb] --instructions N [--doctor] --profile [--top K]
//! ```
//!
//! One line per executed instruction is written *before* it runs, starting at
//! `PC=$0100`; see GEP-0001 Appendix A for the exact format. Exit codes: 0 on
//! success, 1 on usage/IO errors, 2 if the emulator panics.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufWriter, Write};

use gb_core::{Emulator, Model, StepResult};
use gb_tools::{parse_model, trace_line};

/// Upper bound on steps that produce no trace line before giving up, so a ROM
/// that HALTed forever cannot hang the tool.
const MAX_STEPS_PER_LINE: u64 = 50_000_000;

/// Parsed command line.
struct Options {
    rom: String,
    model: Model,
    instructions: u64,
    doctor: bool,
    output: Option<String>,
    profile: bool,
    top: usize,
}

fn usage() -> &'static str {
    "usage:\n  \
     gb-trace --rom PATH [--model dmg|cgb] --instructions N [--doctor] [--output FILE]\n  \
     gb-trace --rom PATH [--model dmg|cgb] --instructions N [--doctor] --profile [--top K]"
}

fn parse_args(argv: &[String]) -> Result<Options, String> {
    let mut rom = None;
    let mut model = Model::Dmg;
    let mut instructions = None;
    let mut doctor = false;
    let mut output = None;
    let mut profile = false;
    let mut top = 20usize;

    let mut i = 0;
    while i < argv.len() {
        let raw = argv[i].clone();
        let (key, inline) = match raw.split_once('=') {
            Some((k, v)) => (k.to_string(), Some(v.to_string())),
            None => (raw.clone(), None),
        };
        // Fetch a value either from `--key=value` or the next argument.
        macro_rules! value {
            () => {
                if let Some(v) = &inline {
                    v.clone()
                } else {
                    i += 1;
                    argv.get(i)
                        .cloned()
                        .ok_or_else(|| format!("missing value for {key}"))?
                }
            };
        }
        match key.as_str() {
            "--rom" => rom = Some(value!()),
            "--model" => {
                let name = value!();
                model = parse_model(&name)
                    .ok_or_else(|| format!("unknown model {name:?} (expected dmg or cgb)"))?;
            }
            "--instructions" => {
                let text = value!();
                instructions = Some(
                    text.parse::<u64>()
                        .map_err(|_| format!("bad instruction count {text:?}"))?,
                );
            }
            "--doctor" => doctor = true,
            "--output" => output = Some(value!()),
            "--profile" => profile = true,
            "--top" => {
                let text = value!();
                top = text
                    .parse::<usize>()
                    .map_err(|_| format!("bad top count {text:?}"))?;
            }
            "-h" | "--help" => {
                println!("{}", usage());
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument {other:?}\n{}", usage())),
        }
        i += 1;
    }

    let rom = rom.ok_or_else(|| format!("--rom is required\n{}", usage()))?;
    let instructions =
        instructions.ok_or_else(|| format!("--instructions is required\n{}", usage()))?;

    Ok(Options {
        rom,
        model,
        instructions,
        doctor,
        output,
        profile,
        top,
    })
}

/// Emit a line for the instruction about to run? Not while HALTed and not for
/// an interrupt dispatch that would be serviced instead of an instruction.
fn line_pending(emu: &Emulator) -> bool {
    if emu.is_halted() {
        return false;
    }
    let pending = emu.ime() && (emu.peek(0xFF0F) & emu.peek(0xFFFF) & 0x1F) != 0;
    !pending
}

fn run() -> Result<(), String> {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let opts = parse_args(&argv)?;

    let rom = std::fs::read(&opts.rom).map_err(|e| format!("cannot read {}: {e}", opts.rom))?;
    let mut emu = Emulator::load_with_model(&rom, opts.model)
        .map_err(|e| format!("cannot load {}: {e}", opts.rom))?;
    if opts.doctor {
        emu.set_doctor(true);
    }

    let mut out: Box<dyn Write> = match &opts.output {
        Some(path) => {
            let file = File::create(path).map_err(|e| format!("cannot create {path}: {e}"))?;
            Box::new(BufWriter::new(file))
        }
        None => Box::new(BufWriter::new(std::io::stdout())),
    };

    let mut counts: HashMap<u16, u64> = HashMap::new();
    let mut produced: u64 = 0;
    let mut since_line: u64 = 0;

    while produced < opts.instructions {
        if line_pending(&emu) {
            let regs = emu.registers();
            let pc = regs.pc;
            let mem = [
                emu.peek(pc),
                emu.peek(pc.wrapping_add(1)),
                emu.peek(pc.wrapping_add(2)),
                emu.peek(pc.wrapping_add(3)),
            ];
            if opts.profile {
                *counts.entry(pc).or_insert(0) += 1;
            } else {
                writeln!(out, "{}", trace_line(&regs, mem))
                    .map_err(|e| format!("write error: {e}"))?;
            }
            produced += 1;
            since_line = 0;
        } else if since_line >= MAX_STEPS_PER_LINE {
            // The ROM stopped early (HALTed with no interrupt coming).
            break;
        } else {
            since_line += 1;
        }
        match emu.step_instruction() {
            StepResult::Ran(_) | StepResult::Breakpoint => {}
        }
    }

    if opts.profile {
        let mut entries: Vec<(u16, u64)> = counts.into_iter().collect();
        entries.sort_by(|a, b| b.1.cmp(&a.1).then(b.0.cmp(&a.0)));
        for (pc, count) in entries.into_iter().take(opts.top) {
            writeln!(out, "PC:{pc:04X} COUNT:{count}").map_err(|e| format!("write error: {e}"))?;
        }
        writeln!(out, "TOTAL:{produced}").map_err(|e| format!("write error: {e}"))?;
    }

    out.flush().map_err(|e| format!("write error: {e}"))?;
    Ok(())
}

fn main() {
    let code = match std::panic::catch_unwind(run) {
        Ok(Ok(())) => 0,
        Ok(Err(message)) => {
            eprintln!("gb-trace: {message}");
            1
        }
        Err(_) => {
            eprintln!("gb-trace: internal emulator error");
            2
        }
    };
    std::process::exit(code);
}
