// SPDX-License-Identifier: MIT
//! What the programs of a tap, a timer and the loop's pace share, on every route: the planted
//! copies, a message's way from the queue to the dispatch, and the calls a second. Each program
//! takes this file by `#[path]`, and of it what its route needs. It judges nothing: a read is a
//! printed line or a number handed back.
//!
//! A planted wrong form stands here and never in the crates: [`TapKeys`] holds a bit `Keys`
//! does not, [`timer_reach`]'s `handed_on` does what `dispatch_message` does not, and [`Slowed`]
//! waits in its pump.
use core::cell::Cell;
use core::hint::black_box;
use core::sync::atomic::{AtomicU64, Ordering};
use std::io::{self, Write};
use std::path::Path;
use std::time::Instant;

use log::{LevelFilter, Log, Metadata, Record};
use oxide_logline::Roots;
use window::desk::Desk;
use window::front::Front as WindowFront;
use window::table::{Calls as WindowCalls, Row as WindowRow};
use window::{Event, Message, Rect, Screen};
use window_input::Input;
use window_input::front::Front;
use window_input::table::{Calls, Row};

pub const KEY_A: u8 = 0x41;
const WM_TIMER: u32 = 0x113;
const PM_REMOVE: u32 = 1;
/// A number standing for a timer's procedure, which nothing here calls; its id is `TIMER_ID`.
const TIMER_PROCEDURE: u32 = 0x2000;
const TIMER_ID: u32 = 7;
const ASYNC_DOWN: u32 = 0x8000;
const ASYNC_PRESSED: u32 = 1;
/// A row's first count, doubled until its loop has run `LEAST_SECONDS`.
const FIRST_COUNT: u32 = 1024;
const LEAST_SECONDS: f64 = 0.2;
/// What [`Fronts::run`]'s `call` numbers, as the rows name them.
const CALL_NAMES: [&str; 3] = ["GetAsyncKeyState", "PeekMessageA", "GetCursorPos"];

static LINES: AtomicU64 = AtomicU64::new(0);

static INPUT_CALLS: Calls = Calls {
    get_cursor_pos: "USER32!GetCursorPos",
    set_cursor_pos: "USER32!SetCursorPos",
    show_cursor: "USER32!ShowCursor",
    load_cursor: "USER32!LoadCursorA",
    clip_cursor: "USER32!ClipCursor",
    get_async_key_state: "USER32!GetAsyncKeyState",
    get_key_state: "USER32!GetKeyState",
    map_virtual_key: "USER32!MapVirtualKeyA",
};
pub static INPUT_ROW: Row = Row {
    file: "loop_checks",
    calls: &INPUT_CALLS,
};
static WINDOW_CALLS: WindowCalls = WindowCalls {
    register_class: "USER32!RegisterClassA",
    create_window: "USER32!CreateWindowExA",
    destroy_window: "USER32!DestroyWindow",
    find_window: "USER32!FindWindowA",
    load_icon: "USER32!LoadIconA",
    show_window: "USER32!ShowWindow",
    set_window_pos: "USER32!SetWindowPos",
    adjust_window_rect: "USER32!AdjustWindowRect",
    get_window_rect: "USER32!GetWindowRect",
    get_window_long: "USER32!GetWindowLongA",
    set_window_long: "USER32!SetWindowLongA",
    client_to_screen: "USER32!ClientToScreen",
    screen_to_client: "USER32!ScreenToClient",
    set_focus: "USER32!SetFocus",
    get_system_metrics: "USER32!GetSystemMetrics",
    def_window_proc: "USER32!DefWindowProcA",
    peek_message: "USER32!PeekMessageA",
    wait_message: "USER32!WaitMessage",
    translate_message: "USER32!TranslateMessage",
    dispatch_message: "USER32!DispatchMessageA",
    send_message: "USER32!SendMessageA",
    post_message: "USER32!PostMessageA",
    post_quit_message: "USER32!PostQuitMessage",
    get_message_time: "USER32!GetMessageTime",
};
static WINDOW_ROW: WindowRow = WindowRow {
    file: "loop_checks",
    calls: &WINDOW_CALLS,
    metrics: &[],
};

