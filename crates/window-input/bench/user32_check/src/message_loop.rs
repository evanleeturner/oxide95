// SPDX-License-Identifier: MIT
//! `wu2`: `window::user32`'s 9 message calls. A posted message is seen, taken and gone; a
//! procedure runs and answers through each door; a window's timer and a windowless timer each
//! bring their `0x113`, and dispatching it runs what it should.
use std::process::ExitCode;
use std::sync::atomic::{AtomicU32, Ordering::Relaxed};
use std::thread;
use std::time::{Duration, Instant};

use window::user32 as thin;

use crate::bench::{
    Bench, FROM, NO_WINDOW, Plant, Same, Want, address, code, spot, take_lines, text,
};
use crate::hand::{self, Message};

const NAMES: [&str; 9] = [
    "DefWindowProcA",
    "PeekMessageA",
    "TranslateMessage",
    "DispatchMessageA",
    "PostMessageA",
    "SendMessageA",
    "PostQuitMessage",
    "WaitMessage",
    "GetMessageTime",
];
const SENTENCE: &str =
    "A message never drains through the real USER32. The thin call, not the program.";
const HIDDEN_POPUP: u32 = 0x8000_0000;
const WM_GETTEXTLENGTH: u32 = 0x0e;
const WM_QUIT: u32 = 0x12;
const WM_KEYDOWN: u32 = 0x100;
const WM_CHAR: u32 = 0x102;
const WM_TIMER: u32 = 0x113;
/// Messages of this program's own, from `WM_APP`.
const POSTED: u32 = 0x8001;
const CALLED: u32 = 0x8002;
const UNKNOWN: u32 = 0x8003;
const WOKEN: u32 = 0x8004;
/// How many peeks a queue with one message in it may answer before it counts as never draining.
const DRAIN_LIMIT: u32 = 64;
const TIMER_TURNS: u32 = 400;
const TIMER_TURN: Duration = Duration::from_millis(5);
const WAKE_AFTER: Duration = Duration::from_millis(60);
const WAITED_AT_LEAST: Duration = Duration::from_millis(30);

static CALLED_RUNS: AtomicU32 = AtomicU32::new(0);
static WINDOW_TIMER_RUNS: AtomicU32 = AtomicU32::new(0);
static TIMER_PROCEDURE_RUNS: AtomicU32 = AtomicU32::new(0);

/// Answers `w + l` to a call of its own, so the answer shows it ran; the rest goes on by hand.
unsafe extern "system" fn procedure(window: u32, message: u32, w: u32, l: u32) -> u32 {
    match message {
        CALLED => {
            CALLED_RUNS.fetch_add(1, Relaxed);
            w + l
        }
        WM_TIMER => {
            WINDOW_TIMER_RUNS.fetch_add(1, Relaxed);
            0
        }
        _ => hand::def_window_proc(window, message, w, l),
    }
}

unsafe extern "system" fn timer_procedure(_: u32, _: u32, _: u32, _: u32) {
    TIMER_PROCEDURE_RUNS.fetch_add(1, Relaxed);
}

