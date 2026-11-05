// SPDX-License-Identifier: MIT
//! The game clock: a count of microseconds, moved only by
//! its owner. The owner hands in the host's instant once a frame,
//! [`Clock::frame`]; the gap since the last one moves the clock, clamped to
//! the game's longest frame, so a stall (a page in the background, a
//! debugger's stop) comes back as one long frame, never a burst. A pause
//! holds it; a rebase skips the next gap, as after a load; a reset starts
//! it at 0. The game reads whole milliseconds, [`Clock::millis`], a 32-bit
//! count that wraps, which every reader reads by unsigned subtraction.

#[cfg(test)]
mod tests;

/// The game clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Clock {
    micros: u64,
    /// The last frame's host instant; none after a rebase or a reset.
    last: Option<u64>,
    /// The most one frame moves the clock: the game's number.
    clamp: u64,
    paused: bool,
}

impl Clock {
    /// A clock at `start` microseconds whose frames move it at most
    /// `clamp` microseconds each.
    #[must_use]
    pub const fn new(clamp: u64, start: u64) -> Self {
        Self {
            micros: start,
            last: None,
            clamp,
            paused: false,
        }
    }

    /// The owner's frame at the host's instant `host`, in microseconds:
    /// the gap since the last frame, clamped, moves the clock. The first
    /// frame, the frame after a rebase or a reset, a frame while paused and
    /// an instant before the last move nothing.
    pub fn frame(&mut self, host: u64) {
        if let (Some(last), false) = (self.last, self.paused) {
            let gap = host.saturating_sub(last).min(self.clamp);
            self.micros = self.micros.wrapping_add(gap);
        }
        self.last = Some(host);
    }

    /// Holds the clock: frames move nothing until [`Clock::resume`].
    pub fn pause(&mut self) {
        self.paused = true;
    }

    /// Lets frames move the clock again, from the last frame's instant.
    pub fn resume(&mut self) {
        self.paused = false;
    }

    /// Skips the next frame's gap.
    pub fn rebase(&mut self) {
        self.last = None;
    }

    /// Starts the clock at 0; the next frame's gap is skipped.
    pub fn reset(&mut self) {
        self.micros = 0;
        self.last = None;
    }

    #[must_use]
    pub const fn micros(&self) -> u64 {
        self.micros
    }

    /// The whole milliseconds, as a 32-bit count that wraps.
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        reason = "the count wraps at 32 bits, as Windows' does"
    )]
    pub const fn millis(&self) -> u32 {
        (self.micros / 1000) as u32
    }
}
