// SPDX-License-Identifier: MIT
//! The direct back, on the Win32 target alone: each front asks the real
//! `USER32` and writes one `imp.call` line, `by=window`. No desk, no table
//! and no queue of its own: the handle is Wine's, the message is Wine's,
//! and the program's own loop is the only loop.
//!
//! Why here, and not behind [`crate::Screen`]: a process that holds Miles'
//! and Smacker's own `USER32` calls beside the program's needs one set of
//! windows and one queue, and those libraries find and change the real
//! ones by themselves. A desk's windows would be strangers to them.
//!
//! Said plainly: on this target the answer is the original's call, written
//! in this crate. `by=window` says who wrote the call. It does not say a
//! window manager was rebuilt: [`crate::desk`] and [`crate::front`] are
//! that, on the page's and the host's targets.
//!
//! A front takes the numbers its caller handed in, addresses included, as
//! `u32`: on this target they are what a record or a text's pointer is,
//! and a front passes them on whole. `from` is the address the call
//! returns to, shown on the line and never used. `c` spells the import as
//! the layer's rows do, so a reader of one log reads the other.
//!
//! The empty peek: [`peek_message`] leaves the record to Wine, so what an
//! empty queue leaves there is what Wine leaves: all 7 fields unchanged,
//! measured under Wine 9.0. A program whose loop never tests the answer
//! reads its last message again, by the original's own rule.
#![allow(
    unsafe_code,
    reason = "this back is calls into USER32 and nothing else"
)]

use crate::Numbers;
use core::ffi::CStr;
use core::fmt;
use core::ptr::with_exposed_provenance;

unsafe extern "system" {
    fn CreateWindowExA(
        extended: u32,
        class: u32,
        title: u32,
        style: u32,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        parent: u32,
        menu: u32,
        instance: u32,
        parameter: u32,
    ) -> u32;
    fn AdjustWindowRect(rectangle: u32, style: u32, menu: u32) -> u32;
    fn ClientToScreen(window: u32, point: u32) -> u32;
    fn DefWindowProcA(window: u32, message: u32, w: u32, l: u32) -> u32;
    fn DestroyWindow(window: u32) -> u32;
    fn DispatchMessageA(message: u32) -> u32;
    fn FindWindowA(class: u32, title: u32) -> u32;
    fn GetLastError() -> u32;
    fn GetMessageTime() -> u32;
    fn GetSystemMetrics(index: u32) -> u32;
    fn GetWindowLongA(window: u32, index: u32) -> u32;
    fn GetWindowRect(window: u32, rectangle: u32) -> u32;
    fn LoadIconA(instance: u32, name: u32) -> u32;
    fn PeekMessageA(message: u32, window: u32, low: u32, high: u32, remove: u32) -> u32;
    fn PostMessageA(window: u32, message: u32, w: u32, l: u32) -> u32;
    fn PostQuitMessage(code: u32);
    fn RegisterClassA(class: u32) -> u32;
    fn ScreenToClient(window: u32, point: u32) -> u32;
    fn SendMessageA(window: u32, message: u32, w: u32, l: u32) -> u32;
    fn SetFocus(window: u32) -> u32;
    fn SetLastError(value: u32);
    fn SetWindowLongA(window: u32, index: u32, value: u32) -> u32;
    fn SetWindowPos(
        window: u32,
        after: u32,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        flags: u32,
    ) -> u32;
    fn ShowWindow(window: u32, command: u32) -> u32;
    fn TranslateMessage(message: u32) -> u32;
    fn WaitMessage() -> u32;
}

/// Writes one `imp.call` line, the thread's last error kept around it: a
/// logger writes a file, and the program reads the error its call left.
pub fn kept(line: impl FnOnce()) {
    // SAFETY: reads the thread's own last error.
    let value = unsafe { GetLastError() };
    line();
    // SAFETY: sets back the value read above.
    unsafe { SetLastError(value) };
}

/// [`kept`], for a front here: each writes its `log::debug!` whole, so the
/// logline fence reads its fields.
macro_rules! kept {
    ($line:expr) => {
        kept(|| {
            $line;
        })
    };
}

