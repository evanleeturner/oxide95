// SPDX-License-Identifier: MIT
//! A clip's sound through the real mixer, a clock of the test's and a back that
//! keeps what it is handed. The expected numbers were worked by hand before the
//! code first ran: the mix runs at 44,100 frames a second, so at 44,100 Hz a door
//! gives one frame a sample, and 46 microseconds is 2 frames.
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use sound_mix::output::Output;
use sound_mix::table::Row;
use sound_mix::{Front, Host, SMP_DONE};

use super::clips::samples_clip;
use super::fakes::{FakeBytes, FakeClock, holding_with};
use super::test_log::debug_lines;
use crate::{Choices, MixDoor, Videos};

static ROW: Row = Row {
    file: "a program",
    module: "MSS32",
};

/// The library's bit for a clip's first track.
const TRACK_1: u32 = 0x2000;
const ALL_7: u32 = 0xfe000;
const MIX_HZ: u32 = 44_100;
/// 10,000 microseconds a frame.
const RATE: i32 = -1000;
const PERIOD_US: u64 = 10_000;

/// The mix's clock, moved by the test.
#[derive(Clone, Default)]
struct Seat(Arc<AtomicU64>);

impl Seat {
    fn at(&self, now_us: u64) {
        self.0.store(now_us, Ordering::Relaxed);
    }
}

impl Host for Seat {
    fn now_us(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }

    fn call_back(&self, _callback: u32, _handle: u32) {}

    fn read_file(&self, _name: &str) -> Option<u32> {
        None
    }

    fn free_block(&self, _block: u32) {}
}

#[derive(Default)]
struct Seen {
    frames: Vec<i16>,
    opens: u32,
    closes: u32,
}

/// A back that keeps its frames.
#[derive(Clone, Default)]
struct Back(Arc<Mutex<Seen>>);

impl Back {
    fn frames(&self) -> Vec<i16> {
        self.0
            .lock()
            .expect("no test panics holding this")
            .frames
            .clone()
    }

    fn opens(&self) -> u32 {
        self.0.lock().expect("no test panics holding this").opens
    }

    fn closes(&self) -> u32 {
        self.0.lock().expect("no test panics holding this").closes
    }
}

impl Output for Back {
    fn open(&mut self, rate_hz: u32) -> u32 {
        self.0.lock().expect("no test panics holding this").opens += 1;
        rate_hz
    }

    fn room(&self) -> usize {
        usize::MAX
    }

    fn write(&mut self, frames: &[i16]) {
        self.0
            .lock()
            .expect("no test panics holding this")
            .frames
            .extend_from_slice(frames);
    }

    fn close(&mut self) {
        self.0.lock().expect("no test panics holding this").closes += 1;
    }
}

type Mix = Front<Seat, Back>;
type Fronts<'a> = Videos<FakeClock, FakeBytes, MixDoor<'a, Seat, Back>>;

fn fronts<'a>(mix: &'a Mix, clip: &[u8]) -> Fronts<'a> {
    holding_with(MixDoor::new(mix), clip, 0, Choices::default())
}

/// A WAV file of 16-bit mono at the mix's rate.
fn wave(samples: &[i16]) -> Vec<u8> {
    let data: Vec<u8> = samples
        .iter()
        .flat_map(|sample| sample.to_le_bytes())
        .collect();
    let size = u32::try_from(data.len()).expect("a test's sound fits");
    let mut file = Vec::new();
    file.extend_from_slice(b"RIFF");
    file.extend_from_slice(&(36 + size).to_le_bytes());
    file.extend_from_slice(b"WAVEfmt ");
    file.extend_from_slice(&16u32.to_le_bytes());
    file.extend_from_slice(&1u16.to_le_bytes());
    file.extend_from_slice(&1u16.to_le_bytes());
    file.extend_from_slice(&MIX_HZ.to_le_bytes());
    file.extend_from_slice(&(MIX_HZ * 2).to_le_bytes());
    file.extend_from_slice(&2u16.to_le_bytes());
    file.extend_from_slice(&16u16.to_le_bytes());
    file.extend_from_slice(b"data");
    file.extend_from_slice(&size.to_le_bytes());
    file.extend_from_slice(&data);
    file
}

/// Two mono frames at the mix's rate, 2 samples each.
fn two_frames() -> Vec<u8> {
    samples_clip(MIX_HZ, RATE, 1, &[vec![100, 200], vec![300, 400]])
}

/// Hand-worked, as `sound-mix`'s own door test: 46 us is 2 frames summed with the
/// voice's 1,000, then Miles' close, then 92 us is 2 frames of the clip alone.
#[test]
fn miles_close_ends_its_voice_and_leaves_the_clips_sound() {
    let (seat, back) = (Seat::default(), Back::default());
    let mix = Front::new(&ROW, seat.clone(), back.clone());
    let (answer, driver) = mix.wave_out_open(0, 11_025, 1, 8);
    assert_eq!(answer, 0);
    let voice = mix.allocate_file_sample(driver, 0x10, &wave(&[1_000; 2]), 0);
    mix.set_sample_loop_count(voice, 0);

    let bytes = two_frames();
    let mut videos = fronts(&mix, &bytes);
    let handle = videos.smack_open(b"a.smk", ALL_7, 0);
    videos.smack_do_frame(handle);
    mix.start_sample(voice);
    seat.at(46);
    mix.pump();

    mix.wave_out_close(driver);
    assert_eq!(mix.sample_status(voice), SMP_DONE);
    assert_eq!(back.closes(), 0, "the clip's door keeps the output open");

    videos.smack_next_frame(handle);
    videos.smack_do_frame(handle);
    seat.at(92);
    mix.pump();
    assert_eq!(
        back.frames(),
        [1_100, 1_100, 1_200, 1_200, 300, 300, 400, 400]
    );

    videos.smack_close(handle);
    assert_eq!(
        back.closes(),
        1,
        "the clip's close takes the output with it"
    );
}

