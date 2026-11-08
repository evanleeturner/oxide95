// SPDX-License-Identifier: MIT
//! The reference mix: one script of 3 voices, and the stream it must make, worked by
//! arithmetic alone, so a route's output is judged by an answer no mix code touched.
//! Taken by `#[path]` from every caller, and it names no route.
//!
//! The clock is the bench's own. The mix renders every frame from a voice's start, so
//! this script makes one stream at any pace, exactly [`FRAMES`] long once the clock
//! reaches [`END_US`], where it is clamped.
//!
//! Hazard: frames are compared by their place in what the back was handed. A frame the
//! mix drops for want of room is never handed over, so everything after it reads wrong;
//! read [`Outcome::kept`] and the mix's dropped count before calling that a wrong sound.
#![allow(
    dead_code,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "each caller takes its part of this file, and the worked numbers fit by their formulas"
)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use sound_mix::output::Output;
use sound_mix::table::Row;
use sound_mix::witness::{HASH_START, Seen, Witness, hash_samples};
use sound_mix::{Front, Host};

/// The stream's whole length, 2 seconds at the mix's rate.
pub const FRAMES: u32 = 88_200;
/// The frames one block of the stream's report covers.
pub const BLOCK_FRAMES: u64 = 1_024;
/// Where the clock is held while voice C starts: frame [`C_STARTS`].
pub const C_STARTS_US: u64 = 500_000;
/// Where the clock is clamped.
pub const END_US: u64 = 2_000_000;
/// The stream's FNV-1a hash, from a second program outside this crate that reads no Rust.
pub const PINNED_HASH: u64 = 0x1ef6_27c8_0ea3_5f99;

const C_STARTS: u32 = 22_050;
const B_FRAMES: u32 = 2_205;
/// B's 2 loops, each source frame twice.
const B_ENDS: u32 = B_FRAMES * 2 * 2;
const C_FRAMES: u32 = 2_756;
/// Each of C's source frames 4 times.
const C_ENDS: u32 = C_STARTS + C_FRAMES * 4;
/// The one sample of A that [`Plant::Flip`] negates.
const FLIPPED_SAMPLE: u32 = 30_000;

static ROW: Row = Row {
    file: "reference",
    module: "reference",
};

/// A wrong input the bench must catch, or none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Plant {
    Clean,
    /// Every voice at volume 0.
    Silence,
    /// One sample of A negated in its file.
    Flip,
    /// Left and right exchanged in the files of B and C.
    Swap,
    /// B played at 44,100 Hz, not its file's 22,050.
    Rate,
}

/// Each plant, and the block its first wrong frame must fall in.
pub const PLANTS: [(Plant, u64); 4] = [
    (Plant::Silence, 0),
    (Plant::Flip, 29),
    (Plant::Swap, 0),
    (Plant::Rate, 0),
];

fn a_sample(n: u32) -> i32 {
    (n % 100) as i32 * 300 - 15_000
}

fn b_bytes(n: u32) -> (u8, u8) {
    ((128 + n % 50) as u8, (127 - n % 25) as u8)
}

fn c_samples(n: u32) -> (i32, i32) {
    (30_000 - (n % 10) as i32 * 10, -30_000 + (n % 7) as i32 * 10)
}

/// An unsigned 8-bit sample at 16-bit scale.
fn scaled(byte: u8) -> i32 {
    (i32::from(byte) - 128) * 256
}

/// Frame `frame` of the stream, as (left, right), at any frame below [`FRAMES`].
///
/// A and C pass unchanged, at volume 127 and pan 64. B at volume 64 and pan 32 scales by
/// `64 * 63 / (127 * 63)` left and `64 * 32 / (127 * 64)` right, cutting toward zero, and
/// the sum is clamped to 16 bits: A plus C passes full scale.
#[must_use]
pub fn frame_at(frame: u32) -> (i16, i16) {
    let voice_a = a_sample(frame);
    let (mut left, mut right) = (voice_a, voice_a);
    if frame < B_ENDS {
        let (left_byte, right_byte) = b_bytes((frame % (B_FRAMES * 2)) / 2);
        left += scaled(left_byte) * 4_032 / 8_001;
        right += scaled(right_byte) * 2_048 / 8_128;
    }
    if (C_STARTS..C_ENDS).contains(&frame) {
        let (left_sample, right_sample) = c_samples((frame - C_STARTS) / 4);
        left += left_sample;
        right += right_sample;
    }
    (clamped(left), clamped(right))
}

fn clamped(sum: i32) -> i16 {
    sum.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
}

