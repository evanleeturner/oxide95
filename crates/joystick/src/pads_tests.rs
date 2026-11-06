// SPDX-License-Identifier: MIT
//! The pad front against made-up pads and the stand-in recording, the calls in the
//! numbers a program hands them.

use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::Back;
use crate::pads::{CENTRE, PadFront, Pads, Reading, Record, Shape, Span, has, hat, pov, stick};
use crate::page::{PagePads, WORDS_A_PAD};
use crate::table::{Calls, Row};
use crate::test_log::debug_lines;

static ROW: Row = Row {
    file: "a program",
    calls: Calls {
        devices: "WINMM!joyGetNumDevs",
        caps: "WINMM!joyGetDevCapsA",
        position: "WINMM!joyGetPosEx",
    },
};

/// The frames `page/gamepads.test.mjs` reads too: a stand-in, by its first line.
const RECORDING: &str = include_str!("../page/recorded-pad.txt");

const READ: u32 = 0x483;
const READ_ALL: u32 = 0x483 | 0x7c;
const POSITION_WORDS: usize = 13;
const CAPS_WORDS: usize = 101;

/// A stop a test can catch, where a target ends the program.
struct Panics;

impl Back for Panics {
    fn stop(&self, name: &str) -> ! {
        panic!("stopped: {name}")
    }
}

/// Words that have not been written read as this, so a write shows.
const UNTOUCHED: u32 = 0xdead_beef;

struct Words(Vec<u32>);

impl Record for Words {
    fn word(&self, index: usize) -> u32 {
        self.0[index]
    }

    fn set_word(&mut self, index: usize, value: u32) {
        self.0[index] = value;
    }
}

fn caps_record() -> Words {
    Words(vec![UNTOUCHED; CAPS_WORDS])
}

fn position_record(size: u32, flags: u32) -> Words {
    let mut words = vec![UNTOUCHED; POSITION_WORDS];
    words[0] = size;
    words[1] = flags;
    Words(words)
}

const SHAPE: Shape = Shape {
    spans: [Span::FULL; 6],
    caps: has::R | has::U | has::POV | has::POV_FOUR_WAYS,
    axes: 4,
    buttons: 13,
};

const READING: Reading = Reading {
    axes: [1, 2, 3, 4, 5, 6],
    buttons: 0b101,
    pressed: 2,
    pov: pov::RIGHT,
};

/// A pad that is there, or not, and whose `reading` may answer differently from `shape`.
struct OnePad {
    shape: Option<Shape>,
    reading: Cell<Option<Reading>>,
}

impl Pads for OnePad {
    fn devices(&self) -> u32 {
        u32::from(self.shape.is_some())
    }

    fn shape(&self, device: u32) -> Option<Shape> {
        self.shape.filter(|_| device == 0)
    }

    fn reading(&self, device: u32) -> Option<Reading> {
        self.reading.get().filter(|_| device == 0)
    }
}

fn front(shape: Option<Shape>, reading: Option<Reading>) -> PadFront<Panics, OnePad> {
    PadFront::new(
        &ROW,
        Panics,
        OnePad {
            shape,
            reading: Cell::new(reading),
        },
    )
}

fn no_pad() -> PadFront<Panics, OnePad> {
    front(None, None)
}

fn a_pad() -> PadFront<Panics, OnePad> {
    front(Some(SHAPE), Some(READING))
}

fn stop_of(call: impl FnOnce() -> u32) -> String {
    let stopped = catch_unwind(AssertUnwindSafe(call)).expect_err("the call never returns");
    stopped
        .downcast_ref::<String>()
        .cloned()
        .expect("a stop carries its message")
}

// 5.2: no pads.

#[test]
fn no_pad_counts_none_and_refuses_and_writes_nothing() {
    let pads = no_pad();
    let mut count = 99;
    assert_eq!(
        debug_lines(|| count = pads.devices()),
        ["imp.call c=WINMM!joyGetNumDevs v=0 by=joystick"]
    );
    assert_eq!(count, 0);

    let mut record = caps_record();
    let mut answer = 0;
    let lines = debug_lines(|| answer = pads.caps(0, 404, &mut record));
    assert_eq!(answer, 165);
    assert_eq!(
        lines,
        ["imp.call c=WINMM!joyGetDevCapsA a=0,194 v=a5 by=joystick"]
    );
    assert!(record.0.iter().all(|word| *word == UNTOUCHED));

    let mut record = position_record(0x34, READ);
    let lines = debug_lines(|| answer = pads.position(0, &mut record));
    assert_eq!(answer, 167);
    assert_eq!(
        lines,
        ["imp.call c=WINMM!joyGetPosEx a=0,483 v=a7 o=\"\" by=joystick"]
    );
    assert_eq!(record.0, position_record(0x34, READ).0);
}

