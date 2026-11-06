// SPDX-License-Identifier: MIT
//! The front against made-up rows, the calls in the words a program hands them.

use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::table::{Calls, Row};
use crate::test_log::debug_lines;
use crate::{Back, Front, Record};

const CALLS: Calls = Calls {
    send_command: "WINMM!mciSendCommandA",
    aux_devices: "WINMM!auxGetNumDevs",
    error_string: "WINMM!mciGetErrorStringA",
    aux_caps: "WINMM!auxGetDevCapsA",
    aux_set_volume: "WINMM!auxSetVolume",
    aux_get_volume: "WINMM!auxGetVolume",
};

static ROW: Row = Row {
    file: "a program",
    calls: CALLS,
    letters: 1,
    tracks: 22,
    device_name: "cdaudio",
    device_id: 7,
};

static SMALL: Row = Row {
    file: "another",
    calls: CALLS,
    letters: 3,
    tracks: 7,
    device_name: "disc",
    device_id: 2,
};

/// A stop a test can catch, where a target ends the program.
struct Panics;

impl Back for Panics {
    fn stop(&self, name: &str) -> ! {
        panic!("stopped: {name}")
    }
}

/// A record: its 5 words, and what went through its return pointer.
struct Words {
    words: [u32; 5],
    through: Vec<u8>,
}

fn record(words: [u32; 5]) -> Words {
    Words {
        words,
        through: Vec::new(),
    }
}

impl Record for Words {
    fn word(&self, index: usize) -> u32 {
        self.words[index]
    }

    fn set_word(&mut self, index: usize, value: u32) {
        self.words[index] = value;
    }

    fn write_through(&mut self, index: usize, chunks: &[&[u8]]) {
        assert_eq!(index, 1, "only the return word holds an address");
        for chunk in chunks {
            self.through.extend_from_slice(chunk);
        }
    }
}

/// A record no call may look at: a message that carries none may be handed null.
struct Untouched;

impl Record for Untouched {
    fn word(&self, _: usize) -> u32 {
        panic!("read a record the message does not carry")
    }

    fn set_word(&mut self, _: usize, _: u32) {
        panic!("wrote a record the message does not carry")
    }

    fn write_through(&mut self, _: usize, _: &[&[u8]]) {
        panic!("wrote through a record the message does not carry")
    }
}

/// What a call that stops said: the stop's name, and the lines it logged.
fn stopped(call: impl FnOnce()) -> (String, Vec<String>) {
    let mut name = String::new();
    let lines = debug_lines(|| {
        let panic = catch_unwind(AssertUnwindSafe(call)).expect_err("the call stops");
        name.clone_from(
            panic
                .downcast_ref::<String>()
                .expect("the stop names itself"),
        );
    });
    (name, lines)
}

#[test]
fn the_8_calls_answer_in_order_and_each_line_says_who() {
    let front = Front::new(&ROW, Panics);
    let lines = debug_lines(|| {
        let mut count = record([0, 0xAA, 4, 0, 0x204]);
        assert_eq!(front.send(0, 0x810, 0x100, &mut count), 0);
        assert_eq!(count.through, 1u32.to_le_bytes());

        let mut name = record([0, 0xBB, 0x20, 1, 0x204]);
        assert_eq!(front.send(0, 0x810, 0x400, &mut name), 0);
        assert_eq!(name.through, b"cdaudio\0");

        let mut open = record([0; 5]);
        assert_eq!(front.send(0, 0x803, 0x2000, &mut open), 0);
        assert_eq!(open.words[1], 7);

        assert_eq!(
            front.send(7, 0x80d, 0x400, &mut record([0, 10, 0, 0, 0])),
            0
        );

        let mut tracks = record([0, 0, 3, 0, 0]);
        assert_eq!(front.send(7, 0x814, 0x100, &mut tracks), 0);
        assert_eq!(tracks.words[1], 22);

        assert_eq!(front.aux_devices(), 0);

        let mut mode = record([0, 0x55, 4, 0, 0]);
        assert_eq!(front.send(7, 0x814, 0x100, &mut mode), 274);
        assert_eq!(
            mode.words[1], 0x55,
            "no mode is answered, the open mode least of all"
        );

        assert_eq!(front.send(7, 0x804, 0, &mut Untouched), 0);
    });
    assert_eq!(
        lines,
        [
            "imp.call c=WINMM!mciSendCommandA a=0,810,100,204,4 v=0 o=1 by=cd-audio",
            "imp.call c=WINMM!mciSendCommandA a=0,810,400,204,1,20 v=0 o=cdaudio by=cd-audio",
            "imp.call c=WINMM!mciSendCommandA a=0,803,2000 v=0 o=7 by=cd-audio",
            "imp.call c=WINMM!mciSendCommandA a=7,80d,400,a v=0 by=cd-audio",
            "imp.call c=WINMM!mciSendCommandA a=7,814,100,3 v=0 o=16 by=cd-audio",
            "imp.call c=WINMM!auxGetNumDevs v=0 by=cd-audio",
            "imp.call c=WINMM!mciSendCommandA a=7,814,100,4 v=112 by=cd-audio",
            "imp.call c=WINMM!mciSendCommandA a=7,804,0 v=0 by=cd-audio",
        ]
    );
}

