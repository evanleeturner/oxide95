// SPDX-License-Identifier: MIT
//! The `gilrs` back really started, on a box with no pad: the library comes up or says
//! why not, and the program hears what it hears from a quiet front, with no fault.
//!
//! A box with a pad has nothing to say here: the count is printed and the test ends.
//! No test reads a real pad through this back.
#![cfg(all(feature = "gilrs", any(target_os = "linux", target_os = "macos")))]

use joystick::Back;
use joystick::gilrs_back::GilrsPads;
use joystick::pads::{PadFront, Record};
use joystick::table::{Calls, Row};

static ROW: Row = Row {
    file: "a program",
    calls: Calls {
        devices: "WINMM!joyGetNumDevs",
        caps: "WINMM!joyGetDevCapsA",
        position: "WINMM!joyGetPosEx",
    },
};

struct Panics;

impl Back for Panics {
    fn stop(&self, name: &str) -> ! {
        panic!("stopped: {name}")
    }
}

struct Words(Vec<u32>);

impl Record for Words {
    fn word(&self, index: usize) -> u32 {
        self.0[index]
    }

    fn set_word(&mut self, index: usize, value: u32) {
        self.0[index] = value;
    }
}

const UNTOUCHED: u32 = 0xdead_beef;

/// A lock makes a thing shareable only if the thing can move to another thread at all.
const _: fn() = || {
    fn moves_between_threads<T: Send>() {}
    moves_between_threads::<GilrsPads>();
};

fn position_record(flags: u32) -> Words {
    let mut words = vec![UNTOUCHED; 13];
    words[0] = 0x34;
    words[1] = flags;
    Words(words)
}

#[test]
fn with_no_pad_on_the_box_the_answers_are_the_quiet_ones() {
    let front = PadFront::new(&ROW, Panics, GilrsPads::new());
    let count = front.devices();
    println!("gilrs counts {count} pads");
    if count != 0 {
        return;
    }
    let mut caps = Words(vec![UNTOUCHED; 101]);
    assert_eq!(front.caps(0, 404, &mut caps), 165);
    assert!(caps.0.iter().all(|word| *word == UNTOUCHED));

    for flags in [0, 0x483, 0x4ff] {
        let mut record = position_record(flags);
        assert_eq!(front.position(0, &mut record), 167, "flags {flags:x}");
        assert_eq!(record.0, position_record(flags).0, "flags {flags:x}");
    }
}
