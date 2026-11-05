// SPDX-License-Identifier: MIT
//! The window stand-in's input: the USER32 names a
//! program asks of its keyboard and its pointer, written once over the
//! `window` crate's desk. Windows' input stack is never rebuilt: a front answers what
//! the program reads back of a call, its return and what it wrote, and
//! nothing more.
//!
//! The portable part every target shares is [`Keys`]: the 256 keys down
//! and toggled (the pointer's 3 buttons at 1, 2 and 4), the pointer's
//! point, its clip, the shown count and the lock. [`Input`] keeps it beside
//! a back and is itself a [`Screen`]: each event the back raised is applied
//! to the keys before the desk queues it. Events in, the keys out.
//!
//! The 8 fronts, in [`front::Front`]: `GetCursorPos`, `SetCursorPos`,
//! `ShowCursor`, `LoadCursorA`, `ClipCursor`, `GetAsyncKeyState`,
//! `GetKeyState` and `MapVirtualKeyA`. Each front's doc names its callers
//! by file: the shell, the game file, the launcher; a game's rows crate
//! names the files. The key map, [`map`], answers `MapVirtualKeyA`'s
//! character, and `TranslateMessage`'s through [`Screen::character`].
//! The remap rows, [`remap`], name each chord a browser keeps from a page
//! and the chord the page reads in its place.
//!
//! Names a game imports that no row serves yet have no front, as 6 names in
//! `window`: each comes with its first measured caller. USER32's
//! `GetKeyboardState`, `ReleaseCapture`, `SetCapture` and `SetCursor`;
//! DINPUT's `DirectInputCreateA` and the 7 methods of its interface and
//! device: `CreateDevice`, `SetProperty`, `Acquire`, `GetDeviceState`,
//! `GetDeviceData`, `SetDataFormat` and `SetCooperativeLevel`, answered
//! from the same keys when their caller is cut.
//!
//! Each call writes one `imp.call` line, as the layer's without the
//! caller: `c` the row's name, `a` the call's numbers, `s` the name it was
//! given, `o` what it wrote, `v` its answer; a key state's `v` its 16 bits.
//!
//! The walls, named: the virtual cursor lives here, so under a lock the
//! host's pointer is the back's to hide and hold, and unlocked
//! `SetCursorPos` moves the kept point until the host's pointer moves
//! again; `GetKeyState` reads the keys as the last pump left them, not as
//! of the last message taken; a cursor is a handle and nothing more,
//! drawing being another crate's.
//!
//! The crate names no game and nothing of any one target, so it builds for
//! `wasm32-unknown-unknown`; a game's facts live in its rows crate.
#![cfg_attr(not(test), no_std)]

#[cfg(any(test, feature = "desk"))]
pub mod front;
pub mod map;
pub mod remap;
pub mod table;
#[cfg(test)]
mod test_log;
#[cfg(test)]
mod tests;

use core::cell::{Ref, RefCell, RefMut};

use window::{Event, Rect, Screen};

/// `VK_SHIFT`: while it is down a letter types its capital.
const VK_SHIFT: usize = 0x10;

/// The keys and the pointer as the events left them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Keys {
    /// Each virtual key down now, the pointer's buttons at 1, 2 and 4.
    pub down: [bool; 256],
    /// Each key's toggle: flipped at each press, held while up.
    pub toggled: [bool; 256],
    /// The pointer's point on the screen.
    pub point: (i32, i32),
    /// The rectangle the point is held in, right and bottom one past the
    /// last point; `None` for the whole screen.
    pub clip: Option<Rect>,
    /// `ShowCursor`'s display count: the pointer drawn while 0 or more.
    pub shown: i32,
    /// The pointer locked: the point kept here and moved by motion alone,
    /// the host's pointer the back's to hide and hold.
    pub locked: bool,
    /// The screen's width and height, which hold the point with no clip.
    pub screen: (i32, i32),
}

impl Keys {
    /// No key down or toggled, the point at the origin, no clip, the
    /// count 0, unlocked, on a screen `size` pixels.
    #[must_use]
    pub const fn new(size: (i32, i32)) -> Self {
        Self {
            down: [false; 256],
            toggled: [false; 256],
            point: (0, 0),
            clip: None,
            shown: 0,
            locked: false,
            screen: size,
        }
    }

    /// `event` applied: a key or a button pressed or let go; every key let
    /// go when the host's window loses the user's attention, the toggles
    /// kept; the host's pointer followed while unlocked, its motion while
    /// locked, the point held in the clip either way.
    pub fn apply(&mut self, event: Event) {
        match event {
            Event::Key { key, down, .. } | Event::Button { key, down, .. } => {
                let at = usize::from(key);
                if down && !self.down[at] {
                    self.toggled[at] = !self.toggled[at];
                }
                self.down[at] = down;
            }
            Event::Active { active: false, .. } => self.down = [false; 256],
            Event::Pointer { x, y, .. } if !self.locked => self.point = self.held(x, y),
            Event::Motion { dx, dy } if self.locked => {
                let (x, y) = self.point;
                self.point = self.held(x.saturating_add(dx), y.saturating_add(dy));
            }
            _ => {}
        }
    }

    /// The point `x`, `y` held in the clip, or in the screen with none.
    #[must_use]
    pub fn held(&self, x: i32, y: i32) -> (i32, i32) {
        let (width, height) = self.screen;
        let bound = self.clip.unwrap_or(Rect::sized(0, 0, width, height));
        (
            x.clamp(bound.left, (bound.right - 1).max(bound.left)),
            y.clamp(bound.top, (bound.bottom - 1).max(bound.top)),
        )
    }
}

/// A back with the keys kept beside it: itself a [`Screen`], its pump the
/// back's with each event applied to the keys before the desk takes it.
/// A program's desk is made over it: `Desk::new(Input::new(back))`.
pub struct Input<S> {
    back: S,
    keys: RefCell<Keys>,
}

impl<S: Screen> Input<S> {
    /// `back` with the keys fresh, on its screen's size.
    pub fn new(back: S) -> Self {
        let keys = Keys::new(back.size());
        Self {
            back,
            keys: RefCell::new(keys),
        }
    }

    /// The back, its script's to feed.
    pub const fn back(&self) -> &S {
        &self.back
    }

    /// The keys, to read: a back's script reads the lock and the shown
    /// count here.
    pub fn keys(&self) -> Ref<'_, Keys> {
        self.keys.borrow()
    }

    /// The keys, to change; never held across a line.
    pub fn keys_mut(&self) -> RefMut<'_, Keys> {
        self.keys.borrow_mut()
    }
}

impl<S: Screen> Screen for Input<S> {
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
        self.back.pump(&mut |event| {
            self.keys.borrow_mut().apply(event);
            into(event);
        });
    }

    fn now(&self) -> u32 {
        self.back.now()
    }

    fn quiet(&self, line: impl FnOnce()) {
        self.back.quiet(line);
    }

    /// The key map's character for `key`, Shift as the keys stand.
    fn character(&self, key: u32) -> u32 {
        let shift = self.keys.borrow().down[VK_SHIFT];
        u8::try_from(key).map_or(0, |key| map::character(key, shift))
    }
}
