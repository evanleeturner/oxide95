// SPDX-License-Identifier: MIT
//! The direct back, on the Win32 target alone: each of the 8 fronts asks the
//! real `USER32` and writes one `imp.call` line, `by=window-input`, with
//! `window::user32`'s readers and for that module's reason: the windows and
//! the queue are Wine's, so the keys and the pointer are too. No [`crate::Keys`]
//! here, and no desk.
//!
//! Said plainly: on this target the answer is the original's call, written
//! in this crate. `by=window-input` says who wrote the call. It does not say
//! an input stack was rebuilt: [`crate::Keys`] and [`crate::front`] are that,
//! on the page's and the host's targets.
//!
//! A front takes the numbers its caller handed in as `u32`, addresses
//! included. `from` is the address the call returns to, shown on the line and
//! never used. `c` spells the import as the layer's rows do.
//!
//! `GetKeyState` and `GetAsyncKeyState` answer a `SHORT`: `eax` goes back
//! whole, as the call left it, and `v` shows its low 16 bits, the value the
//! caller reads.
#![allow(
    unsafe_code,
    reason = "this back is calls into USER32 and nothing else"
)]

use window::Numbers;
use window::user32::{kept, name, record, wrote};

unsafe extern "system" {
    fn GetCursorPos(point: u32) -> u32;
    fn SetCursorPos(x: u32, y: u32) -> u32;
    fn ShowCursor(show: u32) -> u32;
    fn LoadCursorA(instance: u32, cursor: u32) -> u32;
    fn ClipCursor(rectangle: u32) -> u32;
    fn GetAsyncKeyState(key: u32) -> u32;
    fn GetKeyState(key: u32) -> u32;
    fn MapVirtualKeyA(code: u32, kind: u32) -> u32;
}

/// `GetCursorPos`: `o` the point it wrote, none when it answered 0.
///
/// # Safety
/// `point` is a `POINT`, as `GetCursorPos` takes it.
#[must_use]
pub unsafe fn get_cursor_pos(from: u32, point: u32) -> u32 {
    // SAFETY: the caller's argument, passed on whole.
    let value = unsafe { GetCursorPos(point) };
    // SAFETY: the caller's point.
    let got = unsafe { wrote::<2>(value, point) };
    kept(|| {
        log::debug!(c = "USER32!GetCursorPos", o:% = got,
            v:% = oxide_trace::__shown!(value), from:% = oxide_trace::__shown!(from),
            by = "window-input"; "imp.call");
    });
    value
}

/// `SetCursorPos`.
#[must_use]
pub fn set_cursor_pos(from: u32, x: u32, y: u32) -> u32 {
    // SAFETY: two numbers, no address.
    let value = unsafe { SetCursorPos(x, y) };
    kept(|| {
        log::debug!(c = "USER32!SetCursorPos", a:% = Numbers(&[x, y]),
            v:% = oxide_trace::__shown!(value), from:% = oxide_trace::__shown!(from),
            by = "window-input"; "imp.call");
    });
    value
}

/// `ShowCursor`: `v` the display count it left.
#[must_use]
pub fn show_cursor(from: u32, show: u32) -> u32 {
    // SAFETY: a number, no address.
    let value = unsafe { ShowCursor(show) };
    kept(|| {
        log::debug!(c = "USER32!ShowCursor", a:% = Numbers(&[show]),
            v:% = oxide_trace::__shown!(value), from:% = oxide_trace::__shown!(from),
            by = "window-input"; "imp.call");
    });
    value
}

/// `LoadCursorA`: `a` the instance, `s` the cursor's name, a resource id
/// shown `#` and the number.
///
/// # Safety
/// `cursor` is a resource id or a NUL-ended text, as `LoadCursorA` takes it.
#[must_use]
pub unsafe fn load_cursor(from: u32, instance: u32, cursor: u32) -> u32 {
    // SAFETY: the caller's arguments, passed on whole.
    let value = unsafe { LoadCursorA(instance, cursor) };
    // SAFETY: the caller's cursor name, a resource id or a text.
    let cursor = unsafe { name(cursor) };
    kept(|| {
        log::debug!(c = "USER32!LoadCursorA", a:% = Numbers(&[instance]), s:% = cursor,
            v:% = oxide_trace::__shown!(value), from:% = oxide_trace::__shown!(from),
            by = "window-input"; "imp.call");
    });
    value
}

/// `ClipCursor`: `a` the rectangle's 4 fields as handed, none for a null
/// one, the clip let go.
///
/// # Safety
/// `rectangle` is 0 or a `RECT`, as `ClipCursor` takes it.
#[must_use]
pub unsafe fn clip_cursor(from: u32, rectangle: u32) -> u32 {
    // SAFETY: the caller's rectangle, read before the call.
    let fields = unsafe { record::<4>(rectangle) };
    // SAFETY: the caller's argument, passed on whole.
    let value = unsafe { ClipCursor(rectangle) };
    let handed: &[u32] = if rectangle == 0 { &[] } else { &fields };
    kept(|| {
        log::debug!(c = "USER32!ClipCursor", a:% = Numbers(handed),
            v:% = oxide_trace::__shown!(value), from:% = oxide_trace::__shown!(from),
            by = "window-input"; "imp.call");
    });
    value
}

/// `GetAsyncKeyState`: `v` the `SHORT`'s 16 bits; `eax` goes back whole.
#[must_use]
pub fn get_async_key_state(from: u32, key: u32) -> u32 {
    // SAFETY: a number, no address.
    let value = unsafe { GetAsyncKeyState(key) };
    let short = value & 0xffff;
    kept(|| {
        log::debug!(c = "USER32!GetAsyncKeyState", a:% = Numbers(&[key]),
            v:% = oxide_trace::__shown!(short), from:% = oxide_trace::__shown!(from),
            by = "window-input"; "imp.call");
    });
    value
}

/// `GetKeyState`: as [`get_async_key_state`].
#[must_use]
pub fn get_key_state(from: u32, key: u32) -> u32 {
    // SAFETY: a number, no address.
    let value = unsafe { GetKeyState(key) };
    let short = value & 0xffff;
    kept(|| {
        log::debug!(c = "USER32!GetKeyState", a:% = Numbers(&[key]),
            v:% = oxide_trace::__shown!(short), from:% = oxide_trace::__shown!(from),
            by = "window-input"; "imp.call");
    });
    value
}

/// `MapVirtualKeyA`.
#[must_use]
pub fn map_virtual_key(from: u32, code: u32, kind: u32) -> u32 {
    // SAFETY: two numbers, no address.
    let value = unsafe { MapVirtualKeyA(code, kind) };
    kept(|| {
        log::debug!(c = "USER32!MapVirtualKeyA", a:% = Numbers(&[code, kind]),
            v:% = oxide_trace::__shown!(value), from:% = oxide_trace::__shown!(from),
            by = "window-input"; "imp.call");
    });
    value
}
