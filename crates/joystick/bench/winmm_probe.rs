// SPDX-License-Identifier: MIT
//! Wine's own `winmm`, called by hand with no pad on the box, beside the crate's answers: the
//! quiet `Front`, `PadFront` over `WinmmPads`, and `WinmmPads` itself. One line a cell, its
//! fields `name=value`, read by `winmm_check.py`, which judges the crate by Wine's numbers.
//!
//! Built for `i686-pc-windows-gnu`, as `joystick::winmm` and the `winmm` import are Windows's:
//! hence its own feature, so a native `cargo test --workspace` leaves it out.
#![allow(unsafe_code)]

use std::panic::{self, AssertUnwindSafe};

use joystick::pads::{PadFront, Pads, Record};
use joystick::table::{Calls, Row};
use joystick::winmm::WinmmPads;
use joystick::{Back, Front};

#[link(name = "winmm")]
unsafe extern "system" {
    fn joyGetNumDevs() -> u32;
    fn joyGetDevCapsA(device: usize, record: *mut u32, size: u32) -> u32;
    fn joyGetPosEx(device: u32, record: *mut u32) -> u32;
}

/// `sizeof(JOYCAPSA)` and `sizeof(JOYINFOEX)`, read by a compile: `_Static_assert` under clang
/// for `i686-w64-mingw32`, against mingw-w64's `joystickapi.h`.
const CAPS_SIZE: u32 = 404;
const INFO_SIZE: u32 = 52;

const NUMBERS: [u32; 6] = [0, 1, 15, 16, 17, 0xffff_fffe];
const CAPS_SIZES: [u32; 4] = [0, CAPS_SIZE - 1, CAPS_SIZE, 1024];
const READ_FLAGS: [u32; 3] = [0, 0x483, 0x4ff];
/// `(size, flags)` outside what `PadFront` reads, asked of number 0 alone.
const BEYOND: [(u32, u32); 7] = [
    (INFO_SIZE, 0x1),
    (INFO_SIZE, 0xff),
    (INFO_SIZE, 0x100),
    (INFO_SIZE, 0x583),
    (0, 0),
    (INFO_SIZE - 1, 0),
    (INFO_SIZE + 1, 0),
];

/// What no call wrote: a write of 0 shows, where a zeroed record would hide it.
const UNTOUCHED: u32 = 0xa5a5_a5a5;
/// 1024 bytes, the largest size asked for.
const RECORD_WORDS: usize = 256;
/// `dwSize` and `dwFlags`, which the caller writes.
const POSITION_ASKED: usize = 2;

struct Words([u32; RECORD_WORDS]);

impl Words {
    fn blank() -> Self {
        Self([UNTOUCHED; RECORD_WORDS])
    }

    fn position(size: u32, flags: u32) -> Self {
        let mut words = Self::blank();
        words.0[0] = size;
        words.0[1] = flags;
        words
    }

    fn touched(&self, from: usize) -> usize {
        self.0[from..]
            .iter()
            .filter(|word| **word != UNTOUCHED)
            .count()
    }
}

impl Record for Words {
    fn word(&self, index: usize) -> u32 {
        self.0[index]
    }

    fn set_word(&mut self, index: usize, value: u32) {
        self.0[index] = value;
    }
}

/// A stop's end of the run, caught by name so the next cell still reads.
struct Stopped(String);

struct Catches;

impl Back for Catches {
    fn stop(&self, name: &str) -> ! {
        panic::panic_any(Stopped(name.to_owned()))
    }
}

/// The call's number, or `stop:` and the call's name; any other panic goes on up.
fn answer(call: impl FnOnce() -> u32) -> String {
    match panic::catch_unwind(AssertUnwindSafe(call)) {
        Ok(number) => number.to_string(),
        Err(payload) => match payload.downcast::<Stopped>() {
            Ok(stopped) => format!("stop:{}", stopped.0),
            Err(other) => panic::resume_unwind(other),
        },
    }
}

fn caps_cell(quiet: &Front<Catches>, front: &PadFront<Catches, WinmmPads>, device: u32, size: u32) {
    let mut wine_record = Words::blank();
    // SAFETY: the record holds 1024 bytes, at least any size asked for here.
    let wine = unsafe { joyGetDevCapsA(device as usize, wine_record.0.as_mut_ptr(), size) };
    let mut front_record = Words::blank();
    let front_answer = answer(|| front.caps(device, size, &mut front_record));
    println!(
        "caps device={device:#x} size={size} wine={wine} wine_touched={} quiet={} padfront={front_answer} padfront_touched={}",
        wine_record.touched(0),
        answer(|| quiet.caps(device, size)),
        front_record.touched(0),
    );
}

fn position_cell(
    kind: &str,
    front: &PadFront<Catches, WinmmPads>,
    device: u32,
    size: u32,
    flags: u32,
) {
    let mut wine_record = Words::position(size, flags);
    // SAFETY: the record holds 1024 bytes, more than `JOYINFOEX`.
    let wine = unsafe { joyGetPosEx(device, wine_record.0.as_mut_ptr()) };
    let mut front_record = Words::position(size, flags);
    let front_answer = answer(|| front.position(device, &mut front_record));
    println!(
        "{kind} device={device:#x} size={size} flags={flags:#x} wine={wine} wine_touched={} padfront={front_answer} padfront_touched={}",
        wine_record.touched(POSITION_ASKED),
        front_record.touched(POSITION_ASKED),
    );
}

static ROW: Row = Row {
    file: "probe",
    calls: Calls {
        devices: "d",
        caps: "c",
        position: "p",
    },
};

fn main() {
    let shown = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        if !info.payload().is::<Stopped>() {
            shown(info);
        }
    }));

    let quiet = Front::new(&ROW, Catches);
    let front = PadFront::new(&ROW, Catches, WinmmPads::new());
    let pads = WinmmPads::new();
    // SAFETY: no argument, no precondition.
    let wine_devices = unsafe { joyGetNumDevs() };
    println!(
        "numdevs wine={wine_devices} quiet={} padfront={} winmm_pads={}",
        quiet.devices(),
        front.devices(),
        pads.devices(),
    );
    for device in NUMBERS {
        for size in CAPS_SIZES {
            caps_cell(&quiet, &front, device, size);
        }
        println!(
            "quiet_position device={device:#x} quiet={}",
            answer(|| quiet.position(device))
        );
        for flags in READ_FLAGS {
            position_cell("position", &front, device, INFO_SIZE, flags);
        }
        println!(
            "pads device={device:#x} shape={} reading={}",
            if pads.shape(device).is_some() {
                "some"
            } else {
                "none"
            },
            if pads.reading(device).is_some() {
                "some"
            } else {
                "none"
            },
        );
    }
    for (size, flags) in BEYOND {
        position_cell("told", &front, 0, size, flags);
    }
}
