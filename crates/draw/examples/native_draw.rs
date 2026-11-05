// SPDX-License-Identifier: MIT
//! One host window painted through softbuffer: 4 bands, red, green, blue and
//! white, a picture a screen grab can read back. A bounded run, never left
//! open, and never a test: Linux turns the loop on the main thread alone. Run
//! it under a time limit:
//!
//! `timeout 30 cargo run -p draw --features native --example native_draw`
//!
//! It prints each step and each `draw.present` or `draw.refused` line; it
//! exits 0 once the host made the window and took the surface, 1 when it
//! refused either.
use std::process::ExitCode;
use std::thread;
use std::time::Duration;

use draw::easel::{Back, Palette, Shown};
use log::kv::{Error, Key, Value, VisitSource};
use log::{Level, LevelFilter, Log, Metadata, Record};
use window::native::Native;
use window::{Rect, Screen};

/// The one window's handle, as a desk would give it.
const WINDOW: u32 = 1;
const WIDTH: u32 = 320;
const HEIGHT: u32 = 200;
/// The same two sides as a host's rectangle takes them, signed.
const SIDES: (i32, i32) = (320, 200);
/// Its corner on the host's desktop, for a screen grab to find it.
const CORNER: (i32, i32) = (40, 40);
/// Turns of the loop, 20 ms apart, the bands up: about 3 s.
const TURNS: u32 = 150;

/// One key's pairs as `key=value`.
struct Pairs;

impl<'kvs> VisitSource<'kvs> for Pairs {
    fn visit_pair(&mut self, key: Key<'kvs>, value: Value<'kvs>) -> Result<(), Error> {
        print!(" {key}={value}");
        Ok(())
    }
}

/// `draw`'s lines on standard output, the host's crates' left out.
struct Lines;

impl Log for Lines {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= Level::Debug && metadata.target().starts_with("draw")
    }

    fn log(&self, record: &Record<'_>) {
        if self.enabled(record.metadata()) {
            print!("{}", record.args());
            let _ = record.key_values().visit(&mut Pairs);
            println!();
        }
    }

    fn flush(&self) {}
}

/// The bands, each a quarter of the picture's width.
fn bands() -> Shown {
    let mut palette = Palette::black();
    palette.0[1] = 0x00ff_0000;
    palette.0[2] = 0x0000_ff00;
    palette.0[3] = 0x0000_00ff;
    palette.0[4] = 0x00ff_ffff;
    let row: Vec<u8> = (0..WIDTH)
        .map(|x| u8::try_from(1 + x * 4 / WIDTH).unwrap_or(0))
        .collect();
    Shown {
        width: WIDTH,
        height: HEIGHT,
        pitch: WIDTH,
        pixels: row.repeat(HEIGHT as usize),
        palette,
    }
}

fn main() -> ExitCode {
    static LINES: Lines = Lines;
    if log::set_logger(&LINES).is_ok() {
        log::set_max_level(LevelFilter::Debug);
    }
    let host = match Native::new(640, 480) {
        Ok(host) => host,
        Err(refusal) => {
            println!("no loop: {refusal}");
            return ExitCode::FAILURE;
        }
    };
    let rect = Rect::sized(CORNER.0, CORNER.1, SIDES.0, SIDES.1);
    host.place(WINDOW, rect, true);
    for _ in 0..25 {
        host.pump(&mut |_| {});
        thread::sleep(Duration::from_millis(20));
    }
    let lent = match host.lend(WINDOW) {
        Ok(lent) => lent,
        Err(refusal) => {
            println!("{} ms: {refusal}: {:?}", host.now(), host.made(WINDOW));
            return ExitCode::FAILURE;
        }
    };
    let (width, height) = lent.inner_size();
    println!(
        "{} ms: lent, inside {width}x{height}, scale {}",
        host.now(),
        lent.scale_factor()
    );
    let painter = match draw::native::Native::new(lent.clone(), width, height) {
        Ok(painter) => painter,
        Err(refusal) => {
            println!("{} ms: no surface: {refusal}", host.now());
            return ExitCode::FAILURE;
        }
    };
    let picture = bands();
    // A Wayland window shows at its first picture, and a host may answer that
    // with a new size: each turn paints at the size the window says now.
    let mut size = (width, height);
    for _ in 0..TURNS {
        host.pump(&mut |_| {});
        if lent.inner_size() != size {
            size = lent.inner_size();
            println!("{} ms: inside now {}x{}", host.now(), size.0, size.1);
            if let Err(refusal) = painter.resize(size.0, size.1) {
                println!("{} ms: no new size: {refusal}", host.now());
            }
        }
        painter.present(&picture);
        thread::sleep(Duration::from_millis(20));
    }
    println!("{} ms: done", host.now());
    host.gone(WINDOW);
    ExitCode::SUCCESS
}