#[test]
fn no_pad_refuses_a_caps_call_of_any_size_as_the_quiet_front_does() {
    for size in [0, 52, 404, 0xffff_ffff] {
        let mut record = caps_record();
        assert_eq!(no_pad().caps(0, size, &mut record), 165, "size {size}");
    }
}

// 5.3: a pad pulled mid-run, by the page's own count.

#[test]
fn a_pulled_pad_is_unplugged_with_no_stale_numbers_and_comes_back() {
    let pad: Vec<u32> = vec![1, 9, 9, 9, 9, 9, 9, 54, 13, 0, 0xffff];
    let gone = vec![0; WORDS_A_PAD];
    let front = PadFront::new(&ROW, Panics, PagePads::new());

    front.pads().hand_in(&pad);
    let mut record = position_record(0x34, READ);
    assert_eq!(front.position(0, &mut record), 0);
    assert_eq!(record.0[2], 9);
    assert_eq!(front.devices(), 1);

    front.pads().hand_in(&gone);
    let mut record = position_record(0x34, READ);
    assert_eq!(front.position(0, &mut record), 167);
    assert_eq!(
        record.0,
        position_record(0x34, READ).0,
        "nothing stale is written"
    );
    assert_eq!(front.devices(), 1, "the count holds");

    front.pads().hand_in(&pad);
    let mut record = position_record(0x34, READ);
    assert_eq!(front.position(0, &mut record), 0);
    assert_eq!(front.devices(), 1);
}

#[test]
fn a_pad_pulled_between_the_two_questions_is_unplugged() {
    let pads = front(Some(SHAPE), None);
    let mut record = position_record(0x34, READ);
    assert_eq!(pads.position(0, &mut record), 167);
    assert_eq!(record.0, position_record(0x34, READ).0);
}

// 5.4: flags 0 is presence.

#[test]
fn flags_0_says_whether_a_pad_is_there_and_writes_nothing() {
    for (pads, answer) in [(a_pad(), 0), (no_pad(), 167)] {
        let mut record = position_record(0x34, 0);
        let mut got = 99;
        let lines = debug_lines(|| got = pads.position(0, &mut record));
        assert_eq!(got, answer);
        assert_eq!(
            lines,
            [format!(
                "imp.call c=WINMM!joyGetPosEx a=0,0 v={answer:x} o=\"\" by=joystick"
            )]
        );
        assert_eq!(record.0, position_record(0x34, 0).0);
    }
}

// 5.5: only what was asked.

#[test]
fn a_read_writes_what_was_asked_and_leaves_the_rest() {
    let mut record = position_record(0x34, READ);
    let lines = debug_lines(|| assert_eq!(a_pad().position(0, &mut record), 0));
    assert_eq!(
        record.0,
        [
            0x34, READ, 1, 2, UNTOUCHED, UNTOUCHED, UNTOUCHED, UNTOUCHED, 0b101, 2, UNTOUCHED,
            UNTOUCHED, UNTOUCHED,
        ],
    );
    assert_eq!(
        lines,
        ["imp.call c=WINMM!joyGetPosEx a=0,483 v=0 o=1,2,5,2 by=joystick"]
    );
}

#[test]
fn each_extra_a_read_may_ask_adds_its_own_word_alone() {
    for (flag, at, value) in [
        (4, 4, 3),
        (8, 5, 4),
        (0x10, 6, 5),
        (0x20, 7, 6),
        (0x40, 10, pov::RIGHT),
    ] {
        let mut record = position_record(0x34, READ | flag);
        assert_eq!(a_pad().position(0, &mut record), 0);
        let mut expected = position_record(0x34, READ | flag);
        expected.0[2] = 1;
        expected.0[3] = 2;
        expected.0[8] = 0b101;
        expected.0[9] = 2;
        expected.0[at] = value;
        assert_eq!(record.0, expected.0, "flag {flag:x}");
    }
}

// 5.6: what no answer is written for.

#[test]
fn a_wrong_size_or_any_other_flags_stops_by_name_with_one_line() {
    let refused = [
        (0x30, READ),
        (0, 0),
        (0x34, 0xff),
        (0x34, 1),
        (0x34, READ | 0x200),
        (0x34, READ & !0x400),
        (0x34, READ & !0x80),
        (0x34, READ | 0x100),
    ];
    for (size, flags) in refused {
        let mut record = position_record(size, flags);
        let lines = debug_lines(|| {
            let stop = stop_of(|| a_pad().position(2, &mut record));
            assert_eq!(stop, "stopped: WINMM!joyGetPosEx");
        });
        assert_eq!(
            lines,
            [format!(
                "joy.stop c=WINMM!joyGetPosEx a=2,{size:x},{flags:x} by=joystick"
            )]
        );
        assert_eq!(record.0, position_record(size, flags).0);
    }
}

