// SPDX-License-Identifier: MIT
//! The page's half of playing a disc. A game runs in a Worker and the page's `<audio>`
//! is on the main thread, so a [`crate::play::Player`] here only posts numbers across;
//! `page/cd-player.mjs` reads them. The position stays the clock's, as everywhere.
use crate::play::Player;
use crate::table::Place;

/// `[kind, from's track, from's frame, to's track, to's frame]`, or for a volume
/// `[kind, left, right, 0, 0]`.
pub type Words = [u32; 5];

pub const PLAY: u32 = 1;
pub const PAUSE: u32 = 2;
pub const STOP: u32 = 3;
pub const VOLUME: u32 = 4;

/// Posts each order through the call handed in.
pub struct PagePlayer<F>(pub F);

impl<F: Fn(&Words)> Player for PagePlayer<F> {
    fn play(&self, from: Place, to: Place) {
        (self.0)(&[
            PLAY,
            u32::from(from.track),
            from.frame,
            u32::from(to.track),
            to.frame,
        ]);
    }

    fn pause(&self) {
        (self.0)(&[PAUSE, 0, 0, 0, 0]);
    }

    fn stop(&self) {
        (self.0)(&[STOP, 0, 0, 0, 0]);
    }

    fn volume(&self, left: u16, right: u16) {
        (self.0)(&[VOLUME, u32::from(left), u32::from(right), 0, 0]);
    }
}
