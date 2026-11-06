// SPDX-License-Identifier: MIT
//! The real back of [`SoundDoor`]: a clip's stream through a door of
//! `sound-mix`, a voice whose life is its own, so closing Miles' driver leaves
//! the clip sounding.
use sound_mix::output::Output;
use sound_mix::{Front, Host};

use crate::{SoundDoor, Track};

/// A clip's way into one mixer.
pub struct MixDoor<'a, H, O>(&'a Front<H, O>);

impl<'a, H, O> MixDoor<'a, H, O> {
    /// The door into `front`.
    #[must_use]
    pub const fn new(front: &'a Front<H, O>) -> Self {
        Self(front)
    }
}

impl<H: Host, O: Output> SoundDoor for MixDoor<'_, H, O> {
    fn plays(&self) -> bool {
        true
    }

    fn stream_open(&mut self, track: Track) -> u32 {
        self.0.stream_open(track.rate_hz, u32::from(track.channels))
    }

    fn stream_push(&mut self, stream: u32, samples: &[i16]) {
        self.0.stream_push(stream, samples);
    }

    fn stream_close(&mut self, stream: u32) {
        self.0.stream_close(stream);
    }
}