#[test]
fn a_caps_size_not_the_record_s_stops_by_name_when_a_pad_is_there() {
    for size in [0, 52, 403, 405] {
        let mut record = caps_record();
        let lines = debug_lines(|| {
            let stop = stop_of(|| a_pad().caps(0, size, &mut record));
            assert_eq!(stop, "stopped: WINMM!joyGetDevCapsA");
        });
        assert_eq!(
            lines,
            [format!(
                "joy.stop c=WINMM!joyGetDevCapsA a=0,{size:x} by=joystick"
            )]
        );
        assert!(record.0.iter().all(|word| *word == UNTOUCHED));
    }
}

// 5.7: every span has a size.

#[test]
fn caps_fill_the_whole_record_and_every_span_has_a_size() {
    let shape = Shape {
        spans: [
            Span::FULL,
            Span { min: 0, max: 0 },
            Span { min: 9, max: 9 },
            Span { min: 5, max: 2 },
            Span { min: 10, max: 20 },
            Span {
                min: 0,
                max: u32::MAX,
            },
        ],
        ..SHAPE
    };
    let mut record = caps_record();
    let mut answer = 99;
    let lines = debug_lines(|| {
        answer = front(Some(shape), Some(READING)).caps(0, 404, &mut record);
    });
    assert_eq!(answer, 0);

    let mut expected = vec![0; CAPS_WORDS];
    for (at, min, max) in [
        (9, 0, 65535),
        (11, 0, 65535),
        (13, 0, 65535),
        (18, 0, 65535),
        (20, 10, 20),
        (22, 0, u32::MAX),
    ] {
        expected[at] = min;
        expected[at + 1] = max;
    }
    expected[15] = 13;
    expected[24] = 0x36;
    expected[25] = 6;
    expected[26] = 4;
    expected[27] = 32;
    assert_eq!(record.0, expected);
    for at in [9, 11, 13, 18, 20, 22] {
        assert!(record.0[at + 1] > record.0[at], "the span at word {at}");
    }
    assert_eq!(
        lines,
        ["imp.call c=WINMM!joyGetDevCapsA a=0,194 v=0 \
             o=0,ffff,0,ffff,0,ffff,d,0,0,0,ffff,a,14,0,ffffffff,36,6,4,20 by=joystick"],
    );
}

#[test]
fn a_read_holds_each_axis_to_the_span_the_caps_gave() {
    let shape = Shape {
        spans: [
            Span { min: 10, max: 20 },
            Span { min: 10, max: 20 },
            Span { min: 0, max: 0 },
            Span::FULL,
            Span::FULL,
            Span::FULL,
        ],
        ..SHAPE
    };
    let reading = Reading {
        axes: [5, 25, 70000, 70000, 0, 65535],
        ..READING
    };
    let mut record = position_record(0x34, READ_ALL);
    assert_eq!(
        front(Some(shape), Some(reading)).position(0, &mut record),
        0
    );
    assert_eq!(record.0[2..8], [10, 20, 65535, 65535, 0, 65535]);
}

// 5.1: the stand-in recording, whole.

struct Frame<'a> {
    name: &'a str,
    mapping: &'a str,
    axes: Vec<f32>,
    words: Vec<u32>,
}

fn numbers<T: std::str::FromStr>(text: &str) -> Vec<T> {
    text.split(',')
        .filter(|cell| !cell.trim().is_empty())
        .map(|cell| {
            cell.trim()
                .parse()
                .ok()
                .expect("the recording holds numbers")
        })
        .collect()
}

fn frames() -> Vec<Frame<'static>> {
    RECORDING
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| {
            let cells: Vec<&str> = line.split('|').map(str::trim).collect();
            assert_eq!(cells.len(), 6, "{line}");
            Frame {
                name: cells[0],
                mapping: cells[1],
                axes: numbers(cells[3]),
                words: numbers(cells[5]),
            }
        })
        .collect()
}

#[test]
fn the_recording_says_what_it_is_in_its_first_line() {
    let first = RECORDING
        .lines()
        .next()
        .expect("the recording has a first line");
    assert!(
        first.contains("A stand-in") && first.contains("no device"),
        "{first}"
    );
}

