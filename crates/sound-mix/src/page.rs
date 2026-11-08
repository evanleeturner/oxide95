// SPDX-License-Identifier: MIT
//! The page's back: it keeps the mix in a fixed ring until the page's script reads it,
//! once an animation frame. A browser holds a page's sound until a person's key or
//! click, and the mix does not wait: before [`Page::start`] its room is 0, so voices
//! move and end by the clock, every frame is counted dropped, and nothing is kept that
//! could be replayed.
//!
//! [`Front::new`] owns its back, so the holder keeps a clone for the script: clones
//! share one ring. The script itself is `page/sound.mjs`.
//!
//! Call: `let page = Page::new(); let front = Front::new(&ROW, host, page.clone());`,
//! then [`Page::start`] at the gesture and [`Page::read`] once a frame.

use alloc::rc::Rc;
use core::cell::{Cell, RefCell};

use crate::output::Output;
use crate::ring::Ring;
use crate::{Front, Host};

struct Shared {
    started: Cell<bool>,
    rate_hz: Cell<u32>,
    /// Frames the ring has taken since the start, read or not.
    kept: Cell<u64>,
    ring: RefCell<Ring>,
}

/// The page's end of the seam, and the script's.
#[derive(Clone)]
pub struct Page {
    shared: Rc<Shared>,
}

impl Default for Page {
    fn default() -> Self {
        Self::new()
    }
}

impl Page {
    #[must_use]
    pub fn new() -> Self {
        let shared = Shared {
            started: Cell::new(false),
            rate_hz: Cell::new(0),
            kept: Cell::new(0),
            ring: RefCell::new(Ring::new()),
        };
        Self {
            shared: Rc::new(shared),
        }
    }

    /// The gesture came. It takes the front so the front is brought up to the clock
    /// first, at room 0: what the clock passed before this moment is dropped here, and
    /// every frame [`Page::read`] hands out was rendered after it. A second start only
    /// pumps, and a close does not undo it: a page's gesture lasts as long as the page.
    /// The front's back is any `O`, so this page may sit behind a witness; a front over
    /// another page is not caught, and starts this page while pumping that one.
    pub fn start<H: Host, O: Output>(&self, front: &Front<H, O>) {
        front.pump();
        if !self.shared.started.replace(true) {
            log::debug!(by = "sound-mix"; "sound.page.started");
        }
    }

    /// Whole frames into `into`, oldest first; the answer is how many.
    pub fn read(&self, into: &mut [i16]) -> usize {
        let given = self.shared.ring.borrow_mut().pull(into);
        if given > 0 {
            log::debug!(frames:% = oxide_trace::__shown!(given), by = "sound-mix"; "sound.page.read");
        }
        given
    }

    /// The rate the mix opened at; 0 before an open.
    #[must_use]
    pub fn rate_hz(&self) -> u32 {
        self.shared.rate_hz.get()
    }

    /// The frames the ring has taken since the start, read or not.
    #[must_use]
    pub fn kept(&self) -> u64 {
        self.shared.kept.get()
    }
}

impl Output for Page {
    fn open(&mut self, rate_hz: u32) -> u32 {
        self.shared.rate_hz.set(rate_hz);
        rate_hz
    }

    fn room(&self) -> usize {
        if self.shared.started.get() {
            self.shared.ring.borrow().room()
        } else {
            0
        }
    }

    fn write(&mut self, frames: &[i16]) {
        if !self.shared.started.get() {
            return;
        }
        let taken = self.shared.ring.borrow_mut().push(frames);
        self.shared.kept.set(self.shared.kept.get() + taken as u64);
    }

    fn close(&mut self) {
        let lost = self.shared.ring.borrow_mut().clear();
        log::debug!(frames:% = oxide_trace::__shown!(lost), by = "sound-mix"; "sound.page.closed");
    }
}