/// The order the one caller read makes: open naming all 7 tracks, ask, close, close
/// Miles, reopen, play.
#[test]
fn the_callers_whole_order_plays_every_sample() {
    let (seat, back) = (Seat::default(), Back::default());
    let mix = Front::new(&ROW, seat.clone(), back.clone());
    let (_, driver) = mix.wave_out_open(0, 11_025, 1, 8);
    let voice = mix.allocate_file_sample(driver, 0x10, &wave(&[1_000; 2]), 0);
    mix.set_sample_loop_count(voice, 0);
    mix.start_sample(voice);

    let bytes = two_frames();
    let mut videos = fronts(&mix, &bytes);
    let lines = debug_lines(|| {
        let asked = videos.smack_open(b"a.smk", ALL_7, 0);
        assert_ne!(videos.smack_sound_in_track(asked, TRACK_1), 0);
        videos.smack_close(asked);
        mix.wave_out_close(driver);
        assert_eq!(mix.sample_status(voice), SMP_DONE);
        assert_eq!(back.closes(), 1, "no door and no driver leave no output");

        let handle = videos.smack_open(b"a.smk", ALL_7, 0);
        assert_eq!(back.opens(), 2, "the clip's door opens it again");
        videos.smack_do_frame(handle);
        seat.at(46);
        mix.pump();
        videos.smack_next_frame(handle);
        videos.smack_do_frame(handle);
        seat.at(92);
        mix.pump();
        videos.smack_close(handle);
    });
    assert_eq!(back.frames(), [100, 100, 200, 200, 300, 300, 400, 400]);
    assert_eq!(back.closes(), 2);
    let doors = lines
        .iter()
        .filter(|one| one.starts_with("sound.door.open"))
        .count();
    assert_eq!(doors, 2, "{lines:?}");
    assert!(!lines.iter().any(|one| one.starts_with("sound.named.stop")));
}

/// Hand-worked: 3 frames of 10,000 microseconds, each 441 samples at the mix's rate,
/// are 1,323 frames, and 30,000 microseconds is 1,323 frames of mix: the last sample
/// goes out with the last frame's end. Nothing sounds, so nothing is written, after.
#[test]
fn the_sound_ends_with_the_last_frame_stepped_on_the_clock() {
    let (seat, back) = (Seat::default(), Back::default());
    let mix = Front::new(&ROW, seat.clone(), back.clone());
    let chunk: Vec<i16> = (0..441).map(|at| 100 + at % 2).collect();
    let bytes = samples_clip(
        MIX_HZ,
        RATE,
        1,
        &[chunk.clone(), chunk.clone(), chunk.clone()],
    );
    let mut videos = fronts(&mix, &bytes);
    let handle = videos.smack_open(b"a.smk", ALL_7, 0);

    let mut now_us = 0;
    for _ in 0..3 {
        videos.clock_mut().0 = now_us;
        seat.at(now_us);
        assert_eq!(videos.smack_wait(handle), 0, "due at {now_us}");
        videos.smack_do_frame(handle);
        videos.smack_next_frame(handle);
        videos.clock_mut().0 = now_us + PERIOD_US - 1;
        assert_eq!(videos.smack_wait(handle), 1, "not due before its period");
        now_us += PERIOD_US;
    }
    seat.at(now_us);
    mix.pump();
    let expected: Vec<i16> = chunk
        .iter()
        .chain(&chunk)
        .chain(&chunk)
        .flat_map(|sample| [*sample, *sample])
        .collect();
    assert_eq!(back.frames().len(), 1_323 * 2);
    assert_eq!(
        back.frames(),
        expected,
        "whole, in order, by the last frame's end"
    );

    seat.at(now_us + PERIOD_US);
    mix.pump();
    assert_eq!(back.frames(), expected, "silence after");
    videos.smack_close(handle);
    assert_eq!(back.closes(), 1);
}

/// 2 samples out by 46 microseconds, the close, and the rest never heard.
#[test]
fn a_close_drops_what_was_not_yet_heard() {
    let (seat, back) = (Seat::default(), Back::default());
    let mix = Front::new(&ROW, seat.clone(), back.clone());
    let bytes = samples_clip(MIX_HZ, RATE, 1, &[vec![100, 200, 300, 400]]);
    let mut videos = fronts(&mix, &bytes);
    let handle = videos.smack_open(b"a.smk", ALL_7, 0);
    videos.smack_do_frame(handle);
    seat.at(46);
    mix.pump();
    videos.smack_close(handle);
    seat.at(1_000);
    mix.pump();
    assert_eq!(back.frames(), [100, 100, 200, 200]);
}
