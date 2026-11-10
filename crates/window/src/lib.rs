// SPDX-License-Identifier: MIT
//! The window stand-in: the USER32 names a
//! program asks of its windows and its messages, written once. Windows'
//! window manager is never rebuilt: a front answers what the program reads
//! back of a call, its return and what it wrote, and nothing more.
//!
//! The window table, the class table and the message queue are every
//! target's, in [`desk::Desk`]. What a host's window does is the back's,
//! behind [`Screen`]: the screen's size, a window shown or placed, and the
//! events the host's window raised, handed in as [`Event`]s, never a
//! Windows type. The backs: a page's, `page::Page`; a host's, through
//! winit, `native::Native`. On the Win32 target, `user32`: no desk at all,
//! each front the real `USER32`'s call, its windows and queue Wine's own.
//!
//! The fit a picture is shown at on a larger surface, [`Fit`], is this
//! crate's: the drawing crate shows its picture by it, and a host's back
//! places the pointer by the same rule, through [`PointerFit`].
//!
//! The 24 fronts, in [`front::Front`]: windows `RegisterClassA`,
//! `CreateWindowExA`, `DestroyWindow`, `FindWindowA`, `LoadIconA`,
//! `ShowWindow`, `SetWindowPos`, `AdjustWindowRect`, `GetWindowRect`,
//! `GetWindowLongA`, `SetWindowLongA`, `ClientToScreen`,
//! `ScreenToClient`, `SetFocus` and `GetSystemMetrics`; messages
//! `DefWindowProcA`, `PeekMessageA`, `WaitMessage`, `TranslateMessage`,
//! `DispatchMessageA`, `SendMessageA`, `PostMessageA`, `PostQuitMessage`
//! and `GetMessageTime`. Each front's doc names its callers by file: the
//! shell, the game file, the launcher; a game's rows crate names the
//! files.
//!
//! Six names a game imports that no row serves yet have no front, as the
//! registry's `RegCreateKeyA`: each comes with its first measured caller.
//! `GetForegroundWindow`, `GetMessageA`, `SetForegroundWindow`,
//! `SetRect`, `SetWindowTextA` and `ShowWindowAsync`. `GetMessageA`
//! blocks until a message comes, which a page's own thread cannot do; a
//! Worker can wait between turns, as `web-shell` shows, so where its loop
//! turns is named when its caller is cut.
//!
//! Each call writes one `imp.call` line: `c` the row's name, `a` the
//! call's numbers, `s` the name or title it was given, `o` what it wrote,
//! `v` its answer.
//!
//! The walls, named: the stand-in draws no frame, yet a window's client
//! origin stands past the frame and caption its style names, by the row's
//! metrics, a menu's bar not counted as a window keeps no menu; no message
//! is sent while a window is made,
//! placed or destroyed, the program reading none back; z-order and the
//! owner chain are not kept; the key-to-character map is `window-input`'s,
//! handed to `TranslateMessage` by [`Screen::character`], so a
//! back with none posts no character; a key message's scan code is
//! [`Screen::scan`]'s and every message's `x`, `y` are [`Screen::point`]'s,
//! each 0 from a back with no keys kept; a command's `l` is 0, as
//! [`Event::Command`] says; icons
//! and drawing are another crate's, so an icon is a handle and nothing more;
//! a shown window's `WM_PAINT` waits on every peek until
//! [`desk::Desk::mark_painted`] ends it, as the drawing crate's paint calls and
//! `DefWindowProcA` do.
//!
//! The crate names no game and nothing of any one target, so it builds for
//! `wasm32-unknown-unknown`; a game's facts live in its rows crate.
#![cfg_attr(not(any(test, feature = "native")), no_std)]

#[cfg(any(test, feature = "desk"))]
extern crate alloc;

#[cfg(any(test, feature = "desk"))]
pub mod desk;
mod fit;
#[cfg(any(test, feature = "desk"))]
pub mod front;
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub mod native;
#[cfg(any(test, feature = "page"))]
pub mod page;
pub mod table;
#[cfg(test)]
mod test_log;
#[cfg(test)]
mod tests;
#[cfg(all(feature = "user32", target_arch = "x86", target_os = "windows"))]
pub mod user32;

use core::fmt;

pub use fit::{Fit, PointerFit};

/// A rectangle in screen pixels, Windows' `RECT`: right and bottom one
/// past the last pixel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    /// The rectangle at `left`, `top`, `width` wide and `height` high.
    #[must_use]
    pub const fn sized(left: i32, top: i32, width: i32, height: i32) -> Self {
        Self {
            left,
            top,
            right: left + width,
            bottom: top + height,
        }
    }
}

/// One queued message, the 7 fields `PeekMessageA` writes: the window
/// (0 a message to the program, not a window), its number and two words,
/// the time it was queued and where the pointer was.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Message {
    pub window: u32,
    pub message: u32,
    pub w: u32,
    pub l: u32,
    pub time: u32,
    pub x: i32,
    pub y: i32,
}