/// Text as a line shows it: printable ASCII as it is, any other byte `\xNN`.
struct Text<'a>(&'a [u8]);

impl fmt::Display for Text<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|byte| match byte {
            b' '..=b'~' => write!(f, "{}", char::from(*byte)),
            _ => write!(f, "\\x{byte:02x}"),
        })
    }
}

/// A name a call is handed, as `s` and `with` show it.
pub enum Name<'a> {
    /// A NUL-ended text, the NUL left out; empty for a null name.
    Text(&'a [u8]),
    /// An atom or a resource id: a number below `0x10000`.
    Number(u32),
}

impl fmt::Display for Name<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Name::Text(text) => Text(text).fmt(f),
            Name::Number(number) => {
                f.write_str("#")?;
                oxide_trace::__shown!(*number).fmt(f)
            }
        }
    }
}

/// The name at `address`: nothing for null, a number below `0x10000`.
///
/// # Safety
/// `address` is below `0x10000` or a NUL-ended text, as Win32 takes it.
#[must_use]
pub unsafe fn name<'a>(address: u32) -> Name<'a> {
    match address {
        0 => Name::Text(&[]),
        1..0x10000 => Name::Number(address),
        // SAFETY: the caller's NUL-ended text.
        _ => Name::Text(
            unsafe { CStr::from_ptr(with_exposed_provenance(address as usize)) }.to_bytes(),
        ),
    }
}

/// The `N` numbers of the record at `address`, in its fields' order; all
/// 0 for a null one.
///
/// # Safety
/// `address` is 0 or `4 * N` bytes the caller handed in.
#[must_use]
pub unsafe fn record<const N: usize>(address: u32) -> [u32; N] {
    if address == 0 {
        return [0; N];
    }
    // SAFETY: the caller's record, read where it lies.
    unsafe { with_exposed_provenance::<[u32; N]>(address as usize).read_unaligned() }
}

/// What a call wrote at its record, the line's `o`: `N` numbers, or
/// nothing on a 0 answer.
pub struct Wrote<const N: usize>(Option<[u32; N]>);

impl<const N: usize> fmt::Display for Wrote<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0
            .as_ref()
            .map_or(Ok(()), |numbers| Numbers(numbers).fmt(f))
    }
}

/// The record at `address` after a call answered `value`; none on 0.
///
/// # Safety
/// `address` is 0 or `4 * N` bytes the caller handed in.
#[must_use]
pub unsafe fn wrote<const N: usize>(value: u32, address: u32) -> Wrote<N> {
    // SAFETY: the caller's record, read on a nonzero answer alone.
    Wrote((value != 0).then(|| unsafe { record(address) }))
}

/// `RegisterClassA`: `a` the class record's first 8 fields (style,
/// procedure, the two extra counts, instance, icon, cursor, brush), `s`
/// its class name, `with` its menu name.
///
/// # Safety
/// `class` is a `WNDCLASSA`, as `RegisterClassA` takes it.
#[must_use]
pub unsafe fn register_class(from: u32, class: u32) -> u32 {
    // SAFETY: the caller's class record, 10 fields, read before the call.
    let fields = unsafe { record::<10>(class) };
    // SAFETY: the caller's argument, passed on whole.
    let value = unsafe { RegisterClassA(class) };
    // SAFETY: the record's class and menu names, each a number or a text.
    let (named, menu) = unsafe { (name(fields[9]), name(fields[8])) };
    kept!(
        log::debug!(c = "USER32!RegisterClassA", a:% = Numbers(&fields[..8]),
        s:% = named, with:% = menu, v:% = oxide_trace::__shown!(value),
        from:% = oxide_trace::__shown!(from), by = "window"; "imp.call")
    );
    value
}

