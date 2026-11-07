// SPDX-License-Identifier: MIT
//! `wu1`: `window::user32`'s 15 window calls, each beside its hand-declared twin. A call that
//! only reads asks of one window both ways; a call that changes something asks of a twin class
//! or a twin window, so neither side's answer is the other's leftover.
use std::ffi::CStr;
use std::process::ExitCode;

use window::user32 as thin;

use crate::bench::{
    BAD_WINDOW, Bench, FROM, LOGGER_ERROR, MARKED_ERROR, NO_WINDOW, Plant, Same, Want, address,
    code, spot, text, words,
};
use crate::hand::{self, Class, Rect};

const NAMES: [&str; 15] = [
    "RegisterClassA",
    "CreateWindowExA",
    "DestroyWindow",
    "FindWindowA",
    "LoadIconA",
    "ShowWindow",
    "SetWindowPos",
    "AdjustWindowRect",
    "GetWindowRect",
    "GetWindowLongA",
    "SetWindowLongA",
    "ClientToScreen",
    "ScreenToClient",
    "SetFocus",
    "GetSystemMetrics",
];
const SENTENCE: &str = "The real USER32 answered one thing and `window::user32` said another. The thin call, not Wine.";
/// `WS_POPUP`: no frame, so the client area starts at the window's corner.
const HIDDEN_POPUP: u32 = 0x8000_0000;
/// `WS_OVERLAPPEDWINDOW`: a frame, so `AdjustWindowRect` has something to add.
const OVERLAPPED: u32 = 0x00cf_0000;
const MENU_NUMBER: u32 = 7;
/// `ERROR_CLASS_ALREADY_EXISTS`.
const CLASS_EXISTS: u32 = 1410;
/// What Wine leaves when `DestroyWindow` is given a handle no longer good: measured on both
/// sides, where the other calls here leave `BAD_WINDOW`.
const DESTROY_REFUSED: u32 = 5;
/// `SW_SHOW`.
const SHOW: u32 = 5;
/// `SWP_NOZORDER | SWP_NOACTIVATE`.
const MOVE_ONLY: u32 = 0x14;
const PLACED: Rect = [60, 70, 380, 270];
/// `GWL_USERDATA`.
const USER_DATA: i32 = -21;

/// One window made by the thin call, its twin by hand.
#[derive(Clone, Copy)]
struct Twin {
    thin: u32,
    hand: u32,
}

/// Every message goes on by hand, so the only lines written are the calls' under test.
unsafe extern "system" fn procedure(window: u32, message: u32, w: u32, l: u32) -> u32 {
    hand::def_window_proc(window, message, w, l)
}

fn class_record(instance: u32, name: &CStr) -> Class {
    [
        3,
        code(procedure as *const ()),
        0,
        0,
        instance,
        0,
        0,
        0,
        MENU_NUMBER,
        text(name),
    ]
}

/// The process's first class, by hand, so the pair is not asked about it: Wine's first
/// `RegisterClassA` leaves a last error of 0, whoever makes it. The line shows it each run.
fn first_class(instance: u32) {
    let class = class_record(instance, c"uc_first");
    hand::set_last_error(MARKED_ERROR);
    let atom = hand::register_class(&class);
    println!(
        "row WU1 RegisterClassA the process's first atom={atom:x} error={:x}",
        hand::last_error()
    );
}