/// A copy of what `Keys` holds of the keys down, with the bit it does not: a key pressed since
/// its last read.
pub struct TapKeys {
    down: [bool; 256],
    pressed: [bool; 256],
}

impl TapKeys {
    pub const fn new() -> Self {
        Self {
            down: [false; 256],
            pressed: [false; 256],
        }
    }

    pub fn apply(&mut self, event: Event) {
        match event {
            Event::Key { key, down, .. } | Event::Button { key, down, .. } => {
                let at = usize::from(key);
                if down && !self.down[at] {
                    self.pressed[at] = true;
                }
                self.down[at] = down;
            }
            Event::Active { active: false, .. } => self.down = [false; 256],
            _ => {}
        }
    }

    /// `GetAsyncKeyState` with the bit: its top bit while down, its low bit for a press since
    /// the last read, which this read clears.
    pub fn answer(&mut self, key: u8) -> u32 {
        let at = usize::from(key);
        let down = if self.down[at] { ASYNC_DOWN } else { 0 };
        let pressed = if self.pressed[at] { ASYNC_PRESSED } else { 0 };
        self.pressed[at] = false;
        down | pressed
    }
}

/// One read of a tap's check.
pub fn print_read(stage: &str, answer: u32) {
    println!(r#"{{"row":"read","stage":{stage:?},"answer":{answer}}}"#);
}

/// What a timer's message came to: whether the queue handed it back, whether a procedure was
/// called with the message's own `l`, and the dispatch's answer.
#[derive(Clone, Copy)]
pub struct Timer {
    pub peeked: bool,
    pub reached: bool,
    pub answer: u32,
}

/// A copy of `DispatchMessageA` that hands a timer's message with no window to its `l`.
fn dispatch_copy<S: Screen>(
    front: &WindowFront<'_, S>,
    message: &Message,
    call: impl FnOnce(u32, &Message) -> u32,
) -> u32 {
    if message.message == WM_TIMER && message.window == 0 {
        call(message.l, message)
    } else {
        front.dispatch_message(message, call)
    }
}

/// A timer's message queued with no window, peeked back and dispatched, by the front or, when
/// `handed_on`, by the planted copy.
pub fn timer_reach<S: Screen>(desk: &Desk<S>, handed_on: bool) -> Timer {
    let front = WindowFront::new(&WINDOW_ROW, desk);
    let queued = Message {
        window: 0,
        message: WM_TIMER,
        w: TIMER_ID,
        l: TIMER_PROCEDURE,
        ..Message::default()
    };
    desk.state().queue.extend([queued]);
    let Some(message) = front.peek_message(0, WM_TIMER, WM_TIMER, PM_REMOVE) else {
        return Timer {
            peeked: false,
            reached: false,
            answer: 0,
        };
    };
    let mut called = None;
    let call = |procedure: u32, _: &Message| {
        called = Some(procedure);
        1
    };
    let answer = if handed_on {
        dispatch_copy(&front, &message, call)
    } else {
        front.dispatch_message(&message, call)
    };
    Timer {
        peeked: message == queued,
        reached: called == Some(TIMER_PROCEDURE),
        answer,
    }
}

/// One read of a timer's check.
pub fn print_timer(timer: Timer) {
    println!(
        r#"{{"row":"timer","peeked":{},"reached":{},"answer":{}}}"#,
        timer.peeked, timer.reached, timer.answer
    );
}

/// A back whose pump waits first, once planted: the pace check's wrong answer.
pub struct Slowed<S> {
    back: S,
    wait: Cell<Option<fn()>>,
}

impl<S> Slowed<S> {
    pub const fn new(back: S) -> Self {
        Self {
            back,
            wait: Cell::new(None),
        }
    }

    pub fn plant(&self, wait: fn()) {
        self.wait.set(Some(wait));
    }
}

impl<S: Screen> Screen for Slowed<S> {
    fn size(&self) -> (i32, i32) {
        self.back.size()
    }

    fn place(&self, window: u32, rect: Rect, shown: bool) {
        self.back.place(window, rect, shown);
    }

    fn gone(&self, window: u32) {
        self.back.gone(window);
    }

    fn pump(&self, into: &mut dyn FnMut(Event)) {
        if let Some(wait) = self.wait.get() {
            wait();
        }
        self.back.pump(into);
    }

    fn now(&self) -> u32 {
        self.back.now()
    }

    fn quiet(&self, line: impl FnOnce()) {
        self.back.quiet(line);
    }

    fn character(&self, key: u32) -> u32 {
        self.back.character(key)
    }

    fn scan(&self, key: u32) -> u32 {
        self.back.scan(key)
    }

    fn point(&self) -> (i32, i32) {
        self.back.point()
    }
}

/// A desk's 2 fronts, as a program's loop calls them.
pub struct Fronts<'d, S> {
    desk: &'d Desk<Input<S>>,
    input: Front<'d, S>,
    window: WindowFront<'d, Input<S>>,
}

