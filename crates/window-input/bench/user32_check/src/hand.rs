// SPDX-License-Identifier: MIT
//! The control: USER32's calls declared here by hand, in Win32's own types, with no crate's
//! code in the way. A wrong answer beside one of these names the thin call; a wrong answer
//! from one of these names Wine or the computer.
//!
//! A handle or an address a thin call would take as a number stays a `u32` here too: that is
//! what it is on this 32-bit target. Each wrapper answers a `u32`, as a thin call does, so the
//! two compare directly.
use core::ffi::{CStr, c_char};
use core::ptr::{from_mut, from_ref, null, with_exposed_provenance};

/// `WNDCLASSA`, by its 10 fields.
pub type Class = [u32; 10];
/// `MSG`, by its 7 fields: window, message, `w`, `l`, time, x, y.
pub type Message = [u32; 7];
/// `POINT`.
pub type Point = [i32; 2];
/// `RECT`.
pub type Rect = [i32; 4];

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetLastError() -> u32;
    fn GetModuleHandleA(name: *const c_char) -> u32;
    fn SetLastError(value: u32);
}

#[link(name = "user32")]
unsafe extern "system" {
    fn AdjustWindowRect(rectangle: *mut Rect, style: u32, menu: i32) -> i32;
    fn ClientToScreen(window: u32, point: *mut Point) -> i32;
    fn ClipCursor(rectangle: *const Rect) -> i32;
    fn CreateWindowExA(
        extended: u32,
        class: *const c_char,
        title: *const c_char,
        style: u32,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        parent: u32,
        menu: u32,
        instance: u32,
        parameter: u32,
    ) -> u32;
    fn DefWindowProcA(window: u32, message: u32, w: u32, l: u32) -> u32;
    fn DestroyWindow(window: u32) -> i32;
    fn DispatchMessageA(message: *const Message) -> u32;
    fn FindWindowA(class: *const c_char, title: *const c_char) -> u32;
    fn GetAsyncKeyState(key: i32) -> i16;
    fn GetClassInfoA(instance: u32, name: *const c_char, class: *mut Class) -> i32;
    fn GetClipCursor(rectangle: *mut Rect) -> i32;
    fn GetCursorPos(point: *mut Point) -> i32;
    fn GetFocus() -> u32;
    fn GetKeyState(key: i32) -> i16;
    fn GetMessageTime() -> i32;
    fn GetSystemMetrics(index: i32) -> i32;
    fn GetWindowLongA(window: u32, index: i32) -> i32;
    fn GetWindowRect(window: u32, rectangle: *mut Rect) -> i32;
    fn IsWindow(window: u32) -> i32;
    fn IsWindowVisible(window: u32) -> i32;
    fn KillTimer(window: u32, id: u32) -> i32;
    fn LoadCursorA(instance: u32, name: *const c_char) -> u32;
    fn LoadIconA(instance: u32, name: *const c_char) -> u32;
    fn MapVirtualKeyA(code: u32, kind: u32) -> u32;
    fn PeekMessageA(message: *mut Message, window: u32, low: u32, high: u32, remove: u32) -> i32;
    fn PostMessageA(window: u32, message: u32, w: u32, l: u32) -> i32;
    fn PostQuitMessage(code: i32);
    fn RegisterClassA(class: *const Class) -> u16;
    fn ScreenToClient(window: u32, point: *mut Point) -> i32;
    fn SendMessageA(window: u32, message: u32, w: u32, l: u32) -> u32;
    fn SetCursorPos(x: i32, y: i32) -> i32;
    fn SetFocus(window: u32) -> u32;
    fn SetTimer(window: u32, id: u32, after_ms: u32, procedure: u32) -> u32;
    fn SetWindowLongA(window: u32, index: i32, value: i32) -> i32;
    fn SetWindowPos(
        window: u32,
        after: u32,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        flags: u32,
    ) -> i32;
    fn ShowCursor(show: i32) -> i32;
    fn ShowWindow(window: u32, command: i32) -> i32;
    fn TranslateMessage(message: *const Message) -> i32;
    fn WaitMessage() -> i32;
}

/// A name a call takes: 0, a number below `0x10000`, or a text's address.
fn named(address: u32) -> *const c_char {
    with_exposed_provenance(address as usize)
}

/// The thread's last error.
pub fn last_error() -> u32 {
    // SAFETY: takes nothing.
    unsafe { GetLastError() }
}

