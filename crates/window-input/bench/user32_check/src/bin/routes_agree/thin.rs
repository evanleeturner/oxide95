// SPDX-License-Identifier: MIT
//! The route that asks the real USER32: `window::user32` and `window_input::user32`, under Wine.
use std::ffi::CStr;
use std::sync::mpsc::TryRecvError;
use std::thread;
use std::time::Duration;

use window::user32 as windows;
use window_input::user32 as input;

use crate::bench::{FROM, address, spot, text, words};
use crate::hand::{self, Class, Message, Point, Rect};
use crate::keys_and_cursor::Asker;
use crate::script::{
    CLASS_STYLE, Press, Route, WINDOW_EXTRA, WM_TIMER, procedure_address, timer_address,
};

const FIRST_KEY_MESSAGE: u32 = 0x100;
const LAST_KEY_MESSAGE: u32 = 0x109;
const TIMER_MS: u32 = 5;
const TURN: Duration = Duration::from_millis(5);
/// How long a press gets to reach this thread: as the asker's own settling.
const SETTLE_TURNS: u32 = 40;
/// How long the timer gets: far past its 5 ms.
const TIMER_TURNS: u32 = 400;

pub struct Thin {
    asker: Asker,
    instance: u32,
}

impl Thin {
    pub fn new() -> Self {
        Self {
            asker: Asker::new(),
            instance: hand::module(),
        }
    }
}

/// `hand::pump`, but the key messages stay waiting. Any peek takes in the host's input, so the
/// keys reach the queue and no further.
fn pump_but_keys() {
    let mut message = [0; 7];
    for (low, high) in [(0, FIRST_KEY_MESSAGE - 1), (LAST_KEY_MESSAGE + 1, u32::MAX)] {
        while hand::peek_message(&mut message, 0, low, high, 1) != 0 {
            hand::translate_message(&message);
            hand::dispatch_message(&message);
        }
    }
}

impl Route for Thin {
    fn register_class(&self, name: &CStr) {
        let class: Class = [
            CLASS_STYLE,
            procedure_address(),
            0,
            WINDOW_EXTRA,
            self.instance,
            0,
            0,
            0,
            0,
            text(name),
        ];
        // SAFETY: the record and the name it holds outlive the call.
        let _ = unsafe { windows::register_class(FROM, address(&class)) };
    }

    fn create_window(&self, class: &CStr, title: &CStr, style: u32, place: [i32; 4]) -> u32 {
        let [x, y, width, height] = words(place);
        // SAFETY: both texts outlive the call; the procedure is this program's own.
        unsafe {
            windows::create_window(
                FROM,
                0,
                text(class),
                text(title),
                style,
                x,
                y,
                width,
                height,
                0,
                0,
                self.instance,
                0,
            )
        }
    }

    fn destroy_window(&self, window: u32) {
        // SAFETY: a number; a bad handle is only a 0 answer.
        let _ = unsafe { windows::destroy_window(FROM, window) };
    }

    fn find_window(&self, class: &CStr) {
        // SAFETY: the text outlives the call.
        let _ = unsafe { windows::find_window(FROM, text(class), 0) };
    }

    fn load_icon(&self, name: u32) {
        // SAFETY: a number below `0x10000`.
        let _ = unsafe { windows::load_icon(FROM, 0, name) };
    }

    fn load_cursor(&self, name: u32) {
        // SAFETY: a number below `0x10000`.
        let _ = unsafe { input::load_cursor(FROM, 0, name) };
    }

    fn show_window(&self, window: u32, command: i32) {
        // SAFETY: numbers.
        let _ = unsafe { windows::show_window(FROM, window, command.cast_unsigned()) };
    }

    fn set_window_pos(&self, window: u32, place: [i32; 4], flags: u32) {
        let [x, y, width, height] = words(place);
        // SAFETY: numbers.
        let _ = unsafe { windows::set_window_pos(FROM, window, 0, x, y, width, height, flags) };
    }

    fn adjust_window_rect(&self, rectangle: [i32; 4], style: u32) {
        let mut rectangle: Rect = rectangle;
        // SAFETY: a record of the size it writes.
        let _ = unsafe { windows::adjust_window_rect(FROM, spot(&mut rectangle), style, 0) };
    }

    fn get_window_rect(&self, window: u32) {
        let mut rectangle: Rect = [0; 4];
        // SAFETY: a record of the size it writes.
        let _ = unsafe { windows::get_window_rect(FROM, window, spot(&mut rectangle)) };
    }

    fn get_window_long(&self, window: u32, index: i32) {
        let _ = windows::get_window_long(FROM, window, index.cast_unsigned());
    }

    fn set_window_long(&self, window: u32, index: i32, value: u32) {
        // SAFETY: numbers, at indexes that take a plain number.
        let _ = unsafe { windows::set_window_long(FROM, window, index.cast_unsigned(), value) };
    }

