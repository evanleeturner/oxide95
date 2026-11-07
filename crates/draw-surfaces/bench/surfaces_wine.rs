// SPDX-License-Identifier: MIT
//! The worked script on a real window under Wine, through `window::user32` and
//! `draw::user32`. It prints its reads, then `ready`, then holds the last picture for a
//! dump from outside. Built for `i686-pc-windows-gnu`, with `-lgdi32 -luser32` last on the
//! link line, as a program's own imports need them there.
#![expect(
    unsafe_code,
    reason = "a window's class, procedure and messages are the target's own calls"
)]

use std::process::ExitCode;
use std::time::Instant;

use draw::easel::Easel;
use draw::user32::{Real, Window};

#[path = "wine_window.rs"]
mod wine_window;
#[path = "worked_script.rs"]
mod worked_script;

use wine_window::{FROM, address, make_window};
use worked_script::{HEIGHT, LEFT, Script, TOP, Terms, WIDTH};

const PM_REMOVE: u32 = 1;
const MESSAGE_WORDS: usize = 7;
/// A `PROCESS_MEMORY_COUNTERS` at 32 bits, and `WorkingSetSize`'s word in it.
const COUNTER_WORDS: usize = 10;
const COUNTER_BYTES: u32 = 40;
const WORKING_SET: usize = 3;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentProcess() -> u32;
    fn K32GetProcessMemoryInfo(process: u32, counters: *mut u32, bytes: u32) -> i32;
}

/// The working set, by the target's own read: a Unix process's `/proc` is out of reach,
/// as Wine's server, not this process, opens a file.
fn rss() -> Option<u64> {
    let mut counters = [0_u32; COUNTER_WORDS];
    counters[0] = COUNTER_BYTES;
    // SAFETY: the pseudo handle, and a local of the size it is told.
    let answer = unsafe {
        K32GetProcessMemoryInfo(GetCurrentProcess(), counters.as_mut_ptr(), COUNTER_BYTES)
    };
    (answer != 0).then_some(u64::from(counters[WORKING_SET]))
}

/// Every message waiting, each handed on.
fn pump() {
    let message = [0_u32; MESSAGE_WORDS];
    // SAFETY: the message record outlives the calls.
    unsafe {
        while window::user32::peek_message(FROM, address(&message), 0, 0, 0, PM_REMOVE) != 0 {
            let _ = window::user32::translate_message(FROM, address(&message));
            let _ = window::user32::dispatch_message(FROM, address(&message));
        }
    }
}

fn main() -> ExitCode {
    let window = make_window(LEFT, TOP, WIDTH, HEIGHT);
    let terms = match Terms::from_args("wine", window, rss) {
        Ok(terms) => terms,
        Err(refusal) => {
            println!("no run: {refusal}");
            return ExitCode::FAILURE;
        }
    };
    let back = Window::new();
    back.aim(window);
    let easel = Easel::new(back, WIDTH, HEIGHT);
    let mut script = Script::new(terms, &easel, &Real);
    let started = Instant::now();
    script.drive(pump);
    script.rate(u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX));
    print!("{}", script.lines());
    println!("ready");
    script.hold(pump);
    ExitCode::SUCCESS
}
