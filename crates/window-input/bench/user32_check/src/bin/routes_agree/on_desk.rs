// SPDX-License-Identifier: MIT
//! The route that answers by itself: `window::front` and `window_input::front` over `Desk`, over
//! `Page`, with no browser. An event is what a page's script would hand in. Its row's metrics are
//! the only numbers it is given, and a plant puts a wrong one there.
use std::cell::Cell;
use std::ffi::CStr;

use log::LevelFilter;
use window::desk::Desk;
use window::front::{Create, Front};
use window::page::Page;
use window::table::{Calls, Row};
use window::{Event, Rect};
use window_input::Input;
use window_input::front::{Front as InputFront, Name};
use window_input::table::{Calls as InputCalls, Row as InputRow};

use crate::hand::Message;
use crate::script::{self, CLASS_STYLE, Press, Route, WINDOW_EXTRA, WM_TIMER};

/// The size the driver gives the private screen.
const SCREEN: (i32, i32) = (1024, 768);
/// A time for the page's clock, so it is not 0. Nothing compares it.
const CLOCK_MS: u32 = 1000;
const KEY_A: u8 = 0x41;
/// The `w` and `l` the real route's timer message carries, as its first log showed. This route
/// posts them itself, so their agreeing reads neither route.
const TIMER_ID: u32 = 0x7fff;
const TIMER_L: u32 = 0xffff_0011;

static CALLS: Calls = Calls {
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
static INPUT_CALLS: InputCalls = InputCalls {
    get_cursor_pos: "USER32!GetCursorPos",
    set_cursor_pos: "USER32!SetCursorPos",
    show_cursor: "USER32!ShowCursor",
    load_cursor: "USER32!LoadCursorA",
    clip_cursor: "USER32!ClipCursor",
    get_async_key_state: "USER32!GetAsyncKeyState",
    get_key_state: "USER32!GetKeyState",
    map_virtual_key: "USER32!MapVirtualKeyA",
};
static ROW: Row = Row {
    file: "routes_agree",
    calls: &CALLS,
    metrics: &[(11, 32), (12, 32)],
};
static WRONG_METRIC_ROW: Row = Row {
    file: "routes_agree",
    calls: &CALLS,
    metrics: &[(11, 33), (12, 32)],
};
static INPUT_ROW: InputRow = InputRow {
    file: "routes_agree",
    calls: &INPUT_CALLS,
};

/// A wrong answer put on this route on purpose, so the scorer's reds can be seen.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Plant {
    None,
    /// `GetSystemMetrics(11)` answers 33 for 32.
    WrongMetric,
    /// The first `ShowWindow` still runs, and writes no line.
    NoLine,
}

impl Plant {
    pub fn named(name: &str) -> Option<Self> {
        Some(match name {
            "none" => Self::None,
            "wrong-metric" => Self::WrongMetric,
            "no-line" => Self::NoLine,
            _ => return None,
        })
    }
}

struct OnDesk<'d> {
    desk: &'d Desk<Input<Page>>,
    windows: Front<'d, Input<Page>>,
    input: InputFront<'d, Page>,
    plant: Plant,
    line_left_out: Cell<bool>,
}

/// The whole script over a fresh `Desk`.
pub fn run(plant: Plant) {
    let desk = Desk::new(Input::new(Page::new(SCREEN.0, SCREEN.1)));
    desk.screen().back().frame(CLOCK_MS);
    let row = if plant == Plant::WrongMetric {
        &WRONG_METRIC_ROW
    } else {
        &ROW
    };
    script::run(&OnDesk {
        desk: &desk,
        windows: Front::new(row, &desk),
        input: InputFront::new(&INPUT_ROW, &desk),
        plant,
        line_left_out: Cell::new(false),
    });
}

/// What runs here writes no line.
fn quietly<T>(call: impl FnOnce() -> T) -> T {
    let level = log::max_level();
    log::set_max_level(LevelFilter::Off);
    let answer = call();
    log::set_max_level(level);
    answer
}

const fn rect(left: i32, top: i32, right: i32, bottom: i32) -> Rect {
    Rect {
        left,
        top,
        right,
        bottom,
    }
}

const fn to_desk(message: &Message) -> window::Message {
    let [window, number, w, l, time, x, y] = *message;
    window::Message {
        window,
        message: number,
        w,
        l,
        time,
        x: x.cast_signed(),
        y: y.cast_signed(),
    }
}

const fn from_desk(message: &window::Message) -> Message {
    [
        message.window,
        message.message,
        message.w,
        message.l,
        message.time,
        message.x.cast_unsigned(),
        message.y.cast_unsigned(),
    ]
}

impl OnDesk<'_> {
    fn hand(&self, window: u32, press: Press) {
        for &down in press.edges() {
            self.desk.screen().back().hand(Event::Key {
                window,
                key: KEY_A,
                down,
            });
        }
    }

    /// `hand::pump`'s twin on this route: every message taken and run, with no line.
    fn pump(&self) {
        quietly(|| {
            while let Some(message) = self.windows.peek_message(0, 0, 0, 1) {
                let _ = self.windows.translate_message(&message);
                let _ = self
                    .windows
                    .dispatch_message(&message, |_, message| script::answer(message.message));
            }
        });
    }
}