/// `CreateWindowExA`: `s` the class, `with` the title.
///
/// # Safety
/// The arguments are as `CreateWindowExA` takes them. Its procedure runs
/// inside this call.
#[allow(
    clippy::too_many_arguments,
    reason = "CreateWindowExA's own 12, and the caller first"
)]
#[must_use]
pub unsafe fn create_window(
    from: u32,
    extended: u32,
    class: u32,
    title: u32,
    style: u32,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    parent: u32,
    menu: u32,
    instance: u32,
    parameter: u32,
) -> u32 {
    // SAFETY: the caller's arguments, passed on whole.
    let value = unsafe {
        CreateWindowExA(
            extended, class, title, style, x, y, width, height, parent, menu, instance, parameter,
        )
    };
    // SAFETY: the caller's class and title, each a number or a text.
    let (class, title) = unsafe { (name(class), name(title)) };
    kept!(log::debug!(c = "USER32!CreateWindowExA",
        a:% = Numbers(&[extended, style, x, y, width, height, parent, menu, instance, parameter]),
        s:% = class, with:% = title, v:% = oxide_trace::__shown!(value),
        from:% = oxide_trace::__shown!(from), by = "window"; "imp.call"));
    value
}

/// `DestroyWindow`.
///
/// # Safety
/// The window's procedure runs inside this call, on whatever it holds.
#[must_use]
pub unsafe fn destroy_window(from: u32, window: u32) -> u32 {
    // SAFETY: the caller's argument, passed on whole.
    let value = unsafe { DestroyWindow(window) };
    kept!(
        log::debug!(c = "USER32!DestroyWindow", a:% = Numbers(&[window]),
        v:% = oxide_trace::__shown!(value), from:% = oxide_trace::__shown!(from),
        by = "window"; "imp.call")
    );
    value
}

/// `DefWindowProcA`.
///
/// # Safety
/// `w` and `l` are what `message` asks for: some messages' are addresses
/// `DefWindowProcA` reads.
#[must_use]
pub unsafe fn def_window_proc(from: u32, window: u32, message: u32, w: u32, l: u32) -> u32 {
    // SAFETY: the caller's arguments, passed on whole.
    let value = unsafe { DefWindowProcA(window, message, w, l) };
    kept!(
        log::debug!(c = "USER32!DefWindowProcA", a:% = Numbers(&[window, message, w, l]),
        v:% = oxide_trace::__shown!(value), from:% = oxide_trace::__shown!(from),
        by = "window"; "imp.call")
    );
    value
}

/// `PeekMessageA`: `a` the window, the range and the flag; `o` the 7
/// fields of the message it wrote, none when it answered 0.
///
/// # Safety
/// `message` is a `MSG` the call may write.
#[must_use]
pub unsafe fn peek_message(
    from: u32,
    message: u32,
    window: u32,
    low: u32,
    high: u32,
    remove: u32,
) -> u32 {
    // SAFETY: the caller's arguments, passed on whole.
    let value = unsafe { PeekMessageA(message, window, low, high, remove) };
    // SAFETY: the caller's message record.
    let got = unsafe { wrote::<7>(value, message) };
    kept!(
        log::debug!(c = "USER32!PeekMessageA", a:% = Numbers(&[window, low, high, remove]),
        o:% = got, v:% = oxide_trace::__shown!(value), from:% = oxide_trace::__shown!(from),
        by = "window"; "imp.call")
    );
    value
}

/// `TranslateMessage`: `a` the message record's 7 fields (window, message,
/// `w`, `l`, time, x, y), read before the call.
///
/// # Safety
/// `message` is a `MSG`.
#[must_use]
pub unsafe fn translate_message(from: u32, message: u32) -> u32 {
    // SAFETY: the caller's message record, read before the call.
    let fields = unsafe { record::<7>(message) };
    // SAFETY: the caller's argument, passed on whole.
    let value = unsafe { TranslateMessage(message) };
    kept!(
        log::debug!(c = "USER32!TranslateMessage", a:% = Numbers(&fields),
        v:% = oxide_trace::__shown!(value), from:% = oxide_trace::__shown!(from),
        by = "window"; "imp.call")
    );
    value
}