/// Seen, taken, the time read, and gone: the plant's copy never removes, so it never drains.
fn posted_drains(bench: &mut Bench, t: u32) {
    let remove = u32::from(bench.plant != Plant::KeepMessage);
    let want = Want::a(&[t, POSTED, 0x11, 0x22]);
    // SAFETY: plain numbers in `w` and `l`.
    bench.alone("PostMessageA", &want, || unsafe {
        thin::post_message(FROM, t, POSTED, 0x11, 0x22)
    });
    let mut seen = [0; 7];
    let there = hand::peek_message(&mut seen, t, POSTED, POSTED, 0);
    bench.agree(
        "PostMessageA",
        "the message it posted",
        (there, seen[0], seen[1], seen[2], seen[3]),
        (1, t, POSTED, 0x11, 0x22),
    );
    let mut taken = [0; 7];
    let want = Want::a(&[t, POSTED, POSTED, 1]).o(&seen);
    bench.alone("PeekMessageA", &want, || {
        // SAFETY: a record of the size it writes.
        unsafe { thin::peek_message(FROM, spot(&mut taken), t, POSTED, POSTED, remove) }
    });
    bench.agree("PeekMessageA", "the message", taken, seen);
    let [_, time] = bench.beside(
        "GetMessageTime",
        Same::Answer,
        &Want::default(),
        || thin::get_message_time(FROM),
        hand::get_message_time,
    );
    bench.worked("GetMessageTime", "the message's time", time, taken[4]);

    let mut left = 0;
    let mut record = [0; 7];
    while left < DRAIN_LIMIT
        // SAFETY: as above.
        && unsafe { thin::peek_message(FROM, spot(&mut record), t, POSTED, POSTED, remove) } != 0
    {
        left += 1;
    }
    take_lines();
    if left != 0 {
        bench.thin_red(
            "PeekMessageA",
            &format!("a taken message answered {left} more peeks"),
        );
    }
    while hand::peek_message(&mut record, t, POSTED, POSTED, 1) != 0 {}

    let mut untouched = [9; 7];
    let want = Want::a(&[t, POSTED, POSTED, 1]).o(&[]);
    bench.alone("PeekMessageA", &want, || {
        // SAFETY: as above.
        unsafe { thin::peek_message(FROM, spot(&mut untouched), t, POSTED, POSTED, 1) }
    });
    bench.agree("PeekMessageA", "an empty peek's record", untouched, [9; 7]);

    hand::post_message(t, POSTED, 0x11, 0x22);
    let by_hand = [
        hand::peek_message(&mut record, t, POSTED, POSTED, 1),
        hand::peek_message(&mut record, t, POSTED, POSTED, 1),
    ];
    bench.worked("PeekMessageA", "one message, then none", by_hand, [1, 0]);
}

fn translate(bench: &mut Bench, t: u32) {
    let key: Message = [t, WM_KEYDOWN, 0x41, 0x001e_0001, 0, 0, 0];
    let [_, answer] = bench.beside(
        "TranslateMessage",
        Same::Answer,
        &Want::a(&key),
        // SAFETY: a whole record.
        || unsafe { thin::translate_message(FROM, address(&key)) },
        || hand::translate_message(&key),
    );
    bench.worked("TranslateMessage", "a key's answer", answer, 1);
    let (mut first, mut second) = ([0; 7], [0; 7]);
    hand::peek_message(&mut first, t, WM_CHAR, WM_CHAR, 1);
    hand::peek_message(&mut second, t, WM_CHAR, WM_CHAR, 1);
    bench.agree("TranslateMessage", "the character", first[2], second[2]);
    bench.worked("TranslateMessage", "the character", second[2], 0x61);
    let other: Message = [t, UNKNOWN, 0, 0, 0, 0, 0];
    let [_, answer] = bench.beside(
        "TranslateMessage",
        Same::Answer,
        &Want::a(&other),
        // SAFETY: as above.
        || unsafe { thin::translate_message(FROM, address(&other)) },
        || hand::translate_message(&other),
    );
    bench.worked("TranslateMessage", "not a key's answer", answer, 0);
}

