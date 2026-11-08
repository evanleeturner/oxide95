// SPDX-License-Identifier: MIT
//! Wine's own key, asked by a 32-bit program with no crate of ours, its calls declared by
//! hand. Built by `wine.sh link`; no Cargo target.
//!
//! It opens a window at 40,40, prints `ready`, and reads `GetAsyncKeyState` for 450 turns of
//! 20 ms. The keys come from outside, by `xdotool`, which enters past the device: nobody
//! pressed one. A key passes when it reads down and later up; a key held across a read
//! gives `8001`, then `8000`, then `0`, and a tap between reads gives `1`, then `0`.
//!
//! Lines take the log's `imp.call` form, printed as this file links no logger. Exits 0 on a
//! pass, 1 on red.
//!
//! `CK1`: each key sent from outside reads down and later up through `GetAsyncKeyState`.
use std::process::ExitCode;
use std::thread::sleep;
use std::time::Duration;

mod bare_window;
// Each program takes only part of the shared file.
#[allow(dead_code)]
mod worked_picture;

use worked_picture::{HEIGHT, KEY_RED, LEFT, TOP, WIDTH};

const TURNS: u32 = 450;
const TURN: Duration = Duration::from_millis(20);
const HELD_OR_TAPPED: u16 = 0x8001;
const KEYS: [(i32, &str); 3] = [(0x41, "a"), (0x1b, "Escape"), (0x10, "Shift")];

#[link(name = "user32")]
unsafe extern "system" {
    fn GetAsyncKeyState(key: i32) -> i16;
}

/// A key's bits that say whether it is held or was tapped since the last read.
fn read(key: i32) -> u16 {
    // SAFETY: the call takes any number and touches no memory of ours.
    let state = unsafe { GetAsyncKeyState(key) };
    state.cast_unsigned() & HELD_OR_TAPPED
}

fn main() -> ExitCode {
    let size = (WIDTH as i32, HEIGHT as i32);
    if let Err(why) = bare_window::open(c"key_probe", (LEFT, TOP), size) {
        println!("CK1 refused {why}");
        println!("CK1 red {KEY_RED}");
        return ExitCode::FAILURE;
    }
    let mut was = [0_u16; KEYS.len()];
    let mut went_down = [false; KEYS.len()];
    let mut came_up = [false; KEYS.len()];
    for turn in 0..TURNS {
        bare_window::pump();
        for (index, (key, _)) in KEYS.iter().enumerate() {
            let state = read(*key);
            if state != was[index] {
                println!("imp.call c=USER32!GetAsyncKeyState a={key} v={state:x} by=layer-check");
                was[index] = state;
            }
            went_down[index] |= state != 0;
            came_up[index] |= went_down[index] && state == 0;
        }
        if turn == 0 {
            println!("ready");
        }
        sleep(TURN);
    }
    let mut whole = true;
    for (index, (_, name)) in KEYS.iter().enumerate() {
        let (down, up) = (went_down[index], came_up[index]);
        println!("key {name} down={down} up={up}");
        whole &= down && up;
    }
    if whole {
        println!("CK1 pass");
        ExitCode::SUCCESS
    } else {
        println!("CK1 red {KEY_RED}");
        ExitCode::FAILURE
    }
}