/// `DispatchMessageA`: `a` the message record's 7 fields, read before the
/// call, as the procedure it calls may reuse the record.
///
/// This is the name that makes a timer's procedure run: Wine's `WM_TIMER`
/// carries a number of its own in `l`, not the address `SetTimer` was
/// given, so only the real `DispatchMessageA` can call it. Measured under
/// Wine 9.0: `l` read `ffff000e`.
///
/// # Safety
/// `message` is a `MSG`; its window's procedure runs inside this call.
#[must_use]
pub unsafe fn dispatch_message(from: u32, message: u32) -> u32 {
    // SAFETY: the caller's message record, read before the call.
    let fields = unsafe { record::<7>(message) };
    // SAFETY: the caller's argument, passed on whole.
    let value = unsafe { DispatchMessageA(message) };
    kept!(
        log::debug!(c = "USER32!DispatchMessageA", a:% = Numbers(&fields),
        v:% = oxide_trace::__shown!(value), from:% = oxide_trace::__shown!(from),
        by = "window"; "imp.call")
    );
    value
}

/// `PostMessageA`.
///
/// # Safety
/// `w` and `l` stay valid until the window's procedure has read them, when
/// `message` makes either an address.
#[must_use]
pub unsafe fn post_message(from: u32, window: u32, message: u32, w: u32, l: u32) -> u32 {
    // SAFETY: the caller's arguments, passed on whole.
    let value = unsafe { PostMessageA(window, message, w, l) };
    kept!(
        log::debug!(c = "USER32!PostMessageA", a:% = Numbers(&[window, message, w, l]),
        v:% = oxide_trace::__shown!(value), from:% = oxide_trace::__shown!(from),
        by = "window"; "imp.call")
    );
    value
}

/// `FindWindowA`: `s` the class, `with` the title.
///
/// # Safety
/// `class` and `title` are each 0, a number below `0x10000`, or a NUL-ended
/// text.
#[must_use]
pub unsafe fn find_window(from: u32, class: u32, title: u32) -> u32 {
    // SAFETY: the caller's arguments, passed on whole.
    let value = unsafe { FindWindowA(class, title) };
    // SAFETY: the caller's class and title, each a number or a text.
    let (class, title) = unsafe { (name(class), name(title)) };
    kept!(
        log::debug!(c = "USER32!FindWindowA", s:% = class, with:% = title,
        v:% = oxide_trace::__shown!(value), from:% = oxide_trace::__shown!(from),
        by = "window"; "imp.call")
    );
    value
}

/// `LoadIconA`: `a` the instance, `s` the icon's name or resource id.
///
/// # Safety
/// `icon` is a number below `0x10000` or a NUL-ended text.
#[must_use]
pub unsafe fn load_icon(from: u32, instance: u32, icon: u32) -> u32 {
    // SAFETY: the caller's arguments, passed on whole.
    let value = unsafe { LoadIconA(instance, icon) };
    // SAFETY: the caller's icon name, a resource id or a text.
    let icon = unsafe { name(icon) };
    kept!(
        log::debug!(c = "USER32!LoadIconA", a:% = Numbers(&[instance]), s:% = icon,
        v:% = oxide_trace::__shown!(value), from:% = oxide_trace::__shown!(from),
        by = "window"; "imp.call")
    );
    value
}

/// `ShowWindow`.
///
/// # Safety
/// The window's procedure runs inside this call, on whatever it holds.
#[must_use]
pub unsafe fn show_window(from: u32, window: u32, command: u32) -> u32 {
    // SAFETY: the caller's arguments, passed on whole.
    let value = unsafe { ShowWindow(window, command) };
    kept!(
        log::debug!(c = "USER32!ShowWindow", a:% = Numbers(&[window, command]),
        v:% = oxide_trace::__shown!(value), from:% = oxide_trace::__shown!(from),
        by = "window"; "imp.call")
    );
    value
}

/// `SetWindowPos`.
///
/// # Safety
/// The window's procedure runs inside this call, on whatever it holds.
#[allow(
    clippy::too_many_arguments,
    reason = "SetWindowPos's own 7, and the caller first"
)]
#[must_use]
pub unsafe fn set_window_pos(
    from: u32,
    window: u32,
    after: u32,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    flags: u32,
) -> u32 {
    // SAFETY: the caller's arguments, passed on whole.
    let value = unsafe { SetWindowPos(window, after, x, y, width, height, flags) };
    kept!(log::debug!(c = "USER32!SetWindowPos",
        a:% = Numbers(&[window, after, x, y, width, height, flags]),
        v:% = oxide_trace::__shown!(value), from:% = oxide_trace::__shown!(from),
        by = "window"; "imp.call"));
    value
}