/// A class each way, read back, then each refused a second time with the same error.
fn classes(bench: &mut Bench, instance: u32) -> u32 {
    first_class(instance);
    let thin_class = class_record(instance, c"uc_thin");
    let hand_class = class_record(instance, c"uc_hand");
    let want = Want::a(&thin_class[..8]).s("uc_thin").with("#7");
    let [atom, _] = bench.beside(
        "RegisterClassA",
        Same::BothNonzero,
        &want,
        // SAFETY: the record and its texts outlive the call.
        || unsafe { thin::register_class(FROM, address(&thin_class)) },
        || hand::register_class(&hand_class),
    );
    let (found, read) = hand::class_info(instance, c"uc_thin");
    bench.agree(
        "RegisterClassA",
        "the class it made",
        (found != 0, read[0], read[1]),
        (true, thin_class[0], thin_class[1]),
    );
    bench.beside(
        "RegisterClassA",
        Same::Answer,
        &want,
        // SAFETY: as above.
        || unsafe { thin::register_class(FROM, address(&thin_class)) },
        || hand::register_class(&hand_class),
    );
    bench.worked(
        "RegisterClassA",
        "refused, last error",
        bench.errors[1],
        CLASS_EXISTS,
    );
    atom
}

fn windows(bench: &mut Bench, instance: u32) -> Twin {
    let want = Want::a(&[0, HIDDEN_POPUP, 40, 40, 320, 200, 0, 0, instance, 0])
        .s("uc_thin")
        .with("uc t");
    let [made_thin, made_hand] = bench.beside(
        "CreateWindowExA",
        Same::BothNonzero,
        &want,
        // SAFETY: both texts outlive the call; the procedure is this program's own.
        || unsafe {
            thin::create_window(
                FROM,
                0,
                text(c"uc_thin"),
                text(c"uc t"),
                HIDDEN_POPUP,
                40,
                40,
                320,
                200,
                0,
                0,
                instance,
                0,
            )
        },
        || {
            hand::create_window(
                c"uc_hand",
                c"uc h",
                HIDDEN_POPUP,
                [40, 40, 320, 200],
                instance,
            )
        },
    );
    bench.worked(
        "CreateWindowExA",
        "a window each",
        [hand::is_window(made_thin), hand::is_window(made_hand)],
        [1, 1],
    );
    Twin {
        thin: made_thin,
        hand: made_hand,
    }
}

/// By class, by title, by atom, and a name nothing holds: each line shows its own form of name.
fn find(bench: &mut Bench, window: u32, atom: u32) {
    let by_atom = format!("#{atom:x}");
    let asks = [
        (text(c"uc_thin"), 0, "uc_thin", "", window),
        (0, text(c"uc t"), "", "uc t", window),
        (atom, 0, by_atom.as_str(), "", window),
        (text(c"uc_none"), 0, "uc_none", "", 0),
    ];
    for (class, title, shown_class, shown_title, value) in asks {
        let want = Want::default().s(shown_class).with(shown_title);
        let [_, answer] = bench.beside(
            "FindWindowA",
            Same::Answer,
            &want,
            // SAFETY: each is 0, a number below `0x10000` or a text that lives on.
            || unsafe { thin::find_window(FROM, class, title) },
            || hand::find_window(class, title),
        );
        bench.worked("FindWindowA", "the window", answer, value);
    }
}

fn icons(bench: &mut Bench, instance: u32) {
    let [_, icon] = bench.beside(
        "LoadIconA",
        Same::Answer,
        &Want::a(&[0]).s("#7f00"),
        // SAFETY: a resource id, `IDI_APPLICATION`.
        || unsafe { thin::load_icon(FROM, 0, 0x7f00) },
        || hand::load_icon(0, 0x7f00),
    );
    bench.worked("LoadIconA", "an icon", icon != 0, true);
    let [_, none] = bench.beside(
        "LoadIconA",
        Same::Answer,
        &Want::a(&[instance]).s("no such"),
        // SAFETY: a text that lives on.
        || unsafe { thin::load_icon(FROM, instance, text(c"no such")) },
        || hand::load_icon(instance, text(c"no such")),
    );
    bench.worked("LoadIconA", "a name not here", none, 0);
}