fn doors(bench: &mut Bench, t: u32) {
    let before = CALLED_RUNS.load(Relaxed);
    let called: Message = [t, CALLED, 5, 6, 0, 0, 0];
    let [_, answer] = bench.beside(
        "DispatchMessageA",
        Same::Answer,
        &Want::a(&called),
        // SAFETY: a whole record; the procedure is this program's own.
        || unsafe { thin::dispatch_message(FROM, address(&called)) },
        || hand::dispatch_message(&called),
    );
    bench.worked("DispatchMessageA", "the procedure's answer", answer, 11);
    let bad: Message = [NO_WINDOW, CALLED, 5, 6, 0, 0, 0];
    bench.beside(
        "DispatchMessageA",
        Same::Answer,
        &Want::a(&bad),
        // SAFETY: as above.
        || unsafe { thin::dispatch_message(FROM, address(&bad)) },
        || hand::dispatch_message(&bad),
    );
    bench.agree(
        "DispatchMessageA",
        "the procedure's runs",
        CALLED_RUNS.load(Relaxed) - before,
        2,
    );

    let before = CALLED_RUNS.load(Relaxed);
    let [_, answer] = bench.beside(
        "SendMessageA",
        Same::Answer,
        &Want::a(&[t, CALLED, 7, 8]),
        // SAFETY: plain numbers; the procedure is this program's own.
        || unsafe { thin::send_message(FROM, t, CALLED, 7, 8) },
        || hand::send_message(t, CALLED, 7, 8),
    );
    bench.worked("SendMessageA", "the procedure's answer", answer, 15);
    bench.agree(
        "SendMessageA",
        "the procedure's runs",
        CALLED_RUNS.load(Relaxed) - before,
        2,
    );

    for (message, value) in [(WM_GETTEXTLENGTH, 4), (UNKNOWN, 0)] {
        let [_, answer] = bench.beside(
            "DefWindowProcA",
            Same::Answer,
            &Want::a(&[t, message, 0, 0]),
            // SAFETY: neither message reads `w` or `l` as an address.
            || unsafe { thin::def_window_proc(FROM, t, message, 0, 0) },
            || hand::def_window_proc(t, message, 0, 0),
        );
        bench.worked("DefWindowProcA", "answer", answer, value);
    }
}

/// `w` of the next `WM_QUIT`, by the peek given; none if it never comes.
fn quit_code(mut peek: impl FnMut(&mut Message) -> u32) -> Option<u32> {
    let mut record = [0; 7];
    for _ in 0..DRAIN_LIMIT {
        if peek(&mut record) != 0 && record[1] == WM_QUIT {
            return Some(record[2]);
        }
    }
    None
}

fn quit(bench: &mut Bench) {
    bench.alone("PostQuitMessage", &Want::a(&[7]).no_v(), || {
        thin::post_quit_message(FROM, 7);
        0
    });
    let code = quit_code(|record| {
        // SAFETY: a record of the size it writes.
        unsafe { thin::peek_message(FROM, spot(record), 0, 0, 0, 1) }
    });
    take_lines();
    bench.agree("PostQuitMessage", "the code", code, Some(7));
    hand::post_quit_message(8);
    let code = quit_code(|record| hand::peek_message(record, 0, 0, 0, 1));
    bench.worked("PostQuitMessage", "the code", code, Some(8));
}

/// Another thread posts after a pause, so the wait has something to end it and a length to read.
fn woken_later(t: u32) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        thread::sleep(WAKE_AFTER);
        hand::post_message(t, WOKEN, 0, 0);
    })
}

fn wait(bench: &mut Bench, t: u32) {
    hand::pump();
    let waker = woken_later(t);
    let started = Instant::now();
    // SAFETY: this program holds nothing a procedure touches.
    let by_thin = bench.alone("WaitMessage", &Want::default(), || unsafe {
        thin::wait_message(FROM)
    });
    let thin_took = started.elapsed();
    waker.join().expect("the waker runs to its end");
    hand::pump();
    let waker = woken_later(t);
    let started = Instant::now();
    let by_hand = hand::wait_message();
    let hand_took = started.elapsed();
    waker.join().expect("the waker runs to its end");
    hand::pump();
    println!("row WU2 WaitMessage thin {thin_took:?} hand {hand_took:?}");
    bench.agree("WaitMessage", "answer", by_thin, by_hand);
    bench.worked("WaitMessage", "answer", by_hand, 1);
    if hand_took >= WAITED_AT_LEAST && thin_took < WAITED_AT_LEAST {
        bench.thin_red(
            "WaitMessage",
            &format!("returned after {thin_took:?}, the hand's {hand_took:?}"),
        );
    }
}

/// Waits for a `WM_TIMER`, by the thin call; true once one is in `record`.
fn timer_arrives(window: u32, record: &mut Message) -> bool {
    for _ in 0..TIMER_TURNS {
        // SAFETY: a record of the size it writes.
        let got = unsafe { thin::peek_message(FROM, spot(record), window, WM_TIMER, WM_TIMER, 1) };
        take_lines();
        if got != 0 {
            return true;
        }
        thread::sleep(TIMER_TURN);
    }
    false
}

