// SPDX-License-Identifier: MIT
//! A disc's sound through `sound-mix`'s door. The door has no pause and cannot say how
//! much it holds, so a pause closes it, a resume opens a new one, and [`DoorPlayer::pump`]
//! holds what it has pushed to [`LEAD_FRAMES`] past the clock's position. A volume change
//! therefore reaches the ear up to that late.
//!
//! [`DoorPlayer::pump`] is the only call that moves samples, and no call a program makes
//! comes often enough to be it: whoever holds the player calls it more often than the
//! lead, on the program's own thread or, the player being `Send` where its parts are,
//! on another behind a lock of the holder's.
use alloc::vec::Vec;
use core::cell::{Cell, RefCell};
use core::ops::Deref;

use logical_disc::{Audio, Disc, ReadAt};
use sound_mix::Host;
use sound_mix::output::Output;

use crate::Back;
use crate::disc::unread;
use crate::play::{Clock, Player, frames_since};
use crate::table::{Place, Tracks};

/// A disc's own shape, the only one the door is asked for.
const RATE_HZ: u32 = 44_100;
const CHANNELS: u32 = 2;

/// Numbers in one of a disc's frames: 588 pairs, a seventy-fifth of a second.
pub const FRAME_SAMPLES: usize = 1176;
/// A disc frame's bytes.
const FRAME_BYTES: u64 = 2352;
/// How far past the clock's position a push may reach: 250 ms, in whole frames.
pub const LEAD_FRAMES: u32 = 18;

/// The full volume, at which a sample passes as it came.
const FULL: u16 = 0xffff;

/// Where the sound goes.
pub trait Door {
    /// Opens a voice; the number it goes by.
    fn open(&self, rate_hz: u32, channels: u32) -> u32;
    /// Queues interleaved samples. A number already closed is the caller's fault.
    fn push(&self, id: u32, samples: &[i16]);
    /// Ends a voice and what it held unheard.
    fn close(&self, id: u32);
}

impl<D: Door> Door for &D {
    fn open(&self, rate_hz: u32, channels: u32) -> u32 {
        (*self).open(rate_hz, channels)
    }

    fn push(&self, id: u32, samples: &[i16]) {
        (*self).push(id, samples);
    }

    fn close(&self, id: u32) {
        (*self).close(id);
    }
}

impl<H: Host, O: Output> Door for sound_mix::Front<H, O> {
    fn open(&self, rate_hz: u32, channels: u32) -> u32 {
        self.stream_open(rate_hz, channels)
    }

    fn push(&self, id: u32, samples: &[i16]) {
        self.stream_push(id, samples);
    }

    fn close(&self, id: u32) {
        self.stream_close(id);
    }
}

/// Where the samples come from.
pub trait Samples {
    /// Fills `into` from `place` on, within its track; how many numbers it wrote, short
    /// at the track's end, or `None` if the track would not read.
    fn read(&mut self, place: Place, into: &mut [i16]) -> Option<usize>;
}

/// An image's tracks, read a range at a time, a big-endian file's samples turned. It holds
/// the disc as `D` does: a borrow, or whatever lets it cross to another thread.
pub struct ImageSamples<D> {
    disc: D,
    bytes: Vec<u8>,
}

impl<D> ImageSamples<D> {
    pub const fn new(disc: D) -> Self {
        Self {
            disc,
            bytes: Vec::new(),
        }
    }
}

impl<R: ReadAt, D: Deref<Target = Disc<R>>> Samples for ImageSamples<D> {
    fn read(&mut self, place: Place, into: &mut [i16]) -> Option<usize> {
        let big_endian = self
            .disc
            .tracks()
            .iter()
            .find_map(|track| match &track.audio {
                Audio::Pcm(pcm) if track.number == place.track => Some(pcm.big_endian),
                _ => None,
            })?;
        self.bytes.resize(into.len() * 2, 0);
        let at = u64::from(place.frame) * FRAME_BYTES;
        let moved = self
            .disc
            .read_track(place.track, at, &mut self.bytes)
            .ok()?;
        for (sample, pair) in into.iter_mut().zip(self.bytes[..moved].chunks_exact(2)) {
            let pair = [pair[0], pair[1]];
            *sample = if big_endian {
                i16::from_be_bytes(pair)
            } else {
                i16::from_le_bytes(pair)
            };
        }
        Some(moved / 2)
    }
}