/// `AdjustWindowRect`: `a` the rectangle's 4 fields as handed, the style
/// and the menu flag; `o` the 4 it wrote, none when it answered 0.
///
/// # Safety
/// `rectangle` is a `RECT` the call may write.
#[must_use]
pub unsafe fn adjust_window_rect(from: u32, rectangle: u32, style: u32, menu: u32) -> u32 {
    // SAFETY: the caller's rectangle, read before the call.
    let [left, top, right, bottom] = unsafe { record::<4>(rectangle) };
    // SAFETY: the caller's arguments, passed on whole.
    let value = unsafe { AdjustWindowRect(rectangle, style, menu) };
    // SAFETY: the caller's rectangle.
    let got = unsafe { wrote::<4>(value, rectangle) };
    kept!(log::debug!(c = "USER32!AdjustWindowRect",
        a:% = Numbers(&[left, top, right, bottom, style, menu]), o:% = got,
        v:% = oxide_trace::__shown!(value), from:% = oxide_trace::__shown!(from),
        by = "window"; "imp.call"));
    value
}

/// `GetWindowRect`: `a` the window; `o` the rectangle's 4 fields it wrote,
/// none when it answered 0.
///
/// # Safety
/// `rectangle` is a `RECT` the call may write.
#[must_use]
pub unsafe fn get_window_rect(from: u32, window: u32, rectangle: u32) -> u32 {
    // SAFETY: the caller's arguments, passed on whole.
    let value = unsafe { GetWindowRect(window, rectangle) };
    // SAFETY: the caller's rectangle.
    let got = unsafe { wrote::<4>(value, rectangle) };
    kept!(
        log::debug!(c = "USER32!GetWindowRect", a:% = Numbers(&[window]), o:% = got,
        v:% = oxide_trace::__shown!(value), from:% = oxide_trace::__shown!(from),
        by = "window"; "imp.call")
    );
    value
}

/// `GetWindowLongA`: reads a number off the window and runs nothing, so a
/// bad handle or index is only a 0 answer.
#[must_use]
pub fn get_window_long(from: u32, window: u32, index: u32) -> u32 {
    // SAFETY: two numbers, no address; the call reads the window's own table.
    let value = unsafe { GetWindowLongA(window, index) };
    kept!(
        log::debug!(c = "USER32!GetWindowLongA", a:% = Numbers(&[window, index]),
        v:% = oxide_trace::__shown!(value), from:% = oxide_trace::__shown!(from),
        by = "window"; "imp.call")
    );
    value
}

/// `SetWindowLongA`.
///
/// # Safety
/// `long` is what `index` asks for: at `GWL_WNDPROC` it becomes the
/// window's procedure, called on every message after.
#[must_use]
pub unsafe fn set_window_long(from: u32, window: u32, index: u32, long: u32) -> u32 {
    // SAFETY: the caller's arguments, passed on whole.
    let value = unsafe { SetWindowLongA(window, index, long) };
    kept!(
        log::debug!(c = "USER32!SetWindowLongA", a:% = Numbers(&[window, index, long]),
        v:% = oxide_trace::__shown!(value), from:% = oxide_trace::__shown!(from),
        by = "window"; "imp.call")
    );
    value
}

/// `ClientToScreen`: `a` the window and the point as handed; `o` the point
/// it wrote, none when it answered 0.
///
/// # Safety
/// `point` is a `POINT` the call may write.
#[must_use]
pub unsafe fn client_to_screen(from: u32, window: u32, point: u32) -> u32 {
    // SAFETY: the caller's point, read before the call.
    let [x, y] = unsafe { record::<2>(point) };
    // SAFETY: the caller's arguments, passed on whole.
    let value = unsafe { ClientToScreen(window, point) };
    // SAFETY: the caller's point.
    let got = unsafe { wrote::<2>(value, point) };
    kept!(
        log::debug!(c = "USER32!ClientToScreen", a:% = Numbers(&[window, x, y]), o:% = got,
        v:% = oxide_trace::__shown!(value), from:% = oxide_trace::__shown!(from),
        by = "window"; "imp.call")
    );
    value
}

