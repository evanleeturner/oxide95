// SPDX-License-Identifier: MIT
//! One voice: its sound, where the clock says it is, and its share of the mix.
//!
//! Two cursors, on purpose. `anchor_*` is the clock's, and alone decides the
//! status, so a sample ends with no output at all. `mix_*` walks the bytes for
//! the mix, and moves only when the mix runs. They agree to within a frame.
//! The volume and pan curves are plain linear ones, unmeasured against Miles.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]

use alloc::vec::Vec;

/// The mix's own rate, in frames a second.
pub(crate) const MIX_HZ: u64 = 44_100;
/// `mix_at`'s fixed point: 16 bits of a frame.
pub(crate) const FRACTION: u32 = 16;

/// mss.h's values; the bytes compare 2, 4 and 8.
pub const SMP_FREE: u32 = 1;
pub const SMP_DONE: u32 = 2;
pub const SMP_PLAYING: u32 = 4;
pub const SMP_STOPPED: u32 = 8;

/// How many of a voice's user words a program may keep.
pub(crate) const USER_WORDS: usize = 8;

pub(crate) struct Voice {
    pub(crate) handle: u32,
    pub(crate) used: bool,
    pub(crate) status: u32,
    pub(crate) channels: u32,
    pub(crate) bits: u32,
    pub(crate) rate_hz: u32,
    pub(crate) volume: i32,
    pub(crate) pan: i32,
    pub(crate) loops: u32,
    pub(crate) callback: u32,
    pub(crate) user: [u32; USER_WORDS],
    /// The file's sound, copied, so the block's owner may free it at any time.
    file: Vec<u8>,
    /// The double buffer's slots, each copied at its load.
    slots: [Vec<u8>; 2],
    fill_next: usize,
    playing_slot: usize,
    anchor_us: u64,
    anchor_frame: u64,
    mix_at: u64,
    mix_slot: usize,
    /// A slot the mix has read to its end and the clock has not yet emptied:
    /// the mix may run ahead of the clock inside one long pass, and must not
    /// read that slot again.
    mix_done: [bool; 2],
    mix_loop: u32,
}

impl Voice {
    pub(crate) fn new(handle: u32) -> Self {
        let mut voice = Self {
            handle,
            used: true,
            status: SMP_DONE,
            channels: 1,
            bits: 8,
            rate_hz: 11_025,
            volume: 127,
            pan: 64,
            loops: 1,
            callback: 0,
            user: [0; USER_WORDS],
            file: Vec::new(),
            slots: [Vec::new(), Vec::new()],
            fill_next: 0,
            playing_slot: 0,
            anchor_us: 0,
            anchor_frame: 0,
            mix_at: 0,
            mix_slot: 0,
            mix_done: [false; 2],
            mix_loop: 0,
        };
        voice.init();
        voice
    }

    /// Back to the defaults, the handle kept.
    pub(crate) fn init(&mut self) {
        self.status = SMP_DONE;
        self.channels = 1;
        self.bits = 8;
        self.rate_hz = 11_025;
        self.volume = 127;
        self.pan = 64;
        self.loops = 1;
        self.callback = 0;
        self.user = [0; USER_WORDS];
        self.file.clear();
        self.slots = [Vec::new(), Vec::new()];
        self.fill_next = 0;
        self.playing_slot = 0;
        self.anchor_frame = 0;
        self.mix_at = 0;
        self.mix_slot = 0;
        self.mix_done = [false; 2];
        self.mix_loop = 0;
    }

    fn frame_bytes(&self) -> usize {
        (self.channels * self.bits.div_ceil(8)).max(1) as usize
    }

    fn file_frames(&self) -> u64 {
        (self.file.len() / self.frame_bytes()) as u64
    }

    fn is_stream(&self) -> bool {
        self.file.is_empty()
    }

    pub(crate) fn set_file(&mut self, channels: u32, bits: u32, rate_hz: u32, data: &[u8]) {
        self.channels = channels;
        self.bits = bits;
        self.rate_hz = rate_hz;
        self.file.clear();
        self.file.extend_from_slice(data);
    }

    pub(crate) fn start(&mut self, now_us: u64) {
        self.status = SMP_PLAYING;
        self.anchor_us = now_us;
        self.anchor_frame = 0;
        self.mix_at = 0;
        self.mix_loop = 0;
        self.mix_done = [false; 2];
        self.playing_slot = self.first_filled();
        self.mix_slot = self.playing_slot;
    }

    fn first_filled(&self) -> usize {
        // The slot loaded first: the one the next load would not take.
        if self.slots[self.fill_next].is_empty() {
            1 - self.fill_next
        } else {
            self.fill_next
        }
    }

    /// Frames the clock has passed since the anchor.
    fn clock_frames(&self, now_us: u64) -> u64 {
        now_us.saturating_sub(self.anchor_us) * u64::from(self.rate_hz) / 1_000_000
    }

    pub(crate) fn stop(&mut self, now_us: u64) {
        if self.status == SMP_PLAYING {
            self.anchor_frame += self.clock_frames(now_us);
            self.status = SMP_STOPPED;
        }
    }

    pub(crate) fn resume(&mut self, now_us: u64) {
        if self.status == SMP_STOPPED {
            self.anchor_us = now_us;
            self.status = SMP_PLAYING;
        }
    }