#[test]
fn every_recorded_frame_reaches_the_record_in_range() {
    let frames = frames();
    assert!(frames.len() >= 10, "{} frames", frames.len());
    for frame in &frames {
        assert_eq!(frame.words.len(), WORDS_A_PAD, "{}", frame.name);
        let front = PadFront::new(&ROW, Panics, PagePads::new());
        front.pads().hand_in(&frame.words);
        let mut record = position_record(0x34, READ_ALL);
        let answer = front.position(0, &mut record);
        if frame.words[0] == 0 {
            assert_eq!(answer, 167, "{}", frame.name);
            assert_eq!(
                record.0,
                position_record(0x34, READ_ALL).0,
                "{}",
                frame.name
            );
            continue;
        }
        assert_eq!(answer, 0, "{}", frame.name);
        assert_eq!(
            record.0[2..8],
            frame.words[1..7],
            "{}: the axes",
            frame.name
        );
        assert!(
            record.0[2..8].iter().all(|axis| *axis <= 65535),
            "{}",
            frame.name
        );
        assert_eq!(record.0[8], frame.words[9], "{}: the buttons", frame.name);
        assert_eq!(
            record.0[9],
            frame.words[9].count_ones(),
            "{}: how many",
            frame.name
        );
        assert_eq!(record.0[10], frame.words[10], "{}: the hat", frame.name);
    }
}

/// `gilrs` shares this sum with the script, and has no pad here to prove it on.
#[test]
fn the_host_s_stick_is_the_script_s_on_every_recorded_axis() {
    for frame in frames().iter().filter(|frame| frame.words[0] != 0) {
        let at: &[usize] = if frame.mapping == "standard" {
            &[1, 2, 4, 5]
        } else {
            &[1, 2, 3, 4, 5, 6]
        };
        for (value, word) in frame.axes.iter().zip(at) {
            assert_eq!(
                stick(*value),
                frame.words[*word],
                "{}: axis {value}",
                frame.name
            );
        }
        if frame.mapping == "standard" {
            assert_eq!(frame.words[3], CENTRE, "{}: no Z", frame.name);
            assert_eq!(frame.words[6], CENTRE, "{}: no V", frame.name);
        }
    }
}

#[test]
fn a_stick_with_no_number_reads_at_rest() {
    assert_eq!(stick(f32::NAN), CENTRE);
    assert_eq!(stick(f32::INFINITY), CENTRE);
    assert_eq!(stick(f32::NEG_INFINITY), CENTRE);
}

#[test]
fn a_diagonal_reads_as_the_first_of_up_right_down_left() {
    assert_eq!(hat(false, false, false, false), pov::CENTRED);
    assert_eq!(hat(true, true, false, false), pov::FORWARD);
    assert_eq!(hat(false, true, true, false), pov::RIGHT);
    assert_eq!(hat(false, false, true, true), pov::BACK);
    assert_eq!(hat(true, false, false, true), pov::FORWARD);
    assert_eq!(hat(false, false, false, true), pov::LEFT);
}

// The page's numbers.

#[test]
fn a_hand_in_of_the_wrong_length_is_no_pads_and_one_line() {
    let pads = PagePads::new();
    pads.hand_in(&[1, 9, 9, 9, 9, 9, 9, 54, 13, 0, 0xffff]);
    assert!(pads.reading(0).is_some());
    for bad in [vec![1; 10], vec![1; 12], vec![1; WORDS_A_PAD * 5]] {
        let lines = debug_lines(|| {
            pads.hand_in(&bad);
            pads.hand_in(&bad);
        });
        let shown = bad.len();
        assert_eq!(lines, [format!("joy.page.words n={shown:x} by=joystick")]);
        assert!(pads.reading(0).is_none());
        assert_eq!(pads.devices(), 1, "the count holds");
        pads.hand_in(&[]);
    }
}

#[test]
fn the_count_is_the_highest_slot_seen_and_is_0_before_any() {
    let pads = PagePads::new();
    assert_eq!(pads.devices(), 0);
    pads.hand_in(&[]);
    pads.hand_in(&[0; WORDS_A_PAD * 4]);
    assert_eq!(pads.devices(), 0, "nulls are no pad");

    let mut third = vec![0; WORDS_A_PAD * 3];
    third[WORDS_A_PAD * 2] = 1;
    pads.hand_in(&third);
    assert_eq!(pads.devices(), 3);
    assert!(pads.reading(1).is_none());
    assert!(pads.reading(2).is_some());
    assert!(pads.reading(3).is_none());
    assert!(pads.reading(0xffff_ffff).is_none());

    pads.hand_in(&[]);
    assert_eq!(pads.devices(), 3, "the count never goes down");
    assert!(pads.reading(2).is_none());
}

#[test]
fn a_page_pad_s_shape_is_its_words() {
    let pads = PagePads::new();
    pads.hand_in(&[1, 0, 0, 0, 0, 0, 0, 54, 13, 0, 0xffff]);
    assert_eq!(pads.shape(0), Some(SHAPE));
    pads.hand_in(&[1, 0, 0, 0, 0, 0, 0, 15, 6, 0, 0xffff]);
    assert_eq!(pads.shape(0).map(|shape| shape.axes), Some(6));
    pads.hand_in(&[1, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0xffff]);
    assert_eq!(pads.shape(0).map(|shape| shape.axes), Some(2));
}
