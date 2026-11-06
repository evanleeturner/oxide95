// SPDX-License-Identifier: MIT
//! The fixed ring two backs share, between the mix's writes and whoever plays them.
//! It holds about 370 ms at the mix's rate, the same as `waveout`'s 16 buffers, so a
//! program's stall of that long loses nothing and a longer one loses its newest frames.

use alloc::collections::VecDeque;

/// The most frames a ring holds: `waveout`'s 16 buffers of 1024.
pub const RING_FRAMES: usize = 16 * 1024;

/// Interleaved stereo `i16` frames, oldest first.
pub(crate) struct Ring {
    samples: VecDeque<i16>,
}

impl Ring {
    pub(crate) const fn new() -> Self {
        Self {
            samples: VecDeque::new(),
        }
    }

    pub(crate) fn room(&self) -> usize {
        RING_FRAMES - self.frames()
    }

    fn frames(&self) -> usize {
        self.samples.len() / 2
    }

    /// Takes whole frames up to its room; the answer is how many it took.
    pub(crate) fn push(&mut self, frames: &[i16]) -> usize {
        let taken = (frames.len() / 2).min(self.room());
        self.samples.extend(&frames[..taken * 2]);
        taken
    }

    /// Hands out whole frames, oldest first; the answer is how many it gave.
    pub(crate) fn pull(&mut self, into: &mut [i16]) -> usize {
        let given = (into.len() / 2).min(self.frames());
        for (slot, sample) in into.iter_mut().zip(self.samples.drain(..given * 2)) {
            *slot = sample;
        }
        given
    }

    /// Empties it; the answer is the frames that were never heard.
    pub(crate) fn clear(&mut self) -> usize {
        let lost = self.frames();
        self.samples.clear();
        lost
    }
}
