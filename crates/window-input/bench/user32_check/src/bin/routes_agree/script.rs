// SPDX-License-Identifier: MIT
//! The one list of calls, over either route. Each step prints `step <name>` first, and each call
//! prints its own line by the logger, so the scorer can line the 2 logs up step by step. The script
//! reads no answer but what the next call needs: judging is the scorer's.
use std::ffi::CStr;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::bench::{NO_WINDOW, code};
use crate::hand::{self, Message};

pub const CLASS_STYLE: u32 = 3;
pub const WINDOW_EXTRA: u32 = 8;
pub const WM_TIMER: u32 = 0x113;
const WM_QUIT: u32 = 0x12;
const WM_USER: u32 = 0x400;
/// What the procedure answers by itself.
const ASKED: u32 = WM_USER + 1;
const ANSWERED: u32 = 0x55;
/// Posted to no window.
const POSTED_TO_NONE: u32 = WM_USER + 2;
const PASSED_ON: u32 = WM_USER + 3;
const FIRST_KEY_MESSAGE: u32 = 0x100;
const LAST_KEY_MESSAGE: u32 = 0x109;
const KEY_A: u32 = 0x41;
/// `IDI_APPLICATION` and `IDC_ARROW`.
const STOCK: u32 = 0x7f00;
/// `WS_POPUP | WS_VISIBLE`: no frame, and the title the driver's `focus` seeks.
const VISIBLE_POPUP: u32 = 0x9000_0000;
/// `WS_OVERLAPPEDWINDOW`: a frame.
const OVERLAPPED: u32 = 0x00cf_0000;
const NO_MOVE_OF_ORDER: u32 = 0x14;
const OPEN_DRAIN_TURNS: u32 = 64;
const KEY_DRAIN_TURNS: u32 = 16;

static REACHED: AtomicU32 = AtomicU32::new(0);

/// A key's edges, by `xdotool`'s word on the one route and by events on the other.
#[derive(Clone, Copy)]
pub enum Press {
    Down,
    Up,
    Tap,
}

impl Press {
    pub const fn request(self) -> &'static str {
        match self {
            Self::Down => "keydown a",
            Self::Up => "keyup a",
            Self::Tap => "key a",
        }
    }

    pub const fn edges(self) -> &'static [bool] {
        match self {
            Self::Down => &[true],
            Self::Up => &[false],
            Self::Tap => &[true, false],
        }
    }
}

/// One method per call name, then what is no call: a press, the host's focus and the timer.
pub trait Route {
    fn register_class(&self, name: &CStr);
    fn create_window(&self, class: &CStr, title: &CStr, style: u32, place: [i32; 4]) -> u32;
    fn destroy_window(&self, window: u32);
    fn find_window(&self, class: &CStr);
    fn load_icon(&self, name: u32);
    fn load_cursor(&self, name: u32);
    fn show_window(&self, window: u32, command: i32);
    fn set_window_pos(&self, window: u32, place: [i32; 4], flags: u32);
    fn adjust_window_rect(&self, rectangle: [i32; 4], style: u32);
    fn get_window_rect(&self, window: u32);
    fn get_window_long(&self, window: u32, index: i32);
    fn set_window_long(&self, window: u32, index: i32, value: u32);
    fn client_to_screen(&self, window: u32, x: i32, y: i32);
    fn screen_to_client(&self, window: u32, x: i32, y: i32);
    fn set_focus(&self, window: u32);
    fn get_system_metrics(&self, index: i32);
    fn def_window_proc(&self, window: u32, message: u32, w: u32, l: u32);
    fn peek_message(&self, window: u32, low: u32, high: u32, remove: u32) -> Option<Message>;
    fn wait_message(&self);
    fn translate_message(&self, message: &Message);
    fn dispatch_message(&self, message: &Message);
    fn send_message(&self, window: u32, message: u32, w: u32, l: u32);
    fn post_message(&self, window: u32, message: u32, w: u32, l: u32);
    fn post_quit_message(&self, code: u32);
    fn get_message_time(&self);
    fn get_cursor_pos(&self);
    fn set_cursor_pos(&self, x: i32, y: i32);
    fn show_cursor(&self, show: u32);
    fn clip_cursor(&self, clip: Option<[i32; 4]>);
    fn get_async_key_state(&self, key: u32);
    fn get_key_state(&self, key: u32);
    fn map_virtual_key(&self, code: u32, kind: u32);

    /// The window the keys go to takes the host's focus, where a host has one.
    fn focus(&self);
    /// A key's edges reach the program, and its messages are taken and run, as a program's loop
    /// would.
    fn press(&self, window: u32, press: Press);
    /// A key's edges reach the program, and its messages stay waiting for the next peek.
    fn press_and_keep(&self, window: u32, press: Press);
    /// A message of the timer's kind, of window 0, is peeked and dispatched.
    fn timer(&self);
}

/// The one answer the procedure gives by itself; the real route's procedure passes the rest on.
pub fn answer(message: u32) -> u32 {
    match message {
        ASKED => ANSWERED,
        WM_TIMER => {
            REACHED.fetch_add(1, Ordering::Relaxed);
            0
        }
        _ => 0,
    }
}

unsafe extern "system" fn procedure(window: u32, message: u32, w: u32, l: u32) -> u32 {
    if message == ASKED {
        answer(message)
    } else {
        hand::def_window_proc(window, message, w, l)
    }
}

