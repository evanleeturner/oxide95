// SPDX-License-Identifier: MIT
//! The page's clocks: everything handed in. The frame loop moves the game
//! clock once a frame from the browser's own instant, and sets the wall
//! clock from `Date`. A code that spins on the clock inside one frame would
//! never see it move, so [`Page::reading`] also takes a function to call at
//! each read of the game clock, as a host's back frames at each read. The
//! function is handed in: nothing here names a target's clock, and nothing
//! blocks: `Sleep` returns at once, the wait being the frame loop's yield
//! to the browser. [`Page::new`] reads nothing: the host tests' fixed clock.
use crate::Clocks;
use crate::clock::Clock;
use core::cell::Cell;

/// The clocks a page's owner feeds.
pub struct Page {
    clock: Cell<Clock>,
    wall: Cell<u64>,
    /// The host's instant, called at each read of the game clock; none on a
    /// clock only the owner's frames move.
    read: Option<fn() -> u64>,
}

impl Page {
    /// The game clock `clock`, and the wall at file-time `wall`.
    #[must_use]
    pub const fn new(clock: Clock, wall: u64) -> Self {
        Self {
            clock: Cell::new(clock),
            wall: Cell::new(wall),
            read: None,
        }
    }

    /// As [`Page::new`], with `read` handing in the host's instant, in
    /// microseconds, at each read of the game clock: each read is a frame.
    #[must_use]
    pub const fn reading(clock: Clock, wall: u64, read: fn() -> u64) -> Self {
        Self {
            clock: Cell::new(clock),
            wall: Cell::new(wall),
            read: Some(read),
        }
    }

    fn moved(&self, step: impl FnOnce(&mut Clock)) {
        let mut clock = self.clock.get();
        step(&mut clock);
        self.clock.set(clock);
    }

    /// The owner's frame at the host's instant `host`, in microseconds.
    pub fn frame(&self, host: u64) {
        self.moved(|clock| clock.frame(host));
    }

    pub fn pause(&self) {
        self.moved(Clock::pause);
    }

    pub fn resume(&self) {
        self.moved(Clock::resume);
    }

    pub fn rebase(&self) {
        self.moved(Clock::rebase);
    }

    pub fn reset(&self) {
        self.moved(Clock::reset);
    }

    /// The wall clock, file-time ticks, UTC.
    pub fn set_wall(&self, ticks: u64) {
        self.wall.set(ticks);
    }

    #[must_use]
    pub fn clock(&self) -> Clock {
        self.clock.get()
    }

    /// The calling thread's last error, the slot every back shares.
    #[must_use]
    pub fn last_error(&self) -> u32 {
        kernel_last_error::get()
    }
}

impl Clocks for Page {
    fn micros(&self) -> u64 {
        if let Some(read) = self.read {
            self.frame(read());
        }
        self.clock.get().micros()
    }

    fn wall(&self) -> u64 {
        self.wall.get()
    }

    fn sleep(&self, _millis: u32) {}

    fn fail(&self, code: u32) {
        kernel_last_error::set(code);
    }
}