/// Sets the thread's last error, so a pair of calls starts from one marked value.
pub fn set_last_error(value: u32) {
    // SAFETY: one number, no address.
    unsafe { SetLastError(value) }
}

/// This program's own instance.
pub fn module() -> u32 {
    // SAFETY: null names this program.
    unsafe { GetModuleHandleA(null()) }
}

pub fn register_class(class: &Class) -> u32 {
    // SAFETY: the record and the texts it names outlive the call.
    u32::from(unsafe { RegisterClassA(from_ref(class)) })
}

/// The answer and the record `GetClassInfoA` wrote.
pub fn class_info(instance: u32, name: &CStr) -> (u32, Class) {
    let mut class = [0; 10];
    // SAFETY: a NUL-ended name and a record of the size it writes.
    let answer = unsafe { GetClassInfoA(instance, name.as_ptr(), from_mut(&mut class)) };
    (answer.cast_unsigned(), class)
}

/// A top-level window of a class, its other fields 0.
pub fn create_window(
    class: &CStr,
    title: &CStr,
    style: u32,
    place: [i32; 4],
    instance: u32,
) -> u32 {
    let [x, y, width, height] = place;
    // SAFETY: both texts outlive the call; its procedure is this program's own.
    unsafe {
        CreateWindowExA(
            0,
            class.as_ptr(),
            title.as_ptr(),
            style,
            x,
            y,
            width,
            height,
            0,
            0,
            instance,
            0,
        )
    }
}

pub fn destroy_window(window: u32) -> u32 {
    // SAFETY: a number; a bad handle is only a 0 answer.
    unsafe { DestroyWindow(window) }.cast_unsigned()
}

pub fn is_window(window: u32) -> u32 {
    // SAFETY: a number.
    unsafe { IsWindow(window) }.cast_unsigned()
}

pub fn is_visible(window: u32) -> u32 {
    // SAFETY: a number.
    unsafe { IsWindowVisible(window) }.cast_unsigned()
}

pub fn find_window(class: u32, title: u32) -> u32 {
    // SAFETY: each is 0, a number below `0x10000` or a text that outlives the call.
    unsafe { FindWindowA(named(class), named(title)) }
}

pub fn load_icon(instance: u32, name: u32) -> u32 {
    // SAFETY: a number below `0x10000` or a text that outlives the call.
    unsafe { LoadIconA(instance, named(name)) }
}

pub fn load_cursor(instance: u32, name: u32) -> u32 {
    // SAFETY: a number below `0x10000` or a text that outlives the call.
    unsafe { LoadCursorA(instance, named(name)) }
}

pub fn show_window(window: u32, command: i32) -> u32 {
    // SAFETY: numbers.
    unsafe { ShowWindow(window, command) }.cast_unsigned()
}

pub fn set_window_pos(window: u32, place: [i32; 4], flags: u32) -> u32 {
    let [x, y, width, height] = place;
    // SAFETY: numbers.
    unsafe { SetWindowPos(window, 0, x, y, width, height, flags) }.cast_unsigned()
}

pub fn adjust_window_rect(rectangle: &mut Rect, style: u32) -> u32 {
    // SAFETY: a record of the size it writes.
    unsafe { AdjustWindowRect(from_mut(rectangle), style, 0) }.cast_unsigned()
}

pub fn get_window_rect(window: u32, rectangle: &mut Rect) -> u32 {
    // SAFETY: a record of the size it writes.
    unsafe { GetWindowRect(window, from_mut(rectangle)) }.cast_unsigned()
}

pub fn get_window_long(window: u32, index: i32) -> u32 {
    // SAFETY: numbers.
    unsafe { GetWindowLongA(window, index) }.cast_unsigned()
}

pub fn set_window_long(window: u32, index: i32, value: i32) -> u32 {
    // SAFETY: numbers, at an index that takes a plain number.
    unsafe { SetWindowLongA(window, index, value) }.cast_unsigned()
}

pub fn client_to_screen(window: u32, point: &mut Point) -> u32 {
    // SAFETY: a record of the size it writes.
    unsafe { ClientToScreen(window, from_mut(point)) }.cast_unsigned()
}

pub fn screen_to_client(window: u32, point: &mut Point) -> u32 {
    // SAFETY: a record of the size it writes.
    unsafe { ScreenToClient(window, from_mut(point)) }.cast_unsigned()
}

pub fn set_focus(window: u32) -> u32 {
    // SAFETY: a number.
    unsafe { SetFocus(window) }
}

pub fn get_focus() -> u32 {
    // SAFETY: takes nothing.
    unsafe { GetFocus() }
}

