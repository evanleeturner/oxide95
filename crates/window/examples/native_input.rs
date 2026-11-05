// SPDX-License-Identifier: MIT
//! The native back's input, once on a host's desktop: one window made,
//! its focus awaited, each grab mode asked and its answer printed, the
//! program's lock grabbed and the cursor hidden, then every event the host
//! raised printed for `turns` pumps (150 by default, 20 ms apart), counted
//! by kind, and the lock let go. A bounded run, never left open, and never
//! a test: Linux turns the loop on the main thread alone. Run it under a
//! time limit, its input given by hand or by `xdotool` once it prints
//! `grabbed`:
//!
//! `DISPLAY=:1 timeout 60 cargo run -p window --features native --example native_input -- 150`
//!
//! It exits 0 once the host made the window and took a grab, 1 otherwise.
use std::process::ExitCode;
use std::thread;
use std::time::Duration;

use window::native::{Hold, Native};
use window::{Event, Rect, Screen};

/// The one window's handle, as a desk would give it.
const WINDOW: u32 = 1;

/// The events counted by kind: close, active, key, button, pointer,
/// motion; the motion's sum beside.
#[derive(Debug, Default)]
struct Counts {
    close: u32,
    active: u32,
    key: u32,
    button: u32,
    pointer: u32,
    motion: u32,
    moved: (i32, i32),
}

impl Counts {
    fn add(&mut self, event: Event) {
        match event {
            Event::Close { .. } => self.close += 1,
            Event::Active { .. } => self.active += 1,
            Event::Key { .. } => self.key += 1,
            Event::Button { .. } => self.button += 1,
            Event::Pointer { .. } => self.pointer += 1,
            Event::Motion { dx, dy } => {
                self.motion += 1;
                self.moved = (self.moved.0 + dx, self.moved.1 + dy);
            }
        }
    }
}

/// Pumps up to `turns` times, 20 ms apart, printing and counting each
/// event, until `until` holds for one.
fn turn(back: &Native, turns: u32, counts: &mut Counts, until: impl Fn(Event) -> bool) -> bool {
    let mut met = false;
    for _ in 0..turns {
        back.pump(&mut |event| {
            counts.add(event);
            met |= until(event);
            println!("{} ms: {event:?}", back.now());
        });
        if met {
            return true;
        }
        thread::sleep(Duration::from_millis(20));
    }
    false
}

fn main() -> ExitCode {
    let turns = std::env::args()
        .nth(1)
        .and_then(|turns| turns.parse().ok())
        .unwrap_or(150);
    let back = match Native::new(640, 480) {
        Ok(back) => back,
        Err(refusal) => {
            println!("no loop: {refusal}");
            return ExitCode::FAILURE;
        }
    };
    back.place(WINDOW, Rect::sized(40, 40, 320, 200), true);
    let mut counts = Counts::default();
    turn(&back, 25, &mut counts, |_| false);
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
    let focused = turn(&back, 250, &mut counts, |event| {
        matches!(event, Event::Active { active: true, .. })
    });
    println!("{} ms: focused {focused}", back.now());
    for hold in [Hold::Locked, Hold::Confined, Hold::Free] {
        println!(
            "{} ms: hold {hold:?}: {:?}",
            back.now(),
            back.hold(WINDOW, hold)
        );
    }
    let grabbed = back.grab(WINDOW, true);
    println!("{} ms: grab: {grabbed:?}", back.now());
    println!(
        "{} ms: cursor hidden: {:?}",
        back.now(),
        back.cursor_shown(WINDOW, false)
    );
    println!("{} ms: grabbed", back.now());
    turn(&back, turns, &mut counts, |_| false);
    println!("{} ms: let go: {:?}", back.now(), back.grab(WINDOW, false));
    let _ = back.cursor_shown(WINDOW, true);
    back.gone(WINDOW);
    turn(&back, 5, &mut counts, |_| false);
    println!("{} ms: done, {counts:?}", back.now());
    if grabbed.is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
