// SPDX-License-Identifier: MIT
//! The worked script on a host's window through softbuffer. It prints its reads, then
//! `ready`, then holds the last picture for a dump from outside. Linux turns the loop on
//! the main thread alone, so it is a bounded program and never a test:
//!
//! `timeout 60 cargo run -p draw-surfaces --features native --example surfaces_native --
//! DS1 none 0`
use std::process::ExitCode;
use std::thread;
use std::time::{Duration, Instant};

use draw::easel::Easel;
use draw::front::Windows;
use window::native::Native;
use window::{Rect, Screen};

#[path = "worked_script.rs"]
mod worked_script;

use worked_script::{HEIGHT, LEFT, Script, TOP, Terms, WIDTH};

/// The one window's handle, as a desk would give it.
const WINDOW: u32 = 1;
const DESKTOP: (i32, i32) = (640, 480);
const SETTLE_TURNS: u32 = 25;
const SETTLE_TURN: Duration = Duration::from_millis(20);
const RSS_FIELD: &str = "VmRSS:";
const KILOBYTE: u64 = 1024;

/// `Surfaces` asks only `quiet` of its windows: the host keeps nothing across a line, and
/// the other 3 answer for a window this program made.
struct Quiet;

impl Windows for Quiet {
    fn quiet(&self, line: impl FnOnce()) {
        line();
    }

    fn size(&self, _: u32) -> Option<(u32, u32)> {
        Some((WIDTH, HEIGHT))
    }

    fn update(&self, _: u32) -> bool {
        true
    }

    fn validate(&self, _: u32, _: Option<[u32; 4]>) -> bool {
        true
    }
}

fn rss() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|line| line.starts_with(RSS_FIELD))?;
    let kilobytes: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kilobytes * KILOBYTE)
}

fn main() -> ExitCode {
    let terms = match Terms::from_args("native", WINDOW, rss) {
        Ok(terms) => terms,
        Err(refusal) => {
            println!("no run: {refusal}");
            return ExitCode::FAILURE;
        }
    };
    let host = match Native::new(DESKTOP.0, DESKTOP.1) {
        Ok(host) => host,
        Err(refusal) => {
            println!("no loop: {refusal}");
            return ExitCode::FAILURE;
        }
    };
    let sides = (
        i32::try_from(WIDTH).unwrap_or(0),
        i32::try_from(HEIGHT).unwrap_or(0),
    );
    host.place(WINDOW, Rect::sized(LEFT, TOP, sides.0, sides.1), true);
    for _ in 0..SETTLE_TURNS {
        host.pump(&mut |_| {});
        thread::sleep(SETTLE_TURN);
    }
    let lent = match host.lend(WINDOW) {
        Ok(lent) => lent,
        Err(refusal) => {
            println!("no window: {refusal}");
            return ExitCode::FAILURE;
        }
    };
    let (width, height) = lent.inner_size();
    let painter = match draw::native::Native::new(lent.clone(), width, height) {
        Ok(painter) => painter,
        Err(refusal) => {
            println!("no surface: {refusal}");
            return ExitCode::FAILURE;
        }
    };
    let easel = Easel::new(painter, WIDTH, HEIGHT);
    let mut script = Script::new(terms, &easel, &Quiet);
    let started = Instant::now();
    script.drive(|| host.pump(&mut |_| {}));
    script.rate(u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX));
    print!("{}", script.lines());
    println!("ready");
    script.hold(|| host.pump(&mut |_| {}));
    host.gone(WINDOW);
    ExitCode::SUCCESS
}