fn show_and_place(bench: &mut Bench, twin: Twin) {
    let Twin { thin: t, hand: h } = twin;
    for _ in 0..2 {
        bench.beside(
            "ShowWindow",
            Same::Answer,
            &Want::a(&[t, SHOW]),
            // SAFETY: the window's procedure is this program's own and holds nothing.
            || unsafe { thin::show_window(FROM, t, SHOW) },
            || hand::show_window(h, 5),
        );
        bench.agree(
            "ShowWindow",
            "visible",
            hand::is_visible(t),
            hand::is_visible(h),
        );
    }
    bench.worked("ShowWindow", "visible", hand::is_visible(h), 1);
    bench.beside(
        "SetWindowPos",
        Same::Answer,
        &Want::a(&[t, 0, 60, 70, 320, 200, MOVE_ONLY]),
        // SAFETY: as above.
        || unsafe { thin::set_window_pos(FROM, t, 0, 60, 70, 320, 200, MOVE_ONLY) },
        || hand::set_window_pos(h, [60, 70, 320, 200], MOVE_ONLY),
    );
    let (mut read_thin, mut read_hand) = ([0; 4], [0; 4]);
    hand::get_window_rect(t, &mut read_thin);
    hand::get_window_rect(h, &mut read_hand);
    bench.agree("SetWindowPos", "where it went", read_thin, read_hand);
    bench.worked("SetWindowPos", "where it went", read_hand, PLACED);
}

fn window_rect(bench: &mut Bench, t: u32) {
    let (mut by_thin, mut by_hand) = ([0; 4], [0; 4]);
    bench.beside(
        "GetWindowRect",
        Same::Answer,
        &Want::a(&[t]).o(&words(PLACED)),
        // SAFETY: a record of the size it writes.
        || unsafe { thin::get_window_rect(FROM, t, spot(&mut by_thin)) },
        || hand::get_window_rect(t, &mut by_hand),
    );
    bench.agree("GetWindowRect", "rectangle", by_thin, by_hand);
    bench.worked("GetWindowRect", "rectangle", by_hand, PLACED);
    let (mut by_thin, mut by_hand) = ([0; 4], [0; 4]);
    bench.beside(
        "GetWindowRect",
        Same::Answer,
        &Want::a(&[NO_WINDOW]).o(&[]),
        // SAFETY: as above.
        || unsafe { thin::get_window_rect(FROM, NO_WINDOW, spot(&mut by_thin)) },
        || hand::get_window_rect(NO_WINDOW, &mut by_hand),
    );
    bench.agree(
        "GetWindowRect",
        "a bad handle's rectangle",
        by_thin,
        by_hand,
    );
    bench.worked(
        "GetWindowRect",
        "a bad handle's error",
        bench.errors[1],
        BAD_WINDOW,
    );
}

fn adjust(bench: &mut Bench) {
    let start: Rect = [0, 0, 640, 480];
    for (style, framed) in [(OVERLAPPED, true), (HIDDEN_POPUP, false)] {
        let mut reference = start;
        hand::adjust_window_rect(&mut reference, style);
        let (mut by_thin, mut by_hand) = (start, start);
        bench.beside(
            "AdjustWindowRect",
            Same::Answer,
            &Want::a(&[0, 0, 640, 480, style, 0]).o(&words(reference)),
            // SAFETY: a record of the size it writes.
            || unsafe { thin::adjust_window_rect(FROM, spot(&mut by_thin), style, 0) },
            || hand::adjust_window_rect(&mut by_hand, style),
        );
        bench.agree("AdjustWindowRect", "rectangle", by_thin, by_hand);
        bench.worked(
            "AdjustWindowRect",
            "a frame added",
            by_hand != start,
            framed,
        );
    }
}

