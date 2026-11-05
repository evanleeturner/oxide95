// SPDX-License-Identifier: MIT
//! An old game's ticks replayed: the clock fed that game's measured frames
//! and read as a game reads it, whole milliseconds by unsigned
//! subtraction into a carry of 4 ms ticks. The 4 ms tick and the 250 ms
//! clamp are that game's, stated here alone.
//! Also the clock's own rules: a pause, a rebase, a reset, the wrap.
use super::Clock;

/// That game's tick and longest frame, in the clock's units.
const TICK_MILLIS: u32 = 4;
const CLAMP_MICROS: u64 = 250_000;

/// A game's reading of the clock: each read's change by unsigned
/// subtraction, into a carry of whole ticks.
struct Ticker {
    last: u32,
    carry: u32,
    ticks: u64,
}

impl Ticker {
    fn new(clock: &Clock) -> Self {
        Self {
            last: clock.millis(),
            carry: 0,
            ticks: 0,
        }
    }

    fn read(&mut self, clock: &Clock) {
        let now = clock.millis();
        self.carry += now.wrapping_sub(self.last);
        self.last = now;
        self.ticks += u64::from(self.carry / TICK_MILLIS);
        self.carry %= TICK_MILLIS;
    }
}

/// The ticks `frames` frames of `gap` microseconds each count, read once a
/// frame, from a clock at 0.
fn ticks(gap: u64, frames: u64) -> u64 {
    let mut clock = Clock::new(CLAMP_MICROS, 0);
    clock.frame(0);
    let mut ticker = Ticker::new(&clock);
    let mut host = 0;
    for _ in 0..frames {
        host += gap;
        clock.frame(host);
        ticker.read(&clock);
    }
    ticker.ticks
}

#[test]
fn measured_frames_count_their_ticks() {
    // The measured rows: the frame's gap, the frames, the ticks counted.
    let rows: [(u64, u64, u64); 7] = [
        (1, 1_000_000, 250),
        (3_999, 15_004, 15_000),
        (6_944, 8_641, 15_000),
        (7_321, 8_196, 15_000),
        (16_667, 3_600, 15_000),
        (33_333, 1_801, 15_008),
        (1_000_000, 60, 3_750),
    ];
    for (gap, frames, want) in rows {
        assert_eq!(ticks(gap, frames), want, "{frames} frames of {gap} us");
    }
}

#[test]
fn a_pause_moves_nothing_and_resumes_by_one_frame() {
    let mut clock = Clock::new(CLAMP_MICROS, 0);
    clock.frame(0);
    clock.frame(4_000);
    clock.pause();
    clock.frame(10_000);
    clock.frame(5_000_000);
    assert_eq!(clock.micros(), 4_000);
    clock.resume();
    clock.frame(5_004_000);
    assert_eq!(clock.micros(), 8_000);
}

#[test]
fn a_rebase_skips_one_gap() {
    let mut clock = Clock::new(CLAMP_MICROS, 0);
    clock.frame(0);
    clock.frame(4_000);
    clock.rebase();
    clock.frame(9_000_000);
    clock.frame(9_004_000);
    assert_eq!(clock.micros(), 8_000);
}

#[test]
fn a_reset_reads_0_and_skips_the_next_gap() {
    let mut clock = Clock::new(CLAMP_MICROS, 0);
    clock.frame(0);
    clock.frame(200_000);
    clock.reset();
    assert_eq!(clock.millis(), 0);
    clock.frame(900_000);
    assert_eq!(clock.millis(), 0);
}

#[test]
fn a_count_across_the_32_bit_wrap_reads_right() {
    let before_wrap = (u64::from(u32::MAX) - 1) * 1_000;
    let mut clock = Clock::new(CLAMP_MICROS, before_wrap);
    clock.frame(0);
    let before = clock.millis();
    clock.frame(4_000);
    let after = clock.millis();
    assert_eq!((before, after), (u32::MAX - 1, 2));
    assert_eq!(after.wrapping_sub(before), 4);
}

#[test]
fn an_instant_before_the_last_moves_nothing() {
    let mut clock = Clock::new(CLAMP_MICROS, 0);
    clock.frame(8_000);
    clock.frame(4_000);
    assert_eq!(clock.micros(), 0);
}