    /// Ends it now; the answer is whether an end-of-sample is owed.
    pub(crate) fn end(&mut self) -> bool {
        let owed = self.status == SMP_PLAYING || self.status == SMP_STOPPED;
        self.status = SMP_DONE;
        owed
    }

    pub(crate) fn set_rate(&mut self, now_us: u64, rate_hz: u32) {
        if self.status == SMP_PLAYING {
            self.anchor_frame += self.clock_frames(now_us);
            self.anchor_us = now_us;
        }
        self.rate_hz = rate_hz;
    }

    /// The double buffer's next slot to fill, -1 if both hold sound.
    pub(crate) fn ready(&self) -> i32 {
        if self.slots[self.fill_next].is_empty() {
            self.fill_next as i32
        } else {
            -1
        }
    }

    pub(crate) fn load(&mut self, slot: usize, data: &[u8]) {
        self.slots[slot].clear();
        self.slots[slot].extend_from_slice(data);
        self.mix_done[slot] = false;
        self.fill_next = 1 - slot;
    }

    /// Moves the status by the clock; the answer is whether this call ended it.
    pub(crate) fn catch_up(&mut self, now_us: u64) -> bool {
        if self.status != SMP_PLAYING {
            return false;
        }
        let rate = u64::from(self.rate_hz.max(1));
        let ended = if self.is_stream() {
            self.catch_up_stream(now_us, rate)
        } else {
            self.loops != 0 && {
                let end = self.file_frames() * u64::from(self.loops);
                let left = end.saturating_sub(self.anchor_frame);
                now_us.saturating_sub(self.anchor_us) * rate >= left * 1_000_000
            }
        };
        if ended {
            self.status = SMP_DONE;
        }
        ended
    }

    /// A slot passed by the clock empties, and makes the other the playing one.
    /// `anchor_frame` is how far a stop or a rate change left this slot.
    fn catch_up_stream(&mut self, now_us: u64, rate: u64) -> bool {
        loop {
            let bytes = self.slots[self.playing_slot].len();
            if bytes == 0 {
                return true;
            }
            let left = ((bytes / self.frame_bytes()) as u64).saturating_sub(self.anchor_frame);
            if now_us.saturating_sub(self.anchor_us) * rate < left * 1_000_000 {
                return false;
            }
            self.anchor_us += (left * 1_000_000).div_ceil(rate);
            self.anchor_frame = 0;
            self.slots[self.playing_slot].clear();
            self.mix_done[self.playing_slot] = false;
            if self.mix_slot == self.playing_slot {
                self.mix_slot = 1 - self.mix_slot;
                self.mix_at = 0;
            }
            self.playing_slot = 1 - self.playing_slot;
        }
    }

    /// One sample as `i32` at 16-bit scale: 8-bit sound is unsigned on disc.
    fn sample(&self, bytes: &[u8], at: usize) -> i32 {
        if self.bits == 8 {
            (i32::from(bytes[at]) - 128) << 8
        } else {
            i32::from(i16::from_le_bytes([bytes[at], bytes[at + 1]]))
        }
    }

    /// Adds this voice to `out`, stereo pairs, moving the mix's cursor.
    pub(crate) fn render(&mut self, out: &mut [i32]) {
        if self.status != SMP_PLAYING {
            return;
        }
        let step = (u64::from(self.rate_hz) << FRACTION) / MIX_HZ;
        let frame_bytes = self.frame_bytes();
        let volume = self.volume.clamp(0, 127);
        let left_gain = volume * (127 - self.pan).clamp(0, 63);
        let right_gain = volume * self.pan.clamp(0, 64);
        for pair in out.chunks_exact_mut(2) {
            let Some(at) = self.next_frame(frame_bytes) else {
                return;
            };
            let source: &[u8] = if self.is_stream() {
                &self.slots[self.mix_slot]
            } else {
                &self.file
            };
            let left = self.sample(source, at);
            let right = if self.channels > 1 {
                self.sample(source, at + frame_bytes / 2)
            } else {
                left
            };
            pair[0] += left * left_gain / (127 * 63);
            pair[1] += right * right_gain / (127 * 64);
            self.mix_at += step;
        }
    }

    /// The byte the mix reads next, wrapping a loop or crossing a slot; `None` at the end.
    fn next_frame(&mut self, frame_bytes: usize) -> Option<usize> {
        loop {
            let frame = (self.mix_at >> FRACTION) as usize;
            if self.is_stream() {
                let bytes = self.slots[self.mix_slot].len();
                if frame * frame_bytes + frame_bytes <= bytes {
                    return Some(frame * frame_bytes);
                }
                let other = 1 - self.mix_slot;
                if self.slots[other].is_empty() || self.mix_done[other] {
                    return None;
                }
                self.mix_done[self.mix_slot] = true;
                self.mix_slot = other;
                self.mix_at = 0;
            } else {
                if frame * frame_bytes + frame_bytes <= self.file.len() {
                    return Some(frame * frame_bytes);
                }
                self.mix_loop += 1;
                if self.file.is_empty() || (self.loops != 0 && self.mix_loop >= self.loops) {
                    return None;
                }
                self.mix_at = 0;
            }
        }
    }
}