/// The whole stream, interleaved stereo.
#[must_use]
pub fn worked() -> Vec<i16> {
    (0..FRAMES)
        .flat_map(|frame| {
            let (left, right) = frame_at(frame);
            [left, right]
        })
        .collect()
}

/// A PCM WAVE file around `data`, its head as the mix's parser reads one.
fn wave(channels: u16, bits: u16, rate_hz: u32, data: &[u8]) -> Vec<u8> {
    let bytes = data.len() as u32;
    let align = channels * bits / 8;
    let mut file = Vec::with_capacity(44 + data.len());
    file.extend_from_slice(b"RIFF");
    file.extend_from_slice(&(36 + bytes).to_le_bytes());
    file.extend_from_slice(b"WAVEfmt ");
    file.extend_from_slice(&16u32.to_le_bytes());
    file.extend_from_slice(&1u16.to_le_bytes());
    file.extend_from_slice(&channels.to_le_bytes());
    file.extend_from_slice(&rate_hz.to_le_bytes());
    file.extend_from_slice(&(rate_hz * u32::from(align)).to_le_bytes());
    file.extend_from_slice(&align.to_le_bytes());
    file.extend_from_slice(&bits.to_le_bytes());
    file.extend_from_slice(b"data");
    file.extend_from_slice(&bytes.to_le_bytes());
    file.extend_from_slice(data);
    file
}

fn file_a(plant: Plant) -> Vec<u8> {
    let mut data = Vec::with_capacity(FRAMES as usize * 2);
    for sample in 0..FRAMES {
        let value = a_sample(sample) as i16;
        let value = if plant == Plant::Flip && sample == FLIPPED_SAMPLE {
            -value
        } else {
            value
        };
        data.extend_from_slice(&value.to_le_bytes());
    }
    wave(1, 16, 44_100, &data)
}

fn file_b(plant: Plant) -> Vec<u8> {
    let mut data = Vec::with_capacity(B_FRAMES as usize * 2);
    for sample in 0..B_FRAMES {
        let (left, right) = b_bytes(sample);
        data.extend_from_slice(&if plant == Plant::Swap {
            [right, left]
        } else {
            [left, right]
        });
    }
    wave(2, 8, 22_050, &data)
}

fn file_c(plant: Plant) -> Vec<u8> {
    let mut data = Vec::with_capacity(C_FRAMES as usize * 4);
    for sample in 0..C_FRAMES {
        let (left, right) = c_samples(sample);
        let (first, second) = if plant == Plant::Swap {
            (right, left)
        } else {
            (left, right)
        };
        data.extend_from_slice(&(first as i16).to_le_bytes());
        data.extend_from_slice(&(second as i16).to_le_bytes());
    }
    wave(2, 16, 11_025, &data)
}

/// The script's clock: the bench sets it, the mix only reads it.
struct HeldClock(Rc<Cell<u64>>);

impl Host for HeldClock {
    fn now_us(&self) -> u64 {
        self.0.get()
    }

    fn call_back(&self, _callback: u32, _handle: u32) {}

    fn read_file(&self, _name: &str) -> Option<u32> {
        None
    }

    fn free_block(&self, _block: u32) {}
}

/// What the comparison has seen so far.
#[derive(Default)]
struct Reading {
    at: Cell<u64>,
    first_wrong: Cell<Option<u64>>,
    wrong: Cell<u64>,
    kept: Cell<u64>,
}

/// Compares each frame to the worked stream, then hands it on. Every answer is the
/// back's own. A frame past the stream's end counts as wrong.
struct Against<O> {
    back: Rc<RefCell<O>>,
    want: Rc<Vec<i16>>,
    reading: Rc<Reading>,
}

impl<O: Output> Output for Against<O> {
    fn open(&mut self, rate_hz: u32) -> u32 {
        self.back.borrow_mut().open(rate_hz)
    }

    fn room(&self) -> usize {
        self.back.borrow().room()
    }

    fn write(&mut self, frames: &[i16]) {
        let reading = &self.reading;
        let room = self.back.borrow().room();
        let handed = frames.len() / 2;
        reading
            .kept
            .set(reading.kept.get() + handed.min(room) as u64);
        for pair in frames.chunks_exact(2) {
            let at = reading.at.get();
            let place = at as usize * 2;
            if self.want.get(place..place + 2) != Some(pair) {
                if reading.first_wrong.get().is_none() {
                    reading.first_wrong.set(Some(at));
                }
                reading.wrong.set(reading.wrong.get() + 1);
            }
            reading.at.set(at + 1);
        }
        self.back.borrow_mut().write(frames);
    }

    fn close(&mut self) {
        self.back.borrow_mut().close();
    }
}

