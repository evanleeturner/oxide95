// SPDX-License-Identifier: MIT
//! Do `window::user32` and `window_input::user32` answer as the real USER32 does? This program
//! runs on the 32-bit Windows target, under Wine, and asks each thin call beside a call declared
//! by hand in the same program, so a wrong answer says whether the thin call or Wine gave it.
//!
//! `user32_check <wu1|wu2|wu3> <plant>`. `wu1` is the window calls, `wu2` the message loop, and
//! `wu3` the keys and the cursor, which needs the driver, `../user32_check.sh`, to press keys
//! from outside. A plant is a wrong answer put in on purpose; `none` runs the check clean.
//!
//! Nobody presses a key here: each comes from `xdotool`, which enters past the device, on a
//! private screen nobody looks at. Exits 0 on a pass, 1 on a red.
#![allow(
    unsafe_code,
    reason = "this program is calls into USER32, by hand and through the thin calls"
)]

mod bench;
mod hand;
mod keys_and_cursor;
mod message_loop;
mod window_calls;

use std::process::ExitCode;

use bench::Plant;

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let [check, plant] = arguments.as_slice() else {
        println!("usage: user32_check <wu1|wu2|wu3> <plant>");
        return ExitCode::from(2);
    };
    let Some(plant) = Plant::named(plant) else {
        println!("no plant is named {plant}");
        return ExitCode::from(2);
    };
    bench::install_logger();
    match check.as_str() {
        "wu1" => window_calls::run(plant),
        "wu2" => message_loop::run(plant),
        "wu3" => keys_and_cursor::run(plant),
        _ => {
            println!("no check is named {check}");
            ExitCode::from(2)
        }
    }
}
