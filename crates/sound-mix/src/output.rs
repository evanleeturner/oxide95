// SPDX-License-Identifier: MIT
//! The one seam a device's back fills. The mix's clock never comes from here:
//! a sample still ends with this back silent, refusing, or gone.

/// Where the mix goes, in interleaved stereo `i16` frames.
pub trait Output {
    /// Opens at `rate_hz`; the answer is the rate it took, 0 if it could not.
    fn open(&mut self, rate_hz: u32) -> u32;
    /// How many frames it would take now.
    fn room(&self) -> usize;
    /// Takes frames, 2 numbers each, never more than [`Output::room`] said.
    fn write(&mut self, frames: &[i16]);
    fn close(&mut self);
}

/// Takes everything and plays nothing: the back of every target until a device's.
#[derive(Default)]
pub struct NullBack;

impl Output for NullBack {
    fn open(&mut self, rate_hz: u32) -> u32 {
        rate_hz
    }

    fn room(&self) -> usize {
        usize::MAX
    }

    fn write(&mut self, _frames: &[i16]) {}

    fn close(&mut self) {}
}