/// No timer came: the hand-declared peek says whose fault that can be.
fn no_timer(bench: &mut Bench, window: u32, which: &str) {
    let mut record = [0; 7];
    if hand::peek_message(&mut record, window, WM_TIMER, WM_TIMER, 0) == 0 {
        bench.control_red("PeekMessageA", &format!("no {which}'s 0x113 either way"));
    } else {
        bench.thin_red(
            "PeekMessageA",
            &format!("the hand saw a {which}'s 0x113, thin did not"),
        );
    }
}

fn window_timer(bench: &mut Bench, t: u32) {
    hand::set_timer(t, 1, 20, 0);
    if bench.plant == Plant::NoTimer {
        hand::kill_timer(t, 1);
    }
    let mut record = [0; 7];
    if timer_arrives(t, &mut record) {
        bench.agree(
            "PeekMessageA",
            "a window timer's message",
            (record[0], record[1], record[2]),
            (t, WM_TIMER, 1),
        );
        let before = WINDOW_TIMER_RUNS.load(Relaxed);
        // SAFETY: a whole record; the procedure is this program's own.
        bench.alone("DispatchMessageA", &Want::a(&record), || unsafe {
            thin::dispatch_message(FROM, address(&record))
        });
        bench.agree(
            "DispatchMessageA",
            "the window's timer ran",
            WINDOW_TIMER_RUNS.load(Relaxed) - before,
            1,
        );
    } else {
        no_timer(bench, t, "window timer");
    }
    hand::kill_timer(t, 1);
}

/// A windowless timer's `l` is Wine's own number, not the address handed in: only the real
/// `DispatchMessageA` can run it, which is the reason `dispatch_message`'s note gives.
fn windowless_timer(bench: &mut Bench) {
    let at = code(timer_procedure as *const ());
    let id = hand::set_timer(0, 0, 20, at);
    if bench.plant == Plant::NoTimer {
        hand::kill_timer(0, id);
    }
    let mut record = [0; 7];
    if timer_arrives(0, &mut record) {
        println!(
            "row WU2 timer window={:x} w={:x} l={:x} procedure={at:x}",
            record[0], record[2], record[3]
        );
        bench.agree(
            "PeekMessageA",
            "a windowless timer's message",
            (record[0], record[1], record[2]),
            (0, WM_TIMER, id),
        );
        let before = TIMER_PROCEDURE_RUNS.load(Relaxed);
        // SAFETY: a whole record; the procedure is this program's own.
        bench.alone("DispatchMessageA", &Want::a(&record), || unsafe {
            thin::dispatch_message(FROM, address(&record))
        });
        bench.agree(
            "DispatchMessageA",
            "the timer ran",
            TIMER_PROCEDURE_RUNS.load(Relaxed) - before,
            1,
        );
        let before = TIMER_PROCEDURE_RUNS.load(Relaxed);
        if timer_arrives(0, &mut record) {
            hand::dispatch_message(&record);
            bench.worked(
                "DispatchMessageA",
                "the timer ran",
                TIMER_PROCEDURE_RUNS.load(Relaxed) - before,
                1,
            );
        }
    } else {
        no_timer(bench, 0, "windowless timer");
    }
    hand::kill_timer(0, id);
}

/// Runs the check on one hidden window, the queue pumped clear between parts.
pub fn run(plant: Plant) -> ExitCode {
    let mut bench = Bench::new("WU2", "window", SENTENCE, plant, &NAMES);
    let instance = hand::module();
    let class = [
        0,
        code(procedure as *const ()),
        0,
        0,
        instance,
        0,
        0,
        0,
        0,
        text(c"uc_loop"),
    ];
    hand::register_class(&class);
    let t = hand::create_window(
        c"uc_loop",
        c"uc t",
        HIDDEN_POPUP,
        [40, 40, 320, 200],
        instance,
    );
    hand::pump();
    posted_drains(&mut bench, t);
    translate(&mut bench, t);
    hand::pump();
    doors(&mut bench, t);
    hand::pump();
    quit(&mut bench);
    wait(&mut bench, t);
    window_timer(&mut bench, t);
    windowless_timer(&mut bench);
    hand::destroy_window(t);
    bench.finish()
}
