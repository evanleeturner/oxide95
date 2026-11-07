// SPDX-License-Identifier: MIT
//! A tap, a timer and the loop's pace under Wine, one check a run, one JSON line a read. It
//! judges nothing. `desk` is `Desk` over `Page`: no real call and no window, so Wine runs the
//! layer's own code. `user32` is the direct back, each call the real one: it has no desk, so it
//! takes the pace alone. Built for `i686-pc-windows-gnu`, `-lgdi32 -luser32` last on the link
//! line.
//!
//! `wine wine_loop.exe <desk|user32> <check> <plant>`
//!
//! - `WD4`: a tap between 2 reads, no pump between them; plant `holds-bit`.
//! - `WD5`: a timer's message with no window, queue to dispatch; plant `hands-timer`.
//! - `WD6`: each of 3 calls a second, logger off and on; plant `slow-pump`: 1 ms in `pump`, or
//!   beside each call on `user32`, which has no pump.
//!
//! It exits 0 on a run done, 2 on a word it does not know.
#![expect(
    unsafe_code,
    reason = "the direct back's `PeekMessageA` and `GetCursorPos` write a caller's record"
)]

use std::hint::black_box;
use std::process::ExitCode;
use std::thread;
use std::time::Duration;

use window::Event;
use window::desk::Desk;
use window::page::Page;
use window_input::Input;
use window_input::front::Front;

#[path = "../bench/loop_checks.rs"]
#[allow(dead_code, reason = "each route's program takes what its route needs")]
mod loop_checks;

use loop_checks::{
    INPUT_ROW, KEY_A, Slowed, TapKeys, logger_on, pace, print_read, print_timer, rates, timer_reach,
};

/// The call site a line names: this program has one.
const FROM: u32 = 1;
const PM_REMOVE: u32 = 1;
const MESSAGE_WORDS: usize = 7;
const POINT_WORDS: usize = 2;
const SCREEN: (i32, i32) = (640, 480);
const WINDOW: u32 = 1;
const WAIT: Duration = Duration::from_millis(1);

/// The address of a record a call writes, as the call takes it.
fn address<T>(value: &mut T) -> u32 {
    u32::try_from(std::ptr::from_mut(value).expose_provenance()).unwrap_or(0)
}

/// A key pressed and let go between 2 reads, the back handed both before the second's pump.
fn tap(holds: bool) {
    let desk = Desk::new(Input::new(Page::new(SCREEN.0, SCREEN.1)));
    let front = Front::new(&INPUT_ROW, &desk);
    let mut copy = TapKeys::new();
    let key = u32::from(KEY_A);
    let before = if holds {
        copy.answer(KEY_A)
    } else {
        front.get_async_key_state(key)
    };
    print_read("before", before);
    for down in [true, false] {
        let event = Event::Key {
            window: WINDOW,
            key: KEY_A,
            down,
        };
        copy.apply(event);
        desk.screen().back().hand(event);
    }
    let after = if holds {
        copy.answer(KEY_A)
    } else {
        front.get_async_key_state(key)
    };
    print_read("after", after);
}

/// `Desk` over `Page`'s rows, its pump waiting when `slow`.
fn pace_desk(slow: bool) {
    let desk = Desk::new(Input::new(Slowed::new(Page::new(SCREEN.0, SCREEN.1))));
    if slow {
        desk.screen().back().plant(|| thread::sleep(WAIT));
    }
    pace(&desk);
}

/// One real call of the 3 a pace's rows name.
fn user32_call(call: usize) {
    match call {
        0 => {
            black_box(window_input::user32::get_async_key_state(
                FROM,
                u32::from(KEY_A),
            ));
        }
        1 => {
            let mut message = [0_u32; MESSAGE_WORDS];
            // SAFETY: the record outlives the call and is the size a `MSG` is.
            black_box(unsafe {
                window::user32::peek_message(FROM, address(&mut message), 0, 0, 0, PM_REMOVE)
            });
        }
        _ => {
            let mut point = [0_i32; POINT_WORDS];
            // SAFETY: the record outlives the call and is the size a `POINT` is.
            black_box(unsafe { window_input::user32::get_cursor_pos(FROM, address(&mut point)) });
        }
    }
}

/// The direct back's rows, the same wait beside each call when `slow`.
fn pace_user32(slow: bool) {
    let run = |call: usize, count: u32| {
        for _ in 0..count {
            if slow {
                thread::sleep(WAIT);
            }
            user32_call(call);
        }
    };
    rates("off", run);
    logger_on();
    rates("on", run);
}

fn main() -> ExitCode {
    let words: Vec<String> = std::env::args().skip(1).collect();
    let [route, check, plant] = words.as_slice() else {
        println!("call: wine_loop <desk|user32> <WD4|WD5|WD6> <plant>");
        return ExitCode::from(2);
    };
    let planted = plant != "none";
    match (route.as_str(), check.as_str(), plant.as_str()) {
        ("desk", "WD4", "none" | "holds-bit") => tap(planted),
        ("desk", "WD5", "none" | "hands-timer") => {
            let desk = Desk::new(Page::new(SCREEN.0, SCREEN.1));
            print_timer(timer_reach(&desk, planted));
        }
        ("desk", "WD6", "none" | "slow-pump") => pace_desk(planted),
        ("user32", "WD6", "none" | "slow-pump") => pace_user32(planted),
        _ => {
            println!("no run {route} {check} {plant}");
            return ExitCode::from(2);
        }
    }
    println!(r#"{{"row":"done"}}"#);
    ExitCode::SUCCESS
}
