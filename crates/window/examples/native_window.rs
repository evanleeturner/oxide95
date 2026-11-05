// SPDX-License-Identifier: MIT
//! One host window through the native back: placed, pumped about a
//! second, placed again while made, let go and pumped once more. A bounded
//! run, never left open, and never a test: Linux turns the loop on the
//! main thread alone. Run it under a time limit:
//!
//! `DISPLAY=:1 timeout 30 cargo run -p window --features native --example native_window`
//!
//! It prints one line a step and each event the host raised; it exits 0
//! once the host made the window, 1 when the host refused it or the loop.
use std::process::ExitCode;
use std::thread;
use std::time::Duration;

use window::native::Native;
use window::{Rect, Screen};

/// The one window's handle, as a desk would give it.
const WINDOW: u32 = 1;

/// Pumps `turns` times, 20 ms apart, printing each event; the events
/// counted.
fn turn(back: &Native, turns: u32) -> u32 {
    let mut events = 0;
    for _ in 0..turns {
        back.pump(&mut |event| {
            events += 1;
            println!("{} ms: {event:?}", back.now());
        });
        thread::sleep(Duration::from_millis(20));
    }
    events
}

fn main() -> ExitCode {
    let back = match Native::new(640, 480) {
        Ok(back) => back,
        Err(refusal) => {
            println!("no loop: {refusal}");
            return ExitCode::FAILURE;
        }
    };
    println!("screen {:?}", back.size());
    back.place(WINDOW, Rect::sized(40, 40, 320, 200), true);
    println!(
        "placed {WINDOW} at 40,40 320x200, made {:?}",
        back.made(WINDOW)
    );
    let mut events = turn(&back, 25);
    match back.made(WINDOW) {
        Ok(true) => println!("{} ms: made {WINDOW}", back.now()),
        Ok(false) => {
            println!("{} ms: {WINDOW} not made after 25 pumps", back.now());
            return ExitCode::FAILURE;
        }
        Err(refusal) => {
            println!("{} ms: {WINDOW} refused: {refusal}", back.now());
            return ExitCode::FAILURE;
        }
    }
    back.place(WINDOW, Rect::sized(80, 80, 400, 240), true);
    println!("{} ms: placed {WINDOW} at 80,80 400x240", back.now());
    events += turn(&back, 25);
    back.gone(WINDOW);
    println!(
        "{} ms: {WINDOW} gone, made {:?}",
        back.now(),
        back.made(WINDOW)
    );
    events += turn(&back, 5);
    println!("{} ms: done, {events} events", back.now());
    ExitCode::SUCCESS
}