/// `ScreenToClient`: as [`client_to_screen`], the other way.
///
/// # Safety
/// `point` is a `POINT` the call may write.
#[must_use]
pub unsafe fn screen_to_client(from: u32, window: u32, point: u32) -> u32 {
    // SAFETY: the caller's point, read before the call.
    let [x, y] = unsafe { record::<2>(point) };
    // SAFETY: the caller's arguments, passed on whole.
    let value = unsafe { ScreenToClient(window, point) };
    // SAFETY: the caller's point.
    let got = unsafe { wrote::<2>(value, point) };
    kept!(
        log::debug!(c = "USER32!ScreenToClient", a:% = Numbers(&[window, x, y]), o:% = got,
        v:% = oxide_trace::__shown!(value), from:% = oxide_trace::__shown!(from),
        by = "window"; "imp.call")
    );
    value
}

/// `SetFocus`.
///
/// # Safety
/// The procedures of the window losing focus and the window taking it run
/// inside this call, on whatever they hold.
#[must_use]
pub unsafe fn set_focus(from: u32, window: u32) -> u32 {
    // SAFETY: the caller's argument, passed on whole.
    let value = unsafe { SetFocus(window) };
    kept!(log::debug!(c = "USER32!SetFocus", a:% = Numbers(&[window]),
        v:% = oxide_trace::__shown!(value), from:% = oxide_trace::__shown!(from),
        by = "window"; "imp.call"));
    value
}

/// `GetSystemMetrics`: a number from a number, no address.
#[must_use]
pub fn get_system_metrics(from: u32, index: u32) -> u32 {
    // SAFETY: one number, no address.
    let value = unsafe { GetSystemMetrics(index) };
    kept!(
        log::debug!(c = "USER32!GetSystemMetrics", a:% = Numbers(&[index]),
        v:% = oxide_trace::__shown!(value), from:% = oxide_trace::__shown!(from),
        by = "window"; "imp.call")
    );
    value
}

/// `WaitMessage`.
///
/// # Safety
/// Hold nothing a window's procedure touches: a message another thread
/// sends may be delivered while this waits.
#[must_use]
pub unsafe fn wait_message(from: u32) -> u32 {
    // SAFETY: takes no arguments; the caller holds nothing a procedure touches.
    let value = unsafe { WaitMessage() };
    kept!(
        log::debug!(c = "USER32!WaitMessage", v:% = oxide_trace::__shown!(value),
        from:% = oxide_trace::__shown!(from), by = "window"; "imp.call")
    );
    value
}

/// `SendMessageA`.
///
/// # Safety
/// `w` and `l` are what `message` asks for, and the window's procedure runs
/// inside this call, on whatever it holds.
#[must_use]
pub unsafe fn send_message(from: u32, window: u32, message: u32, w: u32, l: u32) -> u32 {
    // SAFETY: the caller's arguments, passed on whole.
    let value = unsafe { SendMessageA(window, message, w, l) };
    kept!(
        log::debug!(c = "USER32!SendMessageA", a:% = Numbers(&[window, message, w, l]),
        v:% = oxide_trace::__shown!(value), from:% = oxide_trace::__shown!(from),
        by = "window"; "imp.call")
    );
    value
}

/// `PostQuitMessage`: a number into the thread's own queue.
pub fn post_quit_message(from: u32, code: u32) {
    // SAFETY: one number, no address.
    unsafe { PostQuitMessage(code) };
    kept!(
        log::debug!(c = "USER32!PostQuitMessage", a:% = Numbers(&[code]),
        from:% = oxide_trace::__shown!(from), by = "window"; "imp.call")
    );
}

/// `GetMessageTime`: the thread's last message's time, no address.
#[must_use]
pub fn get_message_time(from: u32) -> u32 {
    // SAFETY: takes no arguments.
    let value = unsafe { GetMessageTime() };
    kept!(
        log::debug!(c = "USER32!GetMessageTime", v:% = oxide_trace::__shown!(value),
        from:% = oxide_trace::__shown!(from), by = "window"; "imp.call")
    );
    value
}