impl Route for OnDesk<'_> {
    fn register_class(&self, name: &CStr) {
        let _ = self.windows.register_class(
            CLASS_STYLE,
            script::procedure_address(),
            WINDOW_EXTRA,
            0,
            name.to_bytes(),
        );
    }

    fn create_window(&self, class: &CStr, title: &CStr, style: u32, place: [i32; 4]) -> u32 {
        self.windows.create_window(&Create {
            ex_style: 0,
            class: class.to_bytes(),
            title: title.to_bytes(),
            style,
            place,
            parent: 0,
            menu: 0,
            param: 0,
        })
    }

    fn destroy_window(&self, window: u32) {
        let _ = self.windows.destroy_window(window);
    }

    fn find_window(&self, class: &CStr) {
        let _ = self.windows.find_window(Some(class.to_bytes()), None);
    }

    fn load_icon(&self, name: u32) {
        let _ = self.windows.load_icon(0, name);
    }

    fn load_cursor(&self, name: u32) {
        let _ = self.input.load_cursor(0, Name::Number(name));
    }

    fn show_window(&self, window: u32, command: i32) {
        if self.plant == Plant::NoLine && !self.line_left_out.replace(true) {
            let _ = quietly(|| self.windows.show_window(window, command));
            return;
        }
        let _ = self.windows.show_window(window, command);
    }

    fn set_window_pos(&self, window: u32, place: [i32; 4], flags: u32) {
        let [x, y, width, height] = place;
        let _ = self
            .windows
            .set_window_pos(window, 0, rect(x, y, x + width, y + height), flags);
    }

    fn adjust_window_rect(&self, rectangle: [i32; 4], style: u32) {
        let [left, top, right, bottom] = rectangle;
        let _ = self
            .windows
            .adjust_window_rect(rect(left, top, right, bottom), style, false);
    }

    fn get_window_rect(&self, window: u32) {
        let _ = self.windows.get_window_rect(window);
    }

    fn get_window_long(&self, window: u32, index: i32) {
        let _ = self.windows.get_window_long(window, index);
    }

    fn set_window_long(&self, window: u32, index: i32, value: u32) {
        let _ = self.windows.set_window_long(window, index, value);
    }

    fn client_to_screen(&self, window: u32, x: i32, y: i32) {
        let _ = self.windows.client_to_screen(window, x, y);
    }

    fn screen_to_client(&self, window: u32, x: i32, y: i32) {
        let _ = self.windows.screen_to_client(window, x, y);
    }

    fn set_focus(&self, window: u32) {
        let _ = self.windows.set_focus(window);
    }

    fn get_system_metrics(&self, index: i32) {
        let _ = self.windows.get_system_metrics(index);
    }

    fn def_window_proc(&self, window: u32, message: u32, w: u32, l: u32) {
        let _ = self.windows.def_window_proc(window, message, w, l);
    }

    fn peek_message(&self, window: u32, low: u32, high: u32, remove: u32) -> Option<Message> {
        self.windows
            .peek_message(window, low, high, remove)
            .as_ref()
            .map(from_desk)
    }

    fn wait_message(&self) {
        let _ = self.windows.wait_message();
    }

    fn translate_message(&self, message: &Message) {
        let _ = self.windows.translate_message(&to_desk(message));
    }

    fn dispatch_message(&self, message: &Message) {
        let _ = self
            .windows
            .dispatch_message(&to_desk(message), |_, message| {
                script::answer(message.message)
            });
    }

    fn send_message(&self, window: u32, message: u32, w: u32, l: u32) {
        let _ = self
            .windows
            .send_message(window, message, w, l, |_, message| {
                script::answer(message.message)
            });
    }

    fn post_message(&self, window: u32, message: u32, w: u32, l: u32) {
        let _ = self.windows.post_message(window, message, w, l);
    }

    fn post_quit_message(&self, code: u32) {
        self.windows.post_quit_message(code);
    }

    fn get_message_time(&self) {
        let _ = self.windows.get_message_time();
    }

    fn get_cursor_pos(&self) {
        let _ = self.input.get_cursor_pos();
    }

    fn set_cursor_pos(&self, x: i32, y: i32) {
        let _ = self.input.set_cursor_pos(x, y);
    }

    fn show_cursor(&self, show: u32) {
        let _ = self.input.show_cursor(show);
    }

    fn clip_cursor(&self, clip: Option<[i32; 4]>) {
        let _ = self
            .input
            .clip_cursor(clip.map(|[left, top, right, bottom]| rect(left, top, right, bottom)));
    }

    fn get_async_key_state(&self, key: u32) {
        let _ = self.input.get_async_key_state(key);
    }

    fn get_key_state(&self, key: u32) {
        let _ = self.input.get_key_state(key);
    }

    fn map_virtual_key(&self, code: u32, kind: u32) {
        let _ = self.input.map_virtual_key(code, kind);
    }

    /// No host, so no focus to give.
    fn focus(&self) {}

    fn press(&self, window: u32, press: Press) {
        self.hand(window, press);
        self.pump();
    }

    fn press_and_keep(&self, window: u32, press: Press) {
        self.hand(window, press);
    }

    fn timer(&self) {
        quietly(|| {
            let _ = self.windows.post_message(0, WM_TIMER, TIMER_ID, TIMER_L);
        });
        if let Some(message) = self.peek_message(0, WM_TIMER, WM_TIMER, 1) {
            self.dispatch_message(&message);
        }
    }
}
