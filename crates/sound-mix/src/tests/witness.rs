// SPDX-License-Identifier: MIT
//! The witness through the real front, on a driven clock. Every expected number was
//! worked by hand before the code first ran: the mix runs at 44100, so 10 ms is 441
//! frames and 5 readings 2205, which is `89d`. A voice's frame `n` holds `n` left and
//! `-n` right. The fixed list's hash came from a second program, never from this crate.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use super::{little, pcm, rig};
use crate::output::{NullBack, Output};
use crate::test_log::debug_lines;
use crate::witness::{HASH_START, Seen, Summary, Witness, hash_bytes, hash_samples};

const FIXED_LIST: [i16; 6] = [1, -1, 32_767, -32_768, 0, 256];
const FIXED_LIST_HASH: &str = "5ba47c5c86047d13";

fn numbered_samples(count: i16) -> Vec<i16> {
    (1..=count).flat_map(|n| [n, -n]).collect()
}

fn file_of(samples: &[i16]) -> Vec<u8> {
    pcm(2, 44_100, 16, &little(samples))
}

/// A back that keeps what it is handed, with a room the test can move.
#[derive(Clone)]
struct Keep {
    samples: Arc<Mutex<Vec<i16>>>,
    room: Arc<AtomicUsize>,
}

impl Keep {
    fn new() -> Self {
        Self {
            samples: Arc::default(),
            room: Arc::new(AtomicUsize::new(usize::MAX)),
        }
    }

    fn kept(&self) -> Vec<i16> {
        self.samples
            .lock()
            .expect("no test panics holding it")
            .clone()
    }
}

impl Output for Keep {
    fn open(&mut self, rate_hz: u32) -> u32 {
        rate_hz
    }

    fn room(&self) -> usize {
        self.room.load(Ordering::Relaxed)
    }

    fn write(&mut self, frames: &[i16]) {
        self.samples
            .lock()
            .expect("no test panics holding it")
            .extend_from_slice(frames);
    }

    fn close(&mut self) {}
}

/// A back whose every answer is its own: a rate other than asked, and a small room.
struct Odd;

impl Output for Odd {
    fn open(&mut self, _rate_hz: u32) -> u32 {
        22_050
    }

    fn room(&self) -> usize {
        7
    }

    fn write(&mut self, _frames: &[i16]) {}

    fn close(&mut self) {}
}

/// 5 readings of 10 ms over `file`; `before` runs ahead of each, numbered from 1.
fn five_readings<O: Output>(back: O, file: &[u8], before: impl Fn(u32)) -> Seen {
    let witness = Witness::new(back);
    let seen = witness.seen();
    let (seat, front, driver) = rig(witness);
    let handle = front.allocate_file_sample(driver, 0x10, file, 0);
    front.start_sample(handle);
    for reading in 1..=5 {
        before(reading);
        seat.at(u64::from(reading) * 10_000);
        front.pump();
    }
    seen
}

fn summary_line(seen: &Seen) -> String {
    let lines = debug_lines(|| seen.log_summary());
    lines
        .into_iter()
        .find(|line| line.starts_with("sound.witness.summary "))
        .expect("the summary has its line")
}

#[test]
fn the_hash_is_fnv_1a_64_by_a_second_programs_answers() {
    assert_eq!(format!("{HASH_START:016x}"), "cbf29ce484222325");
    assert_eq!(
        format!("{:016x}", hash_bytes(HASH_START, b"a".iter().copied())),
        "af63dc4c8601ec8c"
    );
    assert_eq!(
        format!("{:016x}", hash_samples(HASH_START, &FIXED_LIST)),
        FIXED_LIST_HASH
    );
}

#[test]
fn one_stream_cut_three_ways_reads_one_hash() {
    let whole = hash_samples(HASH_START, &FIXED_LIST);
    for cut in [1, 2, 5] {
        let (first, rest) = FIXED_LIST.split_at(cut);
        assert_eq!(hash_samples(hash_samples(HASH_START, first), rest), whole);
    }
}

