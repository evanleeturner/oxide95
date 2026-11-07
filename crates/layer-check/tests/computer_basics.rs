// SPDX-License-Identifier: MIT
//! The bench's four rows against this computer, and what its judge calls red.
use std::fs;
use std::process;

#[allow(dead_code, reason = "`main` and the printing are the bench's own")]
#[path = "../bench/computer_basics.rs"]
mod program;

use program::{
    CLOCK_RED, FILE_RED, FileRow, HEAP_RED, HeapRow, Nest, REFUSALS, ROUNDS, Rows, THREAD_RED,
    ThreadRow, measure, reds,
};

fn nest(outer_us: u64, inner_us: u64, grain_us: u64) -> Nest {
    Nest {
        outer: "outer",
        inner: "inner",
        outer_us,
        inner_us,
        grain_us,
    }
}

fn all_hold() -> Rows {
    Rows {
        clock: vec![nest(250_000, 250_000, 0)],
        file: FileRow {
            wrote: 8,
            back: 8,
            same: true,
            sum: 1,
            refused: None,
        },
        thread: ThreadRow {
            refused: REFUSALS,
            in_while_held: false,
            in_on_release: true,
            counted: 2 * ROUNDS,
        },
        heap: HeapRow {
            gave: true,
            sized: true,
            apart: true,
            held: true,
        },
    }
}

/// `CLOCK_RED` and the rest name a computer with no code of ours under it, so this test is the
/// one row of the check in `cargo test`: the plants that prove it can go red are run from
/// outside, as no plant ships.
#[test]
fn this_computer_answers_all_four_rows() {
    let file = std::env::temp_dir().join(format!("computer-basics-{}.bin", process::id()));
    let rows = measure(file.to_str().expect("the temporary folder's name is text"));
    fs::remove_file(&file).expect("the file row left its file");
    assert_eq!(reds(&rows), [], "{rows:?}");
}

#[test]
fn a_good_reading_has_no_red() {
    assert_eq!(reds(&all_hold()), []);
}

#[test]
fn a_clock_is_red_when_it_stands_still() {
    for stopped in [nest(0, 250_000, 16_000), nest(250_000, 0, 16_000)] {
        let rows = Rows {
            clock: vec![stopped],
            ..all_hold()
        };
        assert_eq!(reds(&rows), [("clock", CLOCK_RED)]);
    }
}

#[test]
fn a_clock_is_red_past_the_grain_and_not_at_it() {
    let at = Rows {
        clock: vec![nest(250_000, 266_000, 16_000)],
        ..all_hold()
    };
    assert_eq!(reds(&at), []);
    let past = Rows {
        clock: vec![nest(250_000, 266_001, 16_000)],
        ..all_hold()
    };
    assert_eq!(reds(&past), [("clock", CLOCK_RED)]);
}

#[test]
fn no_clock_read_is_red() {
    let rows = Rows {
        clock: vec![],
        ..all_hold()
    };
    assert_eq!(reds(&rows), [("clock", CLOCK_RED)]);
}

#[test]
fn a_file_is_red_when_it_differs_is_short_or_is_refused() {
    let good = all_hold().file;
    let wrong = [
        FileRow {
            same: false,
            ..good.clone()
        },
        FileRow {
            back: 7,
            ..good.clone()
        },
        FileRow {
            refused: Some("no".to_owned()),
            ..good
        },
    ];
    for file in wrong {
        let rows = Rows { file, ..all_hold() };
        assert_eq!(reds(&rows), [("file", FILE_RED)]);
    }
}

#[test]
fn a_thread_is_red_when_it_got_past_the_lock_or_never_did() {
    let good = all_hold().thread;
    let wrong = [
        ThreadRow {
            refused: REFUSALS - 1,
            ..good
        },
        ThreadRow {
            in_while_held: true,
            ..good
        },
        ThreadRow {
            in_on_release: false,
            ..good
        },
        ThreadRow {
            counted: 2 * ROUNDS - 1,
            ..good
        },
    ];
    for thread in wrong {
        let rows = Rows {
            thread,
            ..all_hold()
        };
        assert_eq!(reds(&rows), [("thread", THREAD_RED)]);
    }
}

#[test]
fn a_heap_is_red_on_any_one_answer() {
    let good = all_hold().heap;
    let wrong = [
        HeapRow {
            gave: false,
            ..good
        },
        HeapRow {
            sized: false,
            ..good
        },
        HeapRow {
            apart: false,
            ..good
        },
        HeapRow {
            held: false,
            ..good
        },
    ];
    for heap in wrong {
        let rows = Rows { heap, ..all_hold() };
        assert_eq!(reds(&rows), [("heap", HEAP_RED)]);
    }
}
