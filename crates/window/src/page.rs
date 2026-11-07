// SPDX-License-Identifier: MIT
//! The page's back: everything handed in. The page's code makes it with
//! the canvas's size, hands in each event the canvas raised and sets the
//! time once a frame; it reads each window's place and shown back to lay
//! its canvas out. Nothing here reaches the browser, and nothing here
//! blocks: whatever turns the program's loop, the script once a frame or a
//! Worker waiting between turns, turns it on the one thread that holds this
//! back. The host tests use it as their back.
use alloc::vec::Vec;
use core::cell::{Cell, RefCell};

use crate::{Event, Rect, Screen};

/// One window as the program last placed it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placed {
    pub window: u32,
    pub rect: Rect,
    pub shown: bool,
}

/// The back a page's script feeds.
pub struct Page {
    size: (i32, i32),
    time: Cell<u32>,
    events: RefCell<Vec<Event>>,
    placed: RefCell<Vec<Placed>>,
}

impl Page {
    /// The canvas `width` by `height` pixels, the screen's size the
    /// program is answered; the time 0.
    #[must_use]
    pub const fn new(width: i32, height: i32) -> Self {
        Self {
            size: (width, height),
            time: Cell::new(0),
            events: RefCell::new(Vec::new()),
            placed: RefCell::new(Vec::new()),
        }
    }

    /// The canvas raised `event`: queued for the program's next pump.
    pub fn hand(&self, event: Event) {
        self.events.borrow_mut().push(event);
    }

    /// The frame's time, milliseconds since the page began.
    pub fn frame(&self, millis: u32) {
        self.time.set(millis);
    }

    /// `window` as the program last placed it; `None` for one never
    /// placed or gone.
    #[must_use]
    pub fn placed(&self, window: u32) -> Option<Placed> {
        self.placed
            .borrow()
            .iter()
            .find(|placed| placed.window == window)
            .copied()
    }
}

impl Screen for Page {
    fn size(&self) -> (i32, i32) {
        self.size
    }

    fn place(&self, window: u32, rect: Rect, shown: bool) {
        let mut placed = self.placed.borrow_mut();
        let now = Placed {
            window,
            rect,
            shown,
        };
        match placed.iter_mut().find(|placed| placed.window == window) {
            Some(was) => *was = now,
            None => placed.push(now),
        }
    }

    fn gone(&self, window: u32) {
        self.placed
            .borrow_mut()
            .retain(|placed| placed.window != window);
    }

    /// The events handed in since the last pump, oldest first, taken out
    /// before any is handed on.
    fn pump(&self, into: &mut dyn FnMut(Event)) {
        let events = core::mem::take(&mut *self.events.borrow_mut());
        events.into_iter().for_each(into);
    }

    fn now(&self) -> u32 {
        self.time.get()
    }
}
