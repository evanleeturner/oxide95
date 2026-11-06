// SPDX-License-Identifier: MIT
//! The front against made-up rows, the calls in the numbers a program hands them.

use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::table::{Calls, Row};
use crate::test_log::debug_lines;
use crate::{Back, Front};

static ROW: Row = Row {
    file: "a program",
    calls: Calls {
        devices: "WINMM!joyGetNumDevs",
        caps: "WINMM!joyGetDevCapsA",
        position: "WINMM!joyGetPosEx",
    },
};

static OTHER: Row = Row {
    file: "another",
    calls: Calls {
        devices: "OTHER!devices",
        caps: "OTHER!caps",
        position: "OTHER!position",
    },
};

/// A stop a test can catch, where a target ends the program.
struct Panics;

impl Back for Panics {
    fn stop(&self, name: &str) -> ! {
        panic!("stopped: {name}")
    }
}

fn front(row: &'static Row) -> Front<Panics> {
    Front::new(row, Panics)
}

#[test]
fn the_count_is_none_with_one_line() {
    let mut count = 99;
    let lines = debug_lines(|| count = front(&ROW).devices());
    assert_eq!(count, 0);
    assert_eq!(lines, ["imp.call c=WINMM!joyGetNumDevs v=0 by=joystick"],);
}

#[test]
fn caps_are_refused_for_every_device_and_size() {
    for (device, size) in [(0, 404), (1, 404), (15, 52), (0xffff_ffff, 0)] {
        let mut answer = 0;
        let lines = debug_lines(|| answer = front(&ROW).caps(device, size));
        assert_eq!(answer, 165, "JOYERR_PARMS, for device {device}");
        assert_eq!(
            lines,
            [format!(
                "imp.call c=WINMM!joyGetDevCapsA a={device:x},{size:x} v=a5 by=joystick"
            )],
        );
    }
}

#[test]
fn a_position_stops_by_name_and_writes_one_line() {
    let lines = debug_lines(|| {
        let stopped = catch_unwind(AssertUnwindSafe(|| front(&ROW).position(3)));
        let message = stopped.expect_err("the call never returns");
        assert_eq!(
            message.downcast_ref::<String>().map(String::as_str),
            Some("stopped: WINMM!joyGetPosEx"),
        );
    });
    assert_eq!(lines, ["joy.stop c=WINMM!joyGetPosEx a=3 by=joystick"]);
}

#[test]
fn every_name_on_a_line_is_the_row_s() {
    let lines = debug_lines(|| {
        let answers = front(&OTHER);
        let _ = answers.devices();
        let _ = answers.caps(0, 0);
        let _ = catch_unwind(AssertUnwindSafe(|| answers.position(0)));
    });
    assert_eq!(lines.len(), 3);
    for (line, name) in lines
        .iter()
        .zip(["OTHER!devices", "OTHER!caps", "OTHER!position"])
    {
        assert!(line.contains(&format!("c={name} ")), "{line}");
    }
}