fn longs(bench: &mut Bench, twin: Twin) {
    let Twin { thin: t, hand: h } = twin;
    for index in [-16_i32, -20, -4, -6] {
        let number = index.cast_unsigned();
        bench.beside(
            "GetWindowLongA",
            Same::Answer,
            &Want::a(&[t, number]),
            || thin::get_window_long(FROM, t, number),
            || hand::get_window_long(t, index),
        );
    }
    let style = hand::get_window_long(t, -16);
    bench.worked(
        "GetWindowLongA",
        "the style",
        style & HIDDEN_POPUP,
        HIDDEN_POPUP,
    );
    let bad = (-16_i32).cast_unsigned();
    bench.beside(
        "GetWindowLongA",
        Same::Answer,
        &Want::a(&[NO_WINDOW, bad]),
        || thin::get_window_long(FROM, NO_WINDOW, bad),
        || hand::get_window_long(NO_WINDOW, -16),
    );
    bench.worked(
        "GetWindowLongA",
        "a bad handle's error",
        bench.errors[1],
        BAD_WINDOW,
    );
    let index = USER_DATA.cast_unsigned();
    for (value, before) in [(0x55_i32, 0_u32), (0x66, 0x55)] {
        let number = value.cast_unsigned();
        let [_, prior] = bench.beside(
            "SetWindowLongA",
            Same::Answer,
            &Want::a(&[t, index, number]),
            // SAFETY: this index takes a plain number.
            || unsafe { thin::set_window_long(FROM, t, index, number) },
            || hand::set_window_long(h, USER_DATA, value),
        );
        bench.worked("SetWindowLongA", "what stood before", prior, before);
        bench.agree(
            "SetWindowLongA",
            "what it set",
            hand::get_window_long(t, USER_DATA),
            hand::get_window_long(h, USER_DATA),
        );
    }
}

/// Unequal coordinates throughout, so swapped ones cannot pass.
fn points(bench: &mut Bench, t: u32) {
    let plant = bench.plant;
    let (mut by_thin, mut by_hand) = ([10, 20], [10, 20]);
    bench.beside(
        "ClientToScreen",
        Same::Answer,
        &Want::a(&[t, 10, 20]).o(&[70, 90]),
        // SAFETY: a record of the size it writes.
        || unsafe { thin::client_to_screen(FROM, t, spot(&mut by_thin)) },
        || hand::client_to_screen(t, &mut by_hand),
    );
    bench.agree("ClientToScreen", "point", by_thin, by_hand);
    bench.worked("ClientToScreen", "point", by_hand, [70, 90]);
    let (mut by_thin, mut by_hand) = ([123, 77], [123, 77]);
    bench.beside(
        "ScreenToClient",
        Same::Answer,
        &Want::a(&[t, 123, 77]).o(&[63, 7]),
        || {
            // SAFETY: as above.
            let answer = unsafe { thin::screen_to_client(FROM, t, spot(&mut by_thin)) };
            if plant == Plant::SwapXy {
                by_thin.swap(0, 1);
            }
            answer
        },
        || hand::screen_to_client(t, &mut by_hand),
    );
    bench.agree("ScreenToClient", "point", by_thin, by_hand);
    bench.worked("ScreenToClient", "point", by_hand, [63, 7]);
    let (mut by_thin, mut by_hand) = ([123, 77], [123, 77]);
    bench.beside(
        "ScreenToClient",
        Same::Answer,
        &Want::a(&[NO_WINDOW, 123, 77]).o(&[]),
        // SAFETY: as above.
        || unsafe { thin::screen_to_client(FROM, NO_WINDOW, spot(&mut by_thin)) },
        || hand::screen_to_client(NO_WINDOW, &mut by_hand),
    );
    bench.agree("ScreenToClient", "a bad handle's point", by_thin, by_hand);
    bench.worked(
        "ScreenToClient",
        "a bad handle's error",
        bench.errors[1],
        BAD_WINDOW,
    );
}

/// What a `SetFocus` answer named: the window asked for, the one that held it, or neither.
fn named(answer: u32, asked: u32, held: u32) -> &'static str {
    if answer == asked {
        "asked"
    } else if answer == held {
        "held"
    } else {
        "neither"
    }
}

