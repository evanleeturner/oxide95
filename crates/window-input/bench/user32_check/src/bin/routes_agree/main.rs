// SPDX-License-Identifier: MIT
//! Do the two routes to the window calls answer alike? One route is `window::user32` and
//! `window_input::user32`, which ask the real USER32. The other is `window::front` and
//! `window_input::front` over `Desk`, which answer by themselves. This program makes one list of
//! calls, `script.rs`, through one route per run and prints each call's line, and
//! `routes_agree.py`, beside `user32_check/`, reads 2 runs' logs side by side.
//!
//! Both routes run in this one 32-bit program under Wine, `Desk` over `Page` with no browser, so a
//! difference of compiler or target cannot pass for a difference of route. So this reads neither
//! `Native` under `Desk` nor a browser's key.
//!
//! `routes_agree <thin|desk> <plant>`. A plant is a wrong answer put on the `Desk` side on purpose;
//! `none` runs clean. The last line is `routes_agree done`, so a run cut short cannot pass for a
//! short script.
#![allow(
    unsafe_code,
    reason = "this program is calls into USER32, by hand and through the thin calls"
)]

#[allow(
    dead_code,
    reason = "taken for the logger and the address helpers, not the tally"
)]
#[path = "../../bench.rs"]
mod bench;
#[allow(
    dead_code,
    reason = "taken for the calls this program makes by hand, not all of them"
)]
#[path = "../../hand.rs"]
mod hand;
#[allow(dead_code, reason = "taken for its asker alone")]
#[path = "../../keys_and_cursor.rs"]
mod keys_and_cursor;
mod on_desk;
mod script;
mod thin;

use std::process::ExitCode;

use on_desk::Plant;

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let [route, plant] = arguments.as_slice() else {
        println!("usage: routes_agree <thin|desk> <none|wrong-metric|no-line>");
        return ExitCode::from(2);
    };
    let Some(plant) = Plant::named(plant) else {
        println!("no plant is named {plant}");
        return ExitCode::from(2);
    };
    bench::install_logger();
    match (route.as_str(), plant) {
        ("thin", Plant::None) => script::run(&thin::Thin::new()),
        ("desk", plant) => on_desk::run(plant),
        _ => {
            println!("no run is named {route} with that plant");
            return ExitCode::from(2);
        }
    }
    println!("routes_agree done");
    ExitCode::SUCCESS
}
