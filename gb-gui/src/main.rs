//! `gb-gui` — a window, a framebuffer, and keyboard input. Nothing else.
//!
//! Keys: arrows = D-pad · Z = A · X = B · Enter = Start · RShift = Select ·
//! F5 save state · F7 load state · Esc quit.
//!
//! This is demo tooling, kept deliberately tiny so the showcase can show a
//! homebrew game running. It is not part of the acceptance criteria and
//! not built by default (`cargo run -p gb-gui -- GAME.gb`).

use gb_core::{Buttons, Emulator, SCREEN_HEIGHT, SCREEN_WIDTH};
use minifb::{Key, Scale, Window, WindowOptions};
use std::time::{Duration, Instant};

const PALETTE: [u32; 4] = [0xE0F8D0, 0x88C070, 0x346856, 0x081820];

fn main() {
    let path = match std::env::args().nth(1) {
        Some(p) => p,
        None => {
            eprintln!("usage: gb-gui GAME.gb");
            std::process::exit(1);
        }
    };
    let rom = std::fs::read(&path).unwrap_or_else(|e| {
        eprintln!("{path}: {e}");
        std::process::exit(1);
    });
    let mut emu = Emulator::load(&rom).unwrap_or_else(|e| {
        eprintln!("{path}: {e}");
        std::process::exit(1);
    });

    let title = format!("gb — {}", emu_title(&rom));
    let mut window = Window::new(
        &title,
        SCREEN_WIDTH,
        SCREEN_HEIGHT,
        WindowOptions {
            scale: Scale::X4,
            ..WindowOptions::default()
        },
    )
    .expect("open window");

    let frame_time = Duration::from_nanos(1_000_000_000 / 60);
    let mut pixels = vec![0u32; SCREEN_WIDTH * SCREEN_HEIGHT];
    let mut saved: Option<Vec<u8>> = None;

    while window.is_open() && !window.is_key_down(Key::Escape) {
        let start = Instant::now();

        emu.set_buttons(Buttons {
            right: window.is_key_down(Key::Right),
            left: window.is_key_down(Key::Left),
            up: window.is_key_down(Key::Up),
            down: window.is_key_down(Key::Down),
            a: window.is_key_down(Key::Z),
            b: window.is_key_down(Key::X),
            start: window.is_key_down(Key::Enter),
            select: window.is_key_down(Key::RightShift),
        });
        if window.is_key_pressed(Key::F5, minifb::KeyRepeat::No) {
            saved = Some(emu.save_state());
        }
        if window.is_key_pressed(Key::F7, minifb::KeyRepeat::No) {
            if let Some(s) = &saved {
                if let Err(e) = emu.load_state(s) {
                    eprintln!("load state: {e}");
                }
            }
        }

        emu.step_frame();
        for (dst, &shade) in pixels.iter_mut().zip(emu.framebuffer()) {
            *dst = PALETTE[(shade & 3) as usize];
        }
        window
            .update_with_buffer(&pixels, SCREEN_WIDTH, SCREEN_HEIGHT)
            .expect("present");

        if let Some(rest) = frame_time.checked_sub(start.elapsed()) {
            std::thread::sleep(rest);
        }
    }
}

fn emu_title(rom: &[u8]) -> String {
    gb_core::cartridge::Header::parse(rom)
        .map(|h| h.title)
        .unwrap_or_else(|_| "?".into())
}