/// A chain, not a twin: thin and hand take focus in turn. Wine answers with the window asked
/// for, on both sides, where Win32's page says the one that held it (measured). So the pair
/// is compared by what each answer named, and by who holds the focus afterwards.
fn focus(bench: &mut Bench, twin: Twin) {
    let Twin { thin: t, hand: h } = twin;
    let before = hand::get_focus();
    let want = Want::a(&[t]);
    // SAFETY: the procedures it runs are this program's own and hold nothing.
    let answer = bench.alone("SetFocus", &want, || unsafe { thin::set_focus(FROM, t) });
    bench.agree("SetFocus", "who holds it", hand::get_focus(), t);
    let by_hand = named(hand::set_focus(h), h, t);
    bench.worked("SetFocus", "who holds it", hand::get_focus(), h);
    bench.agree(
        "SetFocus",
        "what it named",
        named(answer, t, before),
        by_hand,
    );
    // SAFETY: as above.
    let answer = bench.alone("SetFocus", &want, || unsafe { thin::set_focus(FROM, t) });
    bench.agree("SetFocus", "what it named", named(answer, t, h), by_hand);
    bench.agree("SetFocus", "who holds it", hand::get_focus(), t);
}

fn metrics(bench: &mut Bench) {
    let plant = bench.plant;
    for index in [0_i32, 1, 4, 5, 6, 11, 12, 13, 14, 43, 80, 999] {
        let number = index.cast_unsigned();
        let [_, answer] = bench.beside(
            "GetSystemMetrics",
            Same::Answer,
            &Want::a(&[number]),
            || {
                if plant == Plant::NoLine && index == 0 {
                    hand::get_system_metrics(index)
                } else {
                    thin::get_system_metrics(FROM, number)
                }
            },
            || hand::get_system_metrics(index),
        );
        match index {
            0 => bench.worked("GetSystemMetrics", "the screen's width", answer, 1024),
            1 => bench.worked("GetSystemMetrics", "the screen's height", answer, 768),
            _ => {}
        }
    }
}

fn destroy(bench: &mut Bench, twin: Twin) {
    let plant = bench.plant;
    let Twin { thin: t, hand: h } = twin;
    let want = Want::a(&[t]);
    let [_, answer] = bench.beside(
        "DestroyWindow",
        Same::Answer,
        &want,
        // SAFETY: the window's procedure is this program's own and holds nothing.
        || unsafe { thin::destroy_window(FROM, t) },
        || hand::destroy_window(h),
    );
    bench.worked("DestroyWindow", "answer", answer, 1);
    bench.agree(
        "DestroyWindow",
        "gone",
        hand::is_window(t),
        hand::is_window(h),
    );
    bench.worked("DestroyWindow", "gone", hand::is_window(h), 0);
    bench.beside(
        "DestroyWindow",
        Same::Answer,
        &want,
        || {
            // SAFETY: a handle no longer good is only a 0 answer.
            let answer = unsafe { thin::destroy_window(FROM, t) };
            if plant == Plant::NoKept {
                hand::set_last_error(LOGGER_ERROR);
            }
            answer
        },
        || hand::destroy_window(h),
    );
    bench.worked(
        "DestroyWindow",
        "a bad handle's error",
        bench.errors[1],
        DESTROY_REFUSED,
    );
}

/// Runs the check, in an order where each call's window stands as its worked value says.
pub fn run(plant: Plant) -> ExitCode {
    let mut bench = Bench::new("WU1", "window", SENTENCE, plant, &NAMES);
    let instance = hand::module();
    let atom = classes(&mut bench, instance);
    let twin = windows(&mut bench, instance);
    find(&mut bench, twin.thin, atom);
    icons(&mut bench, instance);
    show_and_place(&mut bench, twin);
    window_rect(&mut bench, twin.thin);
    adjust(&mut bench);
    longs(&mut bench, twin);
    points(&mut bench, twin.thin);
    focus(&mut bench, twin);
    metrics(&mut bench);
    destroy(&mut bench, twin);
    bench.finish()
}