impl<'d, S: Screen> Fronts<'d, S> {
    pub const fn new(desk: &'d Desk<Input<S>>) -> Self {
        Self {
            desk,
            input: Front::new(&INPUT_ROW, desk),
            window: WindowFront::new(&WINDOW_ROW, desk),
        }
    }

    pub const fn desk(&self) -> &'d Desk<Input<S>> {
        self.desk
    }

    /// `count` calls of the one `call` numbers, in [`CALL_NAMES`]' order.
    pub fn run(&self, call: usize, count: u32) {
        for _ in 0..count {
            match call {
                0 => {
                    black_box(self.input.get_async_key_state(u32::from(KEY_A)));
                }
                1 => {
                    black_box(self.window.peek_message(0, 0, 0, PM_REMOVE));
                }
                _ => {
                    black_box(self.input.get_cursor_pos());
                }
            }
        }
    }
}

/// The log lines rendered since the program began.
pub fn lines() -> u64 {
    LINES.load(Ordering::Relaxed)
}

/// A sink that counts each line and keeps none: a disc's cost is not in a rate.
struct Counted;

impl Write for Counted {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let newlines = bytes.split(|byte| *byte == b'\n').count() - 1;
        LINES.fetch_add(u64::try_from(newlines).unwrap_or(0), Ordering::Relaxed);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// The workspace's logger, at every line, on a route with a clock.
pub fn logger_on() {
    let here = Path::new(".");
    let roots = Roots {
        job: here,
        base: here,
        cwd: here,
    };
    let _ = oxide_logline::install("loop_checks", &roots, Counted, LevelFilter::Debug);
}

/// A logger for a wasm build, whose `SystemTime` refuses at run time: each line's body rendered
/// by the workspace's own code, with no clock and no writer.
struct Bodies;

impl Log for Bodies {
    fn enabled(&self, _: &Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &Record<'_>) {
        black_box(oxide_logline::body(record));
        LINES.fetch_add(1, Ordering::Relaxed);
    }

    fn flush(&self) {}
}

static BODIES: Bodies = Bodies;

/// [`Bodies`] on, at every line.
pub fn bodies_on() {
    if log::set_logger(&BODIES).is_ok() {
        log::set_max_level(LevelFilter::Debug);
    }
}

/// A wait a thread with no sleep can make: `count` turns of nothing.
pub fn spin(count: u32) {
    for turn in 0..count {
        black_box(turn);
    }
}

/// One row for each call: `run` makes the calls, doubled until a loop has run long enough for
/// the clock's grain not to show.
pub fn rates(logger: &str, run: impl Fn(usize, u32)) {
    for (call, name) in CALL_NAMES.iter().enumerate() {
        let mut count = FIRST_COUNT;
        loop {
            let before = lines();
            let started = Instant::now();
            run(call, count);
            let seconds = started.elapsed().as_secs_f64();
            if seconds >= LEAST_SECONDS {
                let said = lines() - before;
                let per_second = f64::from(count) / seconds;
                println!(
                    r#"{{"row":"rate","call":{name:?},"logger":{logger:?},"count":{count},"seconds":{seconds},"per_second":{per_second},"lines":{said}}}"#
                );
                break;
            }
            count = count.saturating_mul(2);
        }
    }
}

/// Every call's rows on `desk`, the logger off and then on.
pub fn pace<S: Screen>(desk: &Desk<Input<S>>) {
    let fronts = Fronts::new(desk);
    rates("off", |call, count| fronts.run(call, count));
    logger_on();
    rates("on", |call, count| fronts.run(call, count));
}