#[test]
fn counts_names_and_numbers_come_from_the_row() {
    let front = Front::new(&SMALL, Panics);
    let mut count = record([0, 0xAA, 4, 0, 0x204]);
    assert_eq!(front.send(0, 0x810, 0x100, &mut count), 0);
    assert_eq!(count.through, 3u32.to_le_bytes());

    // The last device the count allows, in a buffer just big enough.
    let mut name = record([0, 0xBB, 5, 3, 0x204]);
    assert_eq!(front.send(0, 0x810, 0x400, &mut name), 0);
    assert_eq!(name.through, b"disc\0");

    let mut open = record([0; 5]);
    assert_eq!(front.send(0, 0x803, 0x2000, &mut open), 0);
    assert_eq!(open.words[1], 2);

    let mut tracks = record([0, 0, 3, 0, 0]);
    assert_eq!(front.send(2, 0x814, 0x100, &mut tracks), 0);
    assert_eq!(tracks.words[1], 7);
}

#[test]
fn every_other_send_stops_by_name_before_it_writes() {
    let front = Front::new(&ROW, Panics);
    let others: [(u32, u32, u32, [u32; 5]); 18] = [
        (7, 0x806, 0x4, [0; 5]),                   // MCI_PLAY
        (7, 0x809, 0, [0; 5]),                     // MCI_PAUSE
        (7, 0x855, 0, [0; 5]),                     // MCI_RESUME
        (7, 0x808, 0, [0; 5]),                     // MCI_STOP
        (7, 0x80d, 0x100, [0; 5]),                 // MCI_SET, other flags
        (7, 0x814, 0x110, [0, 0, 2, 0, 0]),        // MCI_STATUS, other flags
        (7, 0x814, 0x100, [0, 0, 2, 0, 0]),        // an item no row answers
        (7, 0x80d, 0x400, [0, 3, 0, 0, 0]),        // another time format
        (0, 0x80d, 0x400, [0, 10, 0, 0, 0]),       // no device yet
        (u32::MAX, 0x804, 0, [0; 5]),              // a device already closed
        (7, 0x803, 0x2000, [0; 5]),                // open on an open device
        (0, 0x803, 0x200, [0; 5]),                 // open, other flags
        (7, 0x810, 0x100, [0, 0, 4, 0, 0x204]),    // the count, on a device
        (0, 0x810, 0x100, [0, 0, 4, 0, 0x205]),    // the count of another type
        (0, 0x810, 0x100, [0, 0, 3, 0, 0x204]),    // no room for the count
        (0, 0x810, 0x400, [0, 0, 0x20, 0, 0x204]), // device 0
        (0, 0x810, 0x400, [0, 0, 0x20, 2, 0x204]), // past the count
        (0, 0x810, 0x400, [0, 0, 7, 1, 0x204]),    // no room for the zero
    ];
    for (device, message, flags, words) in others {
        let mut asked = record(words);
        let (name, lines) = stopped(|| {
            let _ = front.send(device, message, flags, &mut asked);
        });
        assert_eq!(
            name, "stopped: WINMM!mciSendCommandA",
            "{message:#x} {flags:#x}"
        );
        assert_eq!(lines.len(), 1, "{message:#x} {flags:#x}: {lines:?}");
        assert!(lines[0].starts_with("cd.stop "), "{}", lines[0]);
        assert!(lines[0].ends_with("by=cd-audio"), "{}", lines[0]);
        assert_eq!(
            asked.words, words,
            "{message:#x} {flags:#x}: wrote before stopping"
        );
        assert!(
            asked.through.is_empty(),
            "{message:#x} {flags:#x}: wrote before stopping"
        );
    }
}

#[test]
fn the_4_names_past_the_test_stop_by_name() {
    let front = Front::new(&ROW, Panics);
    let (name, lines) = stopped(|| front.error_string(274, 0x20));
    assert_eq!(name, "stopped: WINMM!mciGetErrorStringA");
    assert_eq!(
        lines,
        ["cd.stop c=WINMM!mciGetErrorStringA a=112,20 by=cd-audio"]
    );
    let (name, lines) = stopped(|| front.aux_caps(0, 0x30));
    assert_eq!(name, "stopped: WINMM!auxGetDevCapsA");
    assert_eq!(lines, ["cd.stop c=WINMM!auxGetDevCapsA a=0,30 by=cd-audio"]);
    let (name, lines) = stopped(|| front.aux_set_volume(0, 0xffff_ffff));
    assert_eq!(name, "stopped: WINMM!auxSetVolume");
    assert_eq!(
        lines,
        ["cd.stop c=WINMM!auxSetVolume a=0,ffffffff by=cd-audio"]
    );
    let (name, lines) = stopped(|| front.aux_get_volume(0));
    assert_eq!(name, "stopped: WINMM!auxGetVolume");
    assert_eq!(lines, ["cd.stop c=WINMM!auxGetVolume a=0 by=cd-audio"]);
}
