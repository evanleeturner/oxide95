// SPDX-License-Identifier: MIT
//! The page's words through `PagePads` to what a program reads back, for `page_check.py` to
//! judge. Native, as `page.rs` names no target.
//!
//! Standard input: one frame a line, its words by spaces. Each frame gets a fresh `PagePads`,
//! so a frame's answers are its own. Then `stick`'s sum over a sweep of -1 to 1 by 1/1024 and
//! the three numbers that are not finite. A word no call wrote prints as `-`.

use std::io::{self, BufRead};

use joystick::Back;
use joystick::pads::{self, PadFront, Record};
use joystick::page::PagePads;
use joystick::table::{Calls, Row};

/// `sizeof(JOYCAPSA)` and `sizeof(JOYINFOEX)`.
const CAPS_SIZE: u32 = 404;
const INFO_SIZE: u32 = 52;
/// `JOY_RETURNALL | JOY_RETURNCENTERED`.
const READ_ALL: u32 = 0x4ff;
const UNTOUCHED: u32 = 0xa5a5_a5a5;
const CAPS_WORDS: usize = 101;
const INFO_WORDS: usize = 13;
const SWEEP_STEPS: i16 = 1024;

struct Words([u32; CAPS_WORDS]);

impl Words {
    fn shown(&self, count: usize) -> String {
        let words: Vec<String> = self.0[..count]
            .iter()
            .map(|word| {
                if *word == UNTOUCHED {
                    "-".to_owned()
                } else {
                    word.to_string()
                }
            })
            .collect();
        words.join(",")
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

struct Stops;

impl Back for Stops {
    fn stop(&self, name: &str) -> ! {
        panic!("stop {name}")
    }
}

static ROW: Row = Row {
    file: "page-pads",
    calls: Calls {
        devices: "d",
        caps: "c",
        position: "p",
    },
};

fn main() -> io::Result<()> {
    for line in io::stdin().lock().lines() {
        let handed: Vec<u32> = line?
            .split_whitespace()
            .map(|word| word.parse().expect("a word is a number"))
            .collect();
        let page = PagePads::new();
        page.hand_in(&handed);
        let front = PadFront::new(&ROW, Stops, page);

        let mut caps = Words([UNTOUCHED; CAPS_WORDS]);
        let caps_answer = front.caps(0, CAPS_SIZE, &mut caps);
        let mut info = Words([UNTOUCHED; CAPS_WORDS]);
        info.0[0] = INFO_SIZE;
        info.0[1] = READ_ALL;
        let info_answer = front.position(0, &mut info);
        println!(
            "frame caps={caps_answer} caps_words={} position={info_answer} position_words={}",
            caps.shown(CAPS_WORDS),
            info.shown(INFO_WORDS),
        );
    }
    for step in -SWEEP_STEPS..=SWEEP_STEPS {
        let value = f32::from(step) / f32::from(SWEEP_STEPS);
        println!("stick step={step} rust={}", pads::stick(value));
    }
    for (name, value) in [
        ("nan", f32::NAN),
        ("inf", f32::INFINITY),
        ("-inf", f32::NEG_INFINITY),
    ] {
        println!("stick step={name} rust={}", pads::stick(value));
    }
    Ok(())
}