    fn client_to_screen(&self, window: u32, x: i32, y: i32) {
        let mut point: Point = [x, y];
        // SAFETY: a record of the size it writes.
        let _ = unsafe { windows::client_to_screen(FROM, window, spot(&mut point)) };
    }

    fn screen_to_client(&self, window: u32, x: i32, y: i32) {
        let mut point: Point = [x, y];
        // SAFETY: a record of the size it writes.
        let _ = unsafe { windows::screen_to_client(FROM, window, spot(&mut point)) };
    }

    fn set_focus(&self, window: u32) {
        // SAFETY: a number.
        let _ = unsafe { windows::set_focus(FROM, window) };
    }

    fn get_system_metrics(&self, index: i32) {
        let _ = windows::get_system_metrics(FROM, index.cast_unsigned());
    }

    fn def_window_proc(&self, window: u32, message: u32, w: u32, l: u32) {
        // SAFETY: numbers, of a message that holds no address.
        let _ = unsafe { windows::def_window_proc(FROM, window, message, w, l) };
    }

    fn peek_message(&self, window: u32, low: u32, high: u32, remove: u32) -> Option<Message> {
        let mut message = [0; 7];
        // SAFETY: a record of the size it writes.
        let got =
            unsafe { windows::peek_message(FROM, spot(&mut message), window, low, high, remove) };
        (got != 0).then_some(message)
    }

    fn wait_message(&self) {
        // SAFETY: takes nothing, and the script posted first, so it returns.
        let _ = unsafe { windows::wait_message(FROM) };
    }

    fn translate_message(&self, message: &Message) {
        // SAFETY: a whole record.
        let _ = unsafe { windows::translate_message(FROM, address(message)) };
    }

    fn dispatch_message(&self, message: &Message) {
        // SAFETY: a whole record; what it runs is this program's own or Wine's.
        let _ = unsafe { windows::dispatch_message(FROM, address(message)) };
    }

    fn send_message(&self, window: u32, message: u32, w: u32, l: u32) {
        // SAFETY: numbers; the procedure it runs is this program's own.
        let _ = unsafe { windows::send_message(FROM, window, message, w, l) };
    }

    fn post_message(&self, window: u32, message: u32, w: u32, l: u32) {
        // SAFETY: numbers.
        let _ = unsafe { windows::post_message(FROM, window, message, w, l) };
    }

    fn post_quit_message(&self, code: u32) {
        windows::post_quit_message(FROM, code);
    }

    fn get_message_time(&self) {
        let _ = windows::get_message_time(FROM);
    }

    fn get_cursor_pos(&self) {
        let mut point: Point = [0; 2];
        // SAFETY: a record of the size it writes.
        let _ = unsafe { input::get_cursor_pos(FROM, spot(&mut point)) };
    }

    fn set_cursor_pos(&self, x: i32, y: i32) {
        let _ = input::set_cursor_pos(FROM, x.cast_unsigned(), y.cast_unsigned());
    }

    fn show_cursor(&self, show: u32) {
        let _ = input::show_cursor(FROM, show);
    }

    fn clip_cursor(&self, clip: Option<[i32; 4]>) {
        let held: Option<Rect> = clip;
        let at = held.as_ref().map_or(0, address);
        // SAFETY: a whole record, read during the call, or 0 for none.
        let _ = unsafe { input::clip_cursor(FROM, at) };
    }

    fn get_async_key_state(&self, key: u32) {
        let _ = input::get_async_key_state(FROM, key);
    }

    fn get_key_state(&self, key: u32) {
        let _ = input::get_key_state(FROM, key);
    }

    fn map_virtual_key(&self, code: u32, kind: u32) {
        let _ = input::map_virtual_key(FROM, code, kind);
    }

    fn focus(&self) {
        self.asker.press("focus");
    }

    fn press(&self, _: u32, press: Press) {
        self.asker.press(press.request());
    }

    fn press_and_keep(&self, _: u32, press: Press) {
        println!("ask {}", press.request());
        loop {
            pump_but_keys();
            match self.asker.replies.try_recv() {
                Ok(_) => break,
                Err(TryRecvError::Empty) => thread::sleep(TURN),
                Err(TryRecvError::Disconnected) => panic!("the driver went away"),
            }
        }
        for _ in 0..SETTLE_TURNS {
            pump_but_keys();
            thread::sleep(TURN);
        }
    }

    fn timer(&self) {
        let id = hand::set_timer(0, 0, TIMER_MS, timer_address());
        let mut waiting = [0; 7];
        for _ in 0..TIMER_TURNS {
            if hand::peek_message(&mut waiting, 0, WM_TIMER, WM_TIMER, 0) != 0 {
                break;
            }
            thread::sleep(TURN);
        }
        if let Some(message) = self.peek_message(0, WM_TIMER, WM_TIMER, 1) {
            self.dispatch_message(&message);
        }
        hand::kill_timer(0, id);
    }
}