/// What one run of the script came to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Outcome {
    /// The frames the witness saw.
    pub frames: u64,
    /// The witness's hash of them.
    pub hash: u64,
    /// The first frame unlike the worked stream.
    pub first_wrong: Option<u64>,
    pub wrong: u64,
    /// Over all writes, the frames the back had room for.
    pub kept: u64,
}

impl Outcome {
    /// The block holding the first wrong frame, `None` on a stream with none wrong.
    #[must_use]
    pub fn block(&self) -> Option<u64> {
        self.first_wrong.map(|frame| frame / BLOCK_FRAMES)
    }

    /// What the person reads when the mix is wrong, `None` when no frame was.
    #[must_use]
    pub fn sentence(&self) -> Option<String> {
        self.block().map(|block| {
            format!(
                "The sound mixer gave different sound from the worked answer at block {block}. \
                 Your computer is cleared."
            )
        })
    }
}

/// The 3 voices over `O`, driven by a clock only the bench moves.
pub struct Script<O> {
    front: Front<HeldClock, Witness<Against<O>>>,
    back: Rc<RefCell<O>>,
    clock: Rc<Cell<u64>>,
    seen: Seen,
    reading: Rc<Reading>,
    driver: u32,
    voice_a: u32,
    voice_b: u32,
    voice_c: u32,
    c_started: Cell<bool>,
}

impl<O: Output> Script<O> {
    /// Opens the back and loads the voices at clock 0, with `plant` in place. Nothing
    /// sounds until [`Script::start`].
    pub fn load(back: O, plant: Plant) -> Self {
        let clock = Rc::new(Cell::new(0));
        let reading = Rc::new(Reading::default());
        let back = Rc::new(RefCell::new(back));
        let witness = Witness::new(Against {
            back: Rc::clone(&back),
            want: Rc::new(worked()),
            reading: Rc::clone(&reading),
        });
        let seen = witness.seen();
        let front = Front::new(&ROW, HeldClock(Rc::clone(&clock)), witness);
        let (_, driver) = front.wave_out_open(0, 44_100, 2, 16);
        let voice_a = front.allocate_file_sample(driver, 0x10, &file_a(plant), 0);
        let voice_b = front.allocate_file_sample(driver, 0x20, &file_b(plant), 0);
        let voice_c = front.allocate_file_sample(driver, 0x30, &file_c(plant), 0);
        front.set_sample_volume(voice_b, 64);
        front.set_sample_pan(voice_b, 32);
        front.set_sample_loop_count(voice_b, 2);
        if plant == Plant::Silence {
            for voice in [voice_a, voice_b, voice_c] {
                front.set_sample_volume(voice, 0);
            }
        }
        if plant == Plant::Rate {
            front.set_sample_playback_rate(voice_b, 44_100);
        }
        Self {
            front,
            back,
            clock,
            seen,
            reading,
            driver,
            voice_a,
            voice_b,
            voice_c,
            c_started: Cell::new(false),
        }
    }

    /// Starts A and B at the clock's frame 0.
    pub fn start(&self) {
        self.front.start_sample(self.voice_a);
        self.front.start_sample(self.voice_b);
    }

    /// Moves the clock to `now_us`, clamped to [`END_US`], and mixes up to there. The
    /// clock stops at [`C_STARTS_US`] on the way, so C starts on one frame at any pace.
    /// A `now_us` behind the clock moves nothing: the mix's clock never goes back.
    pub fn run_to(&self, now_us: u64) {
        let now_us = now_us.min(END_US);
        if now_us >= C_STARTS_US && !self.c_started.replace(true) {
            self.clock.set(C_STARTS_US);
            self.front.pump();
            self.front.start_sample(self.voice_c);
        }
        self.clock.set(self.clock.get().max(now_us));
        self.front.pump();
    }

    /// The back's room, read directly, so a route can read it once the mix stops asking.
    pub fn room(&self) -> usize {
        self.back.borrow().room()
    }

    /// Ends the voices and closes the back.
    pub fn close(&self) {
        self.front.wave_out_close(self.driver);
    }

    #[must_use]
    pub fn outcome(&self) -> Outcome {
        let summary = self.seen.summary();
        Outcome {
            frames: summary.frames,
            hash: summary.hash,
            first_wrong: self.reading.first_wrong.get(),
            wrong: self.reading.wrong.get(),
            kept: self.reading.kept.get(),
        }
    }
}

/// The hash of each [`BLOCK_FRAMES`] of `samples`, each from the start value, so a
/// difference names its block.
#[must_use]
pub fn block_hashes(samples: &[i16]) -> Vec<u64> {
    samples
        .chunks(BLOCK_FRAMES as usize * 2)
        .map(|block| hash_samples(HASH_START, block))
        .collect()
}
