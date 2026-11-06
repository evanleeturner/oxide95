// SPDX-License-Identifier: MIT
//! The real [`Back`]: a buffer's frames through doors of `sound-mix`. The mixer
//! shows no clock of its own, so the clock is a function handed in, the same one
//! its host reads. A target that must keep something across a log line writes its
//! own [`Back`].
use sound_mix::output::Output;
use sound_mix::{Front, Host};

use crate::buffers::Back;

/// The way into one mixer, and its clock.
pub struct MixBack<'a, H, O, C> {
    front: &'a Front<H, O>,
    clock: C,
}

impl<'a, H, O, C> MixBack<'a, H, O, C> {
    /// The way into `front`, its clock read by `clock`.
    #[must_use]
    pub const fn new(front: &'a Front<H, O>, clock: C) -> Self {
        Self { front, clock }
    }
}

impl<H: Host, O: Output, C: Fn() -> u64> Back for MixBack<'_, H, O, C> {
    fn now_us(&self) -> u64 {
        (self.clock)()
    }

    fn stream_open(&self, rate_hz: u32, channels: u32) -> u32 {
        self.front.stream_open(rate_hz, channels)
    }

    fn stream_push(&self, door: u32, samples: &[i16]) {
        self.front.stream_push(door, samples);
    }

    fn stream_close(&self, door: u32) {
        self.front.stream_close(door);
    }
}