impl Message {
    /// The fields as the line shows them, in the record's order.
    #[must_use]
    pub const fn fields(&self) -> [u32; 7] {
        [
            self.window,
            self.message,
            self.w,
            self.l,
            self.time,
            bits(self.x),
            bits(self.y),
        ]
    }
}

/// What a host's window did, as a back hands it in: the seam's own words,
/// never a host's or Windows' type. A key is its virtual key number; what
/// the input events leave readable is `window-input`'s.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// The host asked `window` closed: the program's `WM_CLOSE`.
    Close { window: u32 },
    /// The host's window took or lost the user's attention: the program's
    /// `WM_ACTIVATEAPP`.
    Active { window: u32, active: bool },
    /// A key went down or up in `window`, `key` its virtual key number: the
    /// program's `WM_KEYDOWN` or `WM_KEYUP`, or their `WM_SYS` forms while
    /// Alt is down. A held key's repeat is another down.
    Key { window: u32, key: u8, down: bool },
    /// A menu item or a box's button was chosen in `window`, `id` its number:
    /// the program's `WM_COMMAND`, `id` the low word, the high word 0, `l` 0.
    /// A real button's `l` is its handle: no measured procedure reads it, and
    /// a menu item's is 0. It writes no call's line: the program made no call.
    Command { window: u32, id: u16 },
    /// A pointer button went down or up in `window`, `key` its virtual key
    /// number (1 left, 2 right, 4 middle). It queues nothing: the
    /// program's pumps ask for key messages alone, and poll the buttons.
    Button { window: u32, key: u8, down: bool },
    /// The pointer stands at `x`, `y` on the screen, over `window`. It
    /// queues nothing.
    Pointer { window: u32, x: i32, y: i32 },
    /// The pointer moved by `dx`, `dy`, wherever it stands: what a locked
    /// pointer raises. It queues nothing.
    Motion { dx: i32, dy: i32 },
}

/// Where one program's windows are shown: the seam between the calls and
/// a target. A window is named by the handle the desk gave it.
pub trait Screen {
    /// The screen's width and height in pixels.
    fn size(&self) -> (i32, i32);
    /// Shows `window` at `rect` when `shown`, else hides it.
    fn place(&self, window: u32, rect: Rect, shown: bool);
    /// `window` is gone: the back lets its host's window go.
    fn gone(&self, window: u32);
    /// Hands each event the host raised since the last pump to `into`,
    /// never waiting for one.
    fn pump(&self, into: &mut dyn FnMut(Event));
    /// Milliseconds since the back began, a message's time.
    fn now(&self) -> u32;
    /// Writes one line around whatever the target must keep across it.
    fn quiet(&self, line: impl FnOnce()) {
        line();
    }
    /// The character the key `key` types as the keys stand now,
    /// `TranslateMessage`'s; 0 for none. A back with no key map types none.
    fn character(&self, key: u32) -> u32 {
        let _ = key;
        0
    }
    /// The scan code of the key `key`, as bits 16 to 24 of its message's
    /// `l` hold it; 0 for none. A back with no key map has none.
    fn scan(&self, key: u32) -> u32 {
        let _ = key;
        0
    }
    /// The pointer's point on the screen, which a message carries from the
    /// moment it is queued; the origin from a back that keeps none.
    fn point(&self) -> (i32, i32) {
        (0, 0)
    }
}

/// A borrowed back: each of a program's files keeps its own front and all
/// share the one back.
impl<S: Screen> Screen for &S {
    fn size(&self) -> (i32, i32) {
        (**self).size()
    }

    fn place(&self, window: u32, rect: Rect, shown: bool) {
        (**self).place(window, rect, shown);
    }

    fn gone(&self, window: u32) {
        (**self).gone(window);
    }

    fn pump(&self, into: &mut dyn FnMut(Event)) {
        (**self).pump(into);
    }

    fn now(&self) -> u32 {
        (**self).now()
    }

    fn quiet(&self, line: impl FnOnce()) {
        (**self).quiet(line);
    }

    fn character(&self, key: u32) -> u32 {
        (**self).character(key)
    }

    fn scan(&self, key: u32) -> u32 {
        (**self).scan(key)
    }

    fn point(&self) -> (i32, i32) {
        (**self).point()
    }
}

/// A signed number's bits as the file reads them, two's complement.
#[must_use]
pub const fn bits(value: i32) -> u32 {
    u32::from_ne_bytes(value.to_ne_bytes())
}

/// Bytes as the line shows them: a printable byte as its character, any
/// other `\xNN`.
pub struct Bytes<'a>(pub &'a [u8]);

impl fmt::Display for Bytes<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|byte| match byte {
            b' '..=b'~' => fmt::Write::write_char(f, char::from(*byte)),
            _ => write!(f, "\\x{byte:02x}"),
        })
    }
}

/// Numbers as the line shows them, comma-joined: empty for none.
pub struct Numbers<'a>(pub &'a [u32]);

impl fmt::Display for Numbers<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (at, number) in self.0.iter().enumerate() {
            if at > 0 {
                f.write_str(",")?;
            }
            write!(f, "{}", oxide_trace::__shown!(*number))?;
        }
        Ok(())
    }
}
