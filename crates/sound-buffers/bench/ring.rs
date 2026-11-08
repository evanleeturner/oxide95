// SPDX-License-Identifier: MIT
//! The worked ring S6 plays, and a tee that compares every frame the mix hands over with
//! the worked one at its place. Stream place `i` holds frame `i % RING_FRAMES`, so the
//! answer is arithmetic and owes nothing to the code it checks.
//!
//! Taken by `#[path]` by the route, on a real back, and by the logic test, on the null one.
//!
//! Hazard: frames are compared by place, so one dropped or passed-over frame makes
//! everything after it read wrong. Read the counts before the first wrong frame.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use sound_buffers::sdk::{WAVE_FORMAT_PCM, WaveFormat};
use sound_mix::output::Output;
use sound_mix::witness::{HASH_START, hash_samples};

pub const MIX_HZ: u32 = 44_100;
/// Half a second, so 1.2 s of clock crosses at least 2 seams.
pub const RING_FRAMES: u32 = 22_050;
pub const RING_BYTES: u32 = RING_FRAMES * 4;
/// Where the one `Lock` starts: its last 4,000 bytes first, then the rest from the start.
pub const WRAP_AT: u32 = 84_200;
pub const FIRST_SPAN_BYTES: u32 = RING_BYTES - WRAP_AT;
/// The frame whose left sample the flip plant negates, in the second span.
pub const FLIP_FRAME: u32 = 15_000;

/// The worked frame at stream place `place`.
#[must_use]
pub fn worked_frame(place: u64) -> [i16; 2] {
    let frame = i64::try_from(place % u64::from(RING_FRAMES)).expect("under 22,050");
    let left = frame % 251 * 200 - 25_000;
    let right = 12_000 - frame % 127 * 150;
    [left, right].map(|sample| i16::try_from(sample).expect("the formula stays in 16 bits"))
}

/// The FNV-1a of the first `frames` worked frames, as the witness hashes them.
#[must_use]
pub fn worked_hash(frames: u64) -> u64 {
    (0..frames).fold(HASH_START, |hash, place| {
        hash_samples(hash, &worked_frame(place))
    })
}

/// The ring's bytes in order; `flipped` negates [`FLIP_FRAME`]'s left sample as written.
#[must_use]
pub fn ring_image(flipped: bool) -> Vec<u8> {
    let mut image = Vec::with_capacity(RING_BYTES as usize);
    for frame in 0..RING_FRAMES {
        let [mut left, right] = worked_frame(u64::from(frame));
        if flipped && frame == FLIP_FRAME {
            left = -left;
        }
        image.extend_from_slice(&left.to_le_bytes());
        image.extend_from_slice(&right.to_le_bytes());
    }
    image
}

/// The ring's format: 16-bit stereo at the mix's rate, so a frame in is a frame out.
#[must_use]
pub const fn ring_format() -> WaveFormat {
    WaveFormat {
        tag: WAVE_FORMAT_PCM,
        channels: 2,
        rate_hz: MIX_HZ,
        bytes_per_second: MIX_HZ * 4,
        block_align: 4,
        bits: 16,
        extra: 0,
    }
}

/// What the tee has read, by the frame's place.
pub struct Places {
    next: Cell<u64>,
    pub first_wrong: Cell<Option<u64>>,
    pub wrong: Cell<u64>,
    /// The rate the back's `open` answered.
    pub took: Cell<u32>,
    /// The lowest the mix read of `room`.
    pub room_low: Cell<usize>,
}

/// A tee over any back. Clones share one back and one tally, so a holder still reads
/// them once a front owns the other clone.
pub struct Placed<O> {
    back: Rc<RefCell<O>>,
    places: Rc<Places>,
}

impl<O> Clone for Placed<O> {
    fn clone(&self) -> Self {
        Self {
            back: Rc::clone(&self.back),
            places: Rc::clone(&self.places),
        }
    }
}

impl<O: Output> Placed<O> {
    #[must_use]
    pub fn new(back: O) -> Self {
        Self {
            back: Rc::new(RefCell::new(back)),
            places: Rc::new(Places {
                next: Cell::new(0),
                first_wrong: Cell::new(None),
                wrong: Cell::new(0),
                took: Cell::new(0),
                room_low: Cell::new(usize::MAX),
            }),
        }
    }

    #[must_use]
    pub fn places(&self) -> &Places {
        &self.places
    }

    /// The back's room read from outside, so it does not move `room_low`.
    #[allow(
        dead_code,
        reason = "the route's alone: the test's null back has no drain"
    )]
    #[must_use]
    pub fn room_now(&self) -> usize {
        self.back.borrow().room()
    }
}

impl<O: Output> Output for Placed<O> {
    fn open(&mut self, rate_hz: u32) -> u32 {
        let took = self.back.borrow_mut().open(rate_hz);
        self.places.took.set(took);
        took
    }

    fn room(&self) -> usize {
        let room = self.back.borrow().room();
        self.places
            .room_low
            .set(self.places.room_low.get().min(room));
        room
    }

    fn write(&mut self, frames: &[i16]) {
        let places = &self.places;
        for frame in frames.chunks_exact(2) {
            let place = places.next.get();
            if *frame != worked_frame(place) {
                places.wrong.set(places.wrong.get() + 1);
                if places.first_wrong.get().is_none() {
                    places.first_wrong.set(Some(place));
                }
            }
            places.next.set(place + 1);
        }
        self.back.borrow_mut().write(frames);
    }

    fn close(&mut self) {
        self.back.borrow_mut().close();
    }
}
