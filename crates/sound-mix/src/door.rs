// SPDX-License-Identifier: MIT
//! A voice a program's own code opens, not one Miles' calls made: `i16` sound
//! handed in and drained as it mixes. Its life is its own, so closing Miles'
//! driver leaves it sounding.
#![allow(clippy::cast_possible_truncation)]

use alloc::collections::VecDeque;

use crate::voice::{FRACTION, MIX_HZ};

pub(crate) struct Door {
    pub(crate) id: u32,
    rate_hz: u32,
    channels: usize,
    waiting: VecDeque<i16>,
    /// Frames past `waiting`'s front, in `FRACTION` fixed point.
    at: u64,
}

impl Door {
    pub(crate) fn new(id: u32, rate_hz: u32, channels: u32) -> Self {
        Self {
            id,
            rate_hz,
            channels: channels as usize,
            waiting: VecDeque::new(),
            at: 0,
        }
    }

    pub(crate) fn push(&mut self, samples: &[i16]) {
        self.waiting.extend(samples.iter().copied());
    }

    pub(crate) fn sounding(&self) -> bool {
        self.waiting.len() >= self.channels
    }

    /// Adds this voice to `out`, stereo pairs, and lets go of what it has passed.
    pub(crate) fn render(&mut self, out: &mut [i32]) {
        let step = (u64::from(self.rate_hz) << FRACTION) / MIX_HZ;
        for pair in out.chunks_exact_mut(2) {
            let first = (self.at >> FRACTION) as usize * self.channels;
            if first + self.channels > self.waiting.len() {
                break;
            }
            let left = i32::from(self.waiting[first]);
            let right = if self.channels > 1 {
                i32::from(self.waiting[first + 1])
            } else {
                left
            };
            pair[0] += left;
            pair[1] += right;
            self.at += step;
        }
        let whole = (self.at >> FRACTION) as usize;
        let passed = (whole * self.channels).min(self.waiting.len());
        self.waiting.drain(..passed);
        self.at -= (whole as u64) << FRACTION;
    }
}