pub fn get_system_metrics(index: i32) -> u32 {
    // SAFETY: a number.
    unsafe { GetSystemMetrics(index) }.cast_unsigned()
}

pub fn def_window_proc(window: u32, message: u32, w: u32, l: u32) -> u32 {
    // SAFETY: this program sends only messages whose `w` and `l` are plain numbers, and passes
    // on Wine's own whole.
    unsafe { DefWindowProcA(window, message, w, l) }
}

pub fn peek_message(message: &mut Message, window: u32, low: u32, high: u32, remove: u32) -> u32 {
    // SAFETY: a record of the size it writes.
    unsafe { PeekMessageA(from_mut(message), window, low, high, remove) }.cast_unsigned()
}

pub fn translate_message(message: &Message) -> u32 {
    // SAFETY: a whole record.
    unsafe { TranslateMessage(from_ref(message)) }.cast_unsigned()
}

pub fn dispatch_message(message: &Message) -> u32 {
    // SAFETY: a whole record; the procedure it runs is this program's own or Wine's.
    unsafe { DispatchMessageA(from_ref(message)) }
}

pub fn post_message(window: u32, message: u32, w: u32, l: u32) -> u32 {
    // SAFETY: numbers.
    unsafe { PostMessageA(window, message, w, l) }.cast_unsigned()
}

pub fn send_message(window: u32, message: u32, w: u32, l: u32) -> u32 {
    // SAFETY: numbers; the procedure it runs is this program's own.
    unsafe { SendMessageA(window, message, w, l) }
}

pub fn post_quit_message(code: i32) {
    // SAFETY: a number.
    unsafe { PostQuitMessage(code) }
}

pub fn wait_message() -> u32 {
    // SAFETY: takes nothing, and this program holds nothing a procedure touches.
    unsafe { WaitMessage() }.cast_unsigned()
}

pub fn get_message_time() -> u32 {
    // SAFETY: takes nothing.
    unsafe { GetMessageTime() }.cast_unsigned()
}

pub fn set_timer(window: u32, id: u32, after_ms: u32, procedure: u32) -> u32 {
    // SAFETY: numbers; `procedure` is 0 or a timer procedure of this program.
    unsafe { SetTimer(window, id, after_ms, procedure) }
}

pub fn kill_timer(window: u32, id: u32) -> u32 {
    // SAFETY: numbers.
    unsafe { KillTimer(window, id) }.cast_unsigned()
}

pub fn get_cursor_pos(point: &mut Point) -> u32 {
    // SAFETY: a record of the size it writes.
    unsafe { GetCursorPos(from_mut(point)) }.cast_unsigned()
}

pub fn set_cursor_pos(x: i32, y: i32) -> u32 {
    // SAFETY: numbers.
    unsafe { SetCursorPos(x, y) }.cast_unsigned()
}

pub fn show_cursor(show: i32) -> u32 {
    // SAFETY: a number.
    unsafe { ShowCursor(show) }.cast_unsigned()
}

pub fn clip_cursor(rectangle: &Rect) -> u32 {
    // SAFETY: a whole record, read during the call.
    unsafe { ClipCursor(from_ref(rectangle)) }.cast_unsigned()
}

pub fn get_clip_cursor(rectangle: &mut Rect) -> u32 {
    // SAFETY: a record of the size it writes.
    unsafe { GetClipCursor(from_mut(rectangle)) }.cast_unsigned()
}

/// The `SHORT`'s 16 bits, as the caller reads them.
pub fn get_async_key_state(key: u32) -> u32 {
    // SAFETY: a number; the call takes any.
    u32::from(unsafe { GetAsyncKeyState(key.cast_signed()) }.cast_unsigned())
}

/// The `SHORT`'s 16 bits, as the caller reads them.
pub fn get_key_state(key: u32) -> u32 {
    // SAFETY: a number; the call takes any.
    u32::from(unsafe { GetKeyState(key.cast_signed()) }.cast_unsigned())
}

pub fn map_virtual_key(code: u32, kind: u32) -> u32 {
    // SAFETY: numbers.
    unsafe { MapVirtualKeyA(code, kind) }
}

/// Takes and runs every message waiting, so the queue is empty and the host's input has
/// reached this thread. Every read here is by the hand-declared calls.
pub fn pump() {
    let mut message = [0; 7];
    while peek_message(&mut message, 0, 0, 0, 1) != 0 {
        translate_message(&message);
        dispatch_message(&message);
    }
}
