// SPDX-License-Identifier: MIT
//! One host window asked full screen, painted through softbuffer: a 640 by
//! 480 picture shown at the largest whole factor the window's inside holds,
//! centred, black around it. A screen grab can check every pixel: the pixel
//! at column `x`, row `y` holds palette index `1 + (7 * x + 13 * y) % 255`,
//! so none matches any of its 8 neighbours, and index `i` is the colour with
//! red `i`, green `255 - i` and blue `97 * i % 256`, so no two indices share
//! a colour and none is black. A bounded run, never left open, and never a
//! test: Linux turns the loop on the main thread alone. Run it under a time
//! limit:
//!
//! `timeout 60 cargo run -p draw --features native --example full_screen_draw`
//!
//! The window is asked full screen after it is placed and before it is
//! made. On X11 the window manager carries the ask out: with none running,
//! the window keeps the program's rectangle. It prints each step, each
//! change of the inside and each `draw` line; it exits 0 once the host made
//! the window and took the surface, 1 when it refused either.
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
const WIDTH: u32 = 640;
const HEIGHT: u32 = 480;
/// The same two sides as a host's rectangle takes them, signed.
const SIDES: (i32, i32) = (640, 480);
/// Turns of the loop, 20 ms apart and a present each: time for two grabs.
const TURNS: u32 = 300;

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

/// Palette index `index` as `0x00RRGGBB`: red is the index itself.
fn colour(index: u32) -> u32 {
    (index << 16) | ((255 - index) << 8) | (97 * index % 256)
}

/// The picture, each pixel's index from its column and row.
fn patterned() -> Shown {
    let mut palette = Palette::black();
    for (index, word) in (0u32..).zip(palette.0.iter_mut()).skip(1) {
        *word = colour(index);
    }
    let pixels = (0..HEIGHT)
        .flat_map(|y| {
            (0..WIDTH).map(move |x| u8::try_from(1 + (7 * x + 13 * y) % 255).unwrap_or(0))
        })
        .collect();
    Shown {
        width: WIDTH,
        height: HEIGHT,
        pitch: WIDTH,
        pixels,
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
    let rect = Rect::sized(0, 0, SIDES.0, SIDES.1);
    host.place(WINDOW, rect, true);
    if let Err(refusal) = host.full_screen(WINDOW, true) {
        println!("{} ms: not full screen: {refusal}", host.now());
        return ExitCode::FAILURE;
    }
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
    let picture = patterned();
    // The host may answer the full-screen ask with a new size at any turn:
    // each turn paints at the size the window says now.
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
    println!("{} ms: done, inside {}x{}", host.now(), size.0, size.1);
    host.gone(WINDOW);
    ExitCode::SUCCESS
}
