// SPDX-License-Identifier: MIT
//! The 8 fronts: one file's keyboard and pointer calls over its program's
//! [`Desk`], kept beside its back by [`Input`], each answering in the
//! file's numbers with one `imp.call` line. A polled name pumps the back
//! first, so it reads the keys as the host left them.

use core::fmt;

use window::desk::Desk;
use window::{Bytes, Numbers, Rect, Screen, bits};

use crate::table::Row;
use crate::{Input, map};

/// `MapVirtualKeyA`'s type 2, `MAPVK_VK_TO_CHAR`: the one type answered.
const MAPVK_VK_TO_CHAR: u32 = 2;
/// `GetAsyncKeyState`'s bit for a key down now.
const ASYNC_DOWN: u32 = 0x8000;
/// `GetKeyState`'s bits for a key down, the `SHORT` -128; bit 0 its toggle.
const KEY_DOWN: u32 = 0xff80;

/// A cursor's name as `LoadCursorA` is handed it: a resource number below
/// `0x10000`, or a text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Name<'a> {
    Number(u32),
    Text(&'a [u8]),
}

/// As the layer's line shows it: a number `#` and the number.
impl fmt::Display for Name<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Name::Number(number) => {
                f.write_str("#")?;
                fmt::Display::fmt(&oxide_trace::__shown!(*number), f)
            }
            Name::Text(text) => fmt::Display::fmt(&Bytes(text), f),
        }
    }
}

/// One file's input calls: its row, and its program's desk.
pub struct Front<'d, S> {
    file: &'static Row,
    desk: &'d Desk<Input<S>>,
}

impl<'d, S: Screen> Front<'d, S> {
    pub const fn new(file: &'static Row, desk: &'d Desk<Input<S>>) -> Self {
        Self { file, desk }
    }

    /// Writes the call's one line: `a` and `o` only when the call has
    /// them, as the layer's lines show them.
    fn line(&self, call: &str, arguments: &[u32], wrote: &[u32], answer: u32) {
        let (a, o) = (Numbers(arguments), Numbers(wrote));
        let v = oxide_trace::__shown!(answer);
        self.desk
            .screen()
            .quiet(|| match (arguments.is_empty(), wrote.is_empty()) {
                (true, true) => log::debug!(c = call, v:% = v; "imp.call"),
                (false, true) => log::debug!(c = call, a:% = a, v:% = v; "imp.call"),
                (true, false) => log::debug!(c = call, o:% = o, v:% = v; "imp.call"),
                (false, false) => log::debug!(c = call, a:% = a, o:% = o, v:% = v; "imp.call"),
            });
    }

    /// `GetCursorPos`: 1, the point written: the host's pointer, or the
    /// kept point while locked. Callers: the shell, the game file.
    #[must_use]
    pub fn get_cursor_pos(&self) -> (i32, i32) {
        self.desk.pump();
        let point = self.desk.screen().keys().point;
        self.line(
            self.file.calls.get_cursor_pos,
            &[],
            &[bits(point.0), bits(point.1)],
            1,
        );
        point
    }

    /// `SetCursorPos`: 1, the point set to `x`, `y` held in the clip.
    /// Callers: the shell, the game file.
    #[must_use]
    pub fn set_cursor_pos(&self, x: i32, y: i32) -> u32 {
        let mut keys = self.desk.screen().keys_mut();
        keys.point = keys.held(x, y);
        drop(keys);
        self.line(self.file.calls.set_cursor_pos, &[bits(x), bits(y)], &[], 1);
        1
    }

    /// `ShowCursor`: the display count raised by 1 when `show` is not 0,
    /// else lowered by 1; the new count. Callers: the shell, the game file.
    #[must_use]
    pub fn show_cursor(&self, show: u32) -> i32 {
        let mut keys = self.desk.screen().keys_mut();
        keys.shown += if show == 0 { -1 } else { 1 };
        let shown = keys.shown;
        drop(keys);
        self.line(self.file.calls.show_cursor, &[show], &[], bits(shown));
        shown
    }

    /// `LoadCursorA`: a fresh handle for the cursor `name` of `instance`,
    /// never 0; the cursor itself is drawing's. Callers: the
    /// launcher.
    #[must_use]
    pub fn load_cursor(&self, instance: u32, name: Name<'_>) -> u32 {
        let handle = self.desk.state().handle();
        let (a, v) = (Numbers(&[instance]), oxide_trace::__shown!(handle));
        let call = self.file.calls.load_cursor;
        self.desk.screen().quiet(|| {
            log::debug!(c = call, a:% = a, s:% = name, v:% = v; "imp.call");
        });
        handle
    }

    /// `ClipCursor`: 1, the point held in `clip` from now on, the pointer
    /// locked; `None` lets both go. Callers: the shell, the game file.
    #[must_use]
    pub fn clip_cursor(&self, clip: Option<Rect>) -> u32 {
        let mut keys = self.desk.screen().keys_mut();
        keys.clip = clip;
        keys.locked = clip.is_some();
        keys.point = keys.held(keys.point.0, keys.point.1);
        drop(keys);
        let fields = clip.map(|clip| [clip.left, clip.top, clip.right, clip.bottom].map(bits));
        let handed: &[u32] = fields.as_ref().map_or(&[], |fields| &fields[..]);
        self.line(self.file.calls.clip_cursor, handed, &[], 1);
        1
    }

    /// `GetAsyncKeyState`: the `SHORT`'s 16 bits, `0x8000` while `key` is
    /// down now, else 0; a button is its key 1, 2 or 4. Callers: the shell,
    /// the game file.
    #[must_use]
    pub fn get_async_key_state(&self, key: u32) -> u32 {
        self.desk.pump();
        let down = usize::try_from(key)
            .ok()
            .and_then(|at| self.desk.screen().keys().down.get(at).copied())
            .unwrap_or(false);
        let answer = if down { ASYNC_DOWN } else { 0 };
        self.line(self.file.calls.get_async_key_state, &[key], &[], answer);
        answer
    }

    /// `GetKeyState`: the `SHORT`'s 16 bits, `0xff80` while `key` is down,
    /// bit 0 its toggle, as the last pump left the keys. Callers: the
    /// shell, the game file.
    #[must_use]
    pub fn get_key_state(&self, key: u32) -> u32 {
        let keys = self.desk.screen().keys();
        let (down, toggled) = usize::try_from(key)
            .ok()
            .filter(|at| *at < keys.down.len())
            .map_or((false, false), |at| (keys.down[at], keys.toggled[at]));
        drop(keys);
        let answer = if down { KEY_DOWN } else { 0 } | u32::from(toggled);
        self.line(self.file.calls.get_key_state, &[key], &[], answer);
        answer
    }

    /// `MapVirtualKeyA`: for type 2 the key map's character for `code`, 0
    /// for none; every other type 0, the named gap. Callers: the shell, the
    /// game file.
    #[must_use]
    pub fn map_virtual_key(&self, code: u32, kind: u32) -> u32 {
        let answer = match (u8::try_from(code), kind) {
            (Ok(key), MAPVK_VK_TO_CHAR) => u32::from(map::typed(key)),
            _ => 0,
        };
        self.line(self.file.calls.map_virtual_key, &[code, kind], &[], answer);
        answer
    }
}