unsafe extern "system" fn timer_procedure(_: u32, message: u32, _: u32, _: u32) -> u32 {
    answer(message)
}

/// The procedure's address: both routes name the same one.
pub fn procedure_address() -> u32 {
    code(procedure as *const ())
}

/// The timer procedure's address.
pub fn timer_address() -> u32 {
    code(timer_procedure as *const ())
}

fn step(name: &str) {
    println!("step {name}");
}

/// The whole script, in its one order.
pub fn run(route: &impl Route) {
    let class = c"uc_routes";
    step("classes");
    route.register_class(class);
    route.register_class(class);
    let [first, second] = windows(route, class);
    step("find");
    route.find_window(class);
    route.find_window(c"uc_nobody");
    place(route, first, second);
    longs(route, first);
    step("show");
    route.show_window(second, 5);
    route.show_window(second, 0);
    route.show_window(first, 5);
    route.set_focus(first);
    step("metrics");
    for index in [0, 1, 11, 12, 9999] {
        route.get_system_metrics(index);
    }
    step("queue");
    for _ in 0..OPEN_DRAIN_TURNS {
        if route.peek_message(0, 0, 0, 1).is_none() {
            break;
        }
    }
    messages(route, first);
    cursor(route);
    keys(route, first);
    step("timer");
    route.timer();
    println!("reached {}", REACHED.load(Ordering::Relaxed));
    step("close");
    route.destroy_window(second);
    route.destroy_window(second);
    route.destroy_window(first);
}

fn windows(route: &impl Route, class: &CStr) -> [u32; 2] {
    step("windows");
    route.load_icon(STOCK);
    route.load_cursor(STOCK);
    let first = route.create_window(class, c"uc keys", VISIBLE_POPUP, [40, 40, 640, 480]);
    let second = route.create_window(class, c"uc other", OVERLAPPED, [100, 100, 300, 200]);
    [first, second]
}

fn place(route: &impl Route, first: u32, second: u32) {
    step("place");
    route.get_window_rect(first);
    route.set_window_pos(first, [40, 40, 640, 480], NO_MOVE_OF_ORDER);
    route.get_window_rect(first);
    route.get_window_rect(NO_WINDOW);
    route.adjust_window_rect([0, 0, 640, 480], VISIBLE_POPUP);
    route.adjust_window_rect([0, 0, 640, 480], OVERLAPPED);
    route.client_to_screen(first, 10, 20);
    route.screen_to_client(first, 100, 100);
    route.client_to_screen(second, 10, 20);
    route.screen_to_client(NO_WINDOW, 1, 2);
}

fn longs(route: &impl Route, window: u32) {
    step("longs");
    for index in [-16, -20, -4, -21] {
        route.get_window_long(window, index);
    }
    route.set_window_long(window, -21, 0x1234);
    route.get_window_long(window, -21);
    route.get_window_long(window, 0);
    route.set_window_long(window, 4, 0x5678);
    route.get_window_long(window, 4);
    route.get_window_long(window, 400);
}

fn messages(route: &impl Route, window: u32) {
    step("messages");
    route.post_message(window, ASKED, 2, 3);
    route.post_message(0, POSTED_TO_NONE, 4, 5);
    for (target, number) in [(window, ASKED), (0, POSTED_TO_NONE)] {
        route.peek_message(target, number, number, 0);
        if let Some(message) = route.peek_message(target, number, number, 1) {
            route.translate_message(&message);
            route.dispatch_message(&message);
        }
        route.get_message_time();
    }
    route.peek_message(window, ASKED, ASKED, 1);
    route.send_message(window, ASKED, 6, 7);
    route.def_window_proc(window, PASSED_ON, 0, 0);
    route.post_quit_message(9);
    route.peek_message(0, WM_QUIT, WM_QUIT, 1);
    // Never on an empty queue: nothing here would wake it.
    route.post_message(window, ASKED, 8, 9);
    route.wait_message();
    route.peek_message(window, ASKED, ASKED, 1);
}

fn cursor(route: &impl Route) {
    step("cursor");
    route.set_cursor_pos(100, 100);
    route.get_cursor_pos();
    route.show_cursor(0);
    route.show_cursor(1);
    route.clip_cursor(Some([50, 50, 300, 200]));
    route.set_cursor_pos(500, 400);
    route.get_cursor_pos();
    route.clip_cursor(None);
    route.set_cursor_pos(100, 100);
}

fn keys(route: &impl Route, window: u32) {
    route.focus();
    step("keys.idle");
    route.get_async_key_state(KEY_A);
    route.map_virtual_key(KEY_A, 2);
    step("keys.held");
    route.press(window, Press::Down);
    route.get_async_key_state(KEY_A);
    route.get_async_key_state(KEY_A);
    route.get_key_state(KEY_A);
    route.press(window, Press::Up);
    route.get_async_key_state(KEY_A);
    step("keys.tap");
    route.press(window, Press::Tap);
    route.get_async_key_state(KEY_A);
    route.get_async_key_state(KEY_A);
    step("keys.queue");
    route.press_and_keep(window, Press::Tap);
    for _ in 0..KEY_DRAIN_TURNS {
        let Some(message) = route.peek_message(0, FIRST_KEY_MESSAGE, LAST_KEY_MESSAGE, 1) else {
            break;
        };
        route.translate_message(&message);
        route.dispatch_message(&message);
    }
}