/// One open door and how far it has been fed.
#[derive(Clone, Copy)]
struct Run {
    id: u32,
    since_ms: u32,
    from: u32,
    next: u32,
    end: u32,
}

/// A [`Player`] over a door, one voice at a time.
pub struct DoorPlayer<'t, B, D, S, C> {
    back: B,
    door: D,
    samples: RefCell<S>,
    tracks: Tracks<'t>,
    clock: C,
    run: Cell<Option<Run>>,
    left: Cell<u16>,
    right: Cell<u16>,
    buffer: RefCell<Vec<i16>>,
}

impl<'t, B: Back, D: Door, S: Samples, C: Clock> DoorPlayer<'t, B, D, S, C> {
    pub const fn new(back: B, door: D, samples: S, tracks: Tracks<'t>, clock: C) -> Self {
        Self {
            back,
            door,
            samples: RefCell::new(samples),
            tracks,
            clock,
            run: Cell::new(None),
            left: Cell::new(FULL),
            right: Cell::new(FULL),
            buffer: RefCell::new(Vec::new()),
        }
    }

    /// Feeds the door up to the lead. What the clock has passed is not pushed late, so
    /// after a long wait the ear rejoins the position. A track that will not read gets one
    /// line and is silent to its end, the clock running on.
    pub fn pump(&self) {
        let Some(mut run) = self.run.get() else {
            return;
        };
        let at = run
            .from
            .saturating_add(frames_since(self.clock.now_ms(), run.since_ms));
        let limit = at.saturating_add(LEAD_FRAMES).min(run.end);
        run.next = run.next.max(at);
        while run.next < limit {
            let place = self.tracks.place(run.next);
            let frames = self.feed(run.id, place, limit - run.next);
            if frames == 0 {
                unread(&self.back, place.track);
                run.next = self.tracks.track_end(run.next);
            } else {
                run.next += frames;
            }
        }
        self.run.set(Some(run));
    }

    /// Up to `frames` from `place`, scaled and pushed; how many whole frames that was.
    fn feed(&self, id: u32, place: Place, frames: u32) -> u32 {
        let mut buffer = self.buffer.borrow_mut();
        buffer.resize(frames as usize * FRAME_SAMPLES, 0);
        let read = self.samples.borrow_mut().read(place, &mut buffer);
        let whole = read.unwrap_or(0) / FRAME_SAMPLES;
        if whole == 0 {
            return 0;
        }
        let pushed = &mut buffer[..whole * FRAME_SAMPLES];
        scale(pushed, self.left.get(), self.right.get());
        self.door.push(id, pushed);
        u32::try_from(whole).unwrap_or(frames)
    }

    /// Lets go of the door, exactly once however often it is asked.
    fn close_door(&self) {
        if let Some(run) = self.run.take() {
            self.door.close(run.id);
        }
    }
}

impl<B: Back, D: Door, S: Samples, C: Clock> Player for DoorPlayer<'_, B, D, S, C> {
    fn play(&self, from: Place, to: Place) {
        self.close_door();
        let (Some(from), Some(end)) = (self.tracks.end_of(from), self.tracks.end_of(to)) else {
            return;
        };
        self.run.set(Some(Run {
            id: self.door.open(RATE_HZ, CHANNELS),
            since_ms: self.clock.now_ms(),
            from,
            next: from,
            end,
        }));
        self.pump();
    }

    fn pause(&self) {
        self.close_door();
    }

    fn stop(&self) {
        self.close_door();
    }

    fn volume(&self, left: u16, right: u16) {
        self.left.set(left);
        self.right.set(right);
    }
}

fn scale(samples: &mut [i16], left: u16, right: u16) {
    for pair in samples.chunks_exact_mut(2) {
        pair[0] = side(pair[0], left);
        pair[1] = side(pair[1], right);
    }
}

fn side(sample: i16, volume: u16) -> i16 {
    let scaled = i32::from(sample) * i32::from(volume) / i32::from(FULL);
    i16::try_from(scaled).unwrap_or(sample)
}