#[test]
fn the_back_behind_it_holds_every_sample_the_mix_wrote_in_order() {
    let back = Keep::new();
    let samples = numbered_samples(3000);
    let seen = five_readings(back.clone(), &file_of(&samples), |_| {});
    let kept = back.kept();
    assert_eq!(
        kept,
        samples[..2205 * 2],
        "the first 2205 frames, untouched"
    );
    let summary = seen.summary();
    assert_eq!(summary.frames, 2205);
    assert_eq!(summary.blocks, 5);
    assert_eq!(summary.hash, hash_samples(HASH_START, &kept));
}

#[test]
fn every_answer_is_the_backs_own() {
    let mut witness = Witness::new(Odd);
    assert_eq!(witness.open(44_100), 22_050);
    assert_eq!(witness.room(), 7);
}

#[test]
fn a_planted_fault_shows_in_the_summary() {
    let samples = numbered_samples(3000);
    let file = file_of(&samples);
    let clean = five_readings(NullBack, &file, |_| {});
    let again = five_readings(NullBack, &file, |_| {});
    assert_eq!(clean.summary(), again.summary(), "clean reads as clean");
    assert_eq!(summary_line(&clean), summary_line(&again));
    assert!(
        summary_line(&clean).starts_with("sound.witness.summary frames=89d blocks=5 hash="),
        "{}",
        summary_line(&clean)
    );

    let mut flipped = samples.clone();
    flipped[100] = -flipped[100];
    let flipped = five_readings(NullBack, &file_of(&flipped), |_| {});
    let Summary { frames, hash, .. } = flipped.summary();
    assert_eq!(frames, 2205, "a flipped sample moves no count");
    assert_ne!(
        hash,
        clean.summary().hash,
        "a flipped sample moves the hash"
    );
    assert_ne!(summary_line(&flipped), summary_line(&clean));

    let back = Keep::new();
    let room = back.room.clone();
    let missing = five_readings(back, &file, |reading| {
        let open = if reading == 3 { 0 } else { usize::MAX };
        room.store(open, Ordering::Relaxed);
    });
    let summary = missing.summary();
    assert_eq!(summary.frames, 2205 - 441, "one reading's block never came");
    assert_eq!(summary.blocks, 4);
    assert_ne!(summary.hash, clean.summary().hash);
    assert_ne!(summary_line(&missing), summary_line(&clean));
}

#[cfg(feature = "page")]
mod over_the_page {
    use super::{HASH_START, Witness, file_of, hash_samples, numbered_samples, rig};
    use crate::page::Page;

    /// The clock stands at 20 ms at the gesture, so 441 and 441 are dropped: 883 is the
    /// first frame kept, and 10 ms more are 441.
    #[test]
    fn before_the_gesture_it_counts_nothing_and_after_it_agrees_with_the_ring() {
        let page = Page::new();
        let witness = Witness::new(page.clone());
        let seen = witness.seen();
        let (seat, front, driver) = rig(witness);
        let file = file_of(&numbered_samples(3000));
        let handle = front.allocate_file_sample(driver, 0x10, &file, 0);
        front.start_sample(handle);
        seat.at(10_000);
        front.pump();
        assert_eq!(seen.summary().frames, 0, "room 0: nothing reached the back");
        seat.at(20_000);
        page.start(&front);
        assert_eq!(
            seen.summary().frames,
            0,
            "the start's own pump keeps nothing"
        );
        seat.at(30_000);
        front.pump();
        assert_eq!(seen.summary().frames, 441);
        assert_eq!(seen.summary().frames, page.kept());
        let mut into = vec![0; 2000];
        let given = page.read(&mut into);
        assert_eq!(given, 441);
        assert_eq!(into[..2], [883, -883]);
        assert_eq!(
            hash_samples(HASH_START, &into[..given * 2]),
            seen.summary().hash
        );
    }
}
