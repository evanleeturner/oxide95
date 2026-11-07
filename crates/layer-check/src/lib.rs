// SPDX-License-Identifier: MIT
//! Checks of the computer that run none of this workspace's other code.
//!
//! A fault on a player's screen has three possible causes: the computer, these
//! stand-ins, or the program using them. These checks clear the first, so a later
//! fault in the stand-ins' own checks can name its suspect. The browser's checks are
//! `page/`, plain modules with no build step; the computer's are
//! `examples/computer_check.rs` and `bench/`. This library holds their one piece of
//! logic: making a tone, and judging whether a recording holds it.
#![forbid(unsafe_code)]

use std::f64::consts::TAU;

/// The samples per second of a tone made here and of a recording judged here.
pub const RATE: u32 = 48_000;
/// The share of the sound's power the tone must hold. A relationship, as the mixer may
/// change the level and add a little of its own.
const LEAST_SHARE: f64 = 0.5;

/// What a recording holds, against the tone that was played.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Heard {
    /// The tone, at no less than half the level it was made.
    Tone,
    /// Nothing at all, not one sample off zero.
    Silence,
    /// Some sound, but too quiet or at the wrong pitch.
    Changed,
}

/// A sine wave of `count` samples at [`RATE`], peaking at `amplitude`.
#[must_use]
pub fn tone(hz: f64, count: u32, amplitude: i16) -> Vec<i16> {
    let step = TAU * hz / f64::from(RATE);
    (0..count)
        .map(|index| {
            let level = f64::from(amplitude) * (step * f64::from(index)).sin();
            #[expect(
                clippy::cast_possible_truncation,
                reason = "a sine times an i16 stays inside i16; dropping the fraction is the rounding"
            )]
            let sample = level as i16;
            sample
        })
        .collect()
}

/// The largest distance from zero in `samples`.
#[must_use]
pub fn peak(samples: &[i16]) -> u16 {
    samples
        .iter()
        .map(|sample| sample.unsigned_abs())
        .max()
        .unwrap_or(0)
}

/// The share of the sound's power at one pitch, from 0 to 1, by the Goertzel sum. Silence
/// at either end is left out, as a recording starts before the tone does.
#[must_use]
pub fn share_at(samples: &[i16], hz: f64) -> f64 {
    let Some(first) = samples.iter().position(|&sample| sample != 0) else {
        return 0.0;
    };
    let last = samples
        .iter()
        .rposition(|&sample| sample != 0)
        .unwrap_or(first);
    let coefficient = 2.0 * (TAU * hz / f64::from(RATE)).cos();
    let (mut latest, mut before, mut power, mut count) = (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64);
    for &sample in &samples[first..=last] {
        let level = f64::from(sample);
        let now = level + coefficient * latest - before;
        before = latest;
        latest = now;
        power += level * level;
        count += 1.0;
    }
    let at_pitch = latest * latest + before * before - coefficient * latest * before;
    2.0 * at_pitch / (count * power)
}

/// Whether `samples` hold a tone played at `hz` and made at `amplitude`.
#[must_use]
pub fn judge(samples: &[i16], hz: f64, amplitude: i16) -> Heard {
    let loudest = peak(samples);
    if loudest == 0 {
        Heard::Silence
    } else if loudest >= amplitude.unsigned_abs() / 2 && share_at(samples, hz) >= LEAST_SHARE {
        Heard::Tone
    } else {
        Heard::Changed
    }
}

#[cfg(test)]
mod tests {
    use super::{Heard, RATE, judge, peak, tone};

    const HZ: f64 = 440.0;
    const LEVEL: i16 = 6000;

    #[test]
    fn nothing_and_zeros_read_as_silence() {
        assert_eq!(judge(&[], HZ, LEVEL), Heard::Silence);
        assert_eq!(judge(&[0; 4800], HZ, LEVEL), Heard::Silence);
    }

    #[test]
    fn the_tone_reads_as_itself() {
        assert_eq!(judge(&tone(HZ, RATE, LEVEL), HZ, LEVEL), Heard::Tone);
    }

    #[test]
    fn silence_around_the_tone_does_not_count_against_it() {
        let mut caught = vec![0; 24_000];
        caught.extend(tone(HZ, 24_000, LEVEL));
        caught.extend(vec![0; 24_000]);
        assert_eq!(judge(&caught, HZ, LEVEL), Heard::Tone);
    }

    #[test]
    fn a_wrong_pitch_never_reads_as_the_tone() {
        for wrong in [220.0, 657.0, 880.0, 1000.0] {
            assert_eq!(
                judge(&tone(wrong, RATE, LEVEL), HZ, LEVEL),
                Heard::Changed,
                "{wrong} Hz"
            );
        }
    }

    #[test]
    fn the_peak_limit_is_half_the_level_made() {
        let half = LEVEL / 2;
        assert_eq!(judge(&tone(HZ, RATE, half), HZ, LEVEL), Heard::Tone);
        assert_eq!(
            judge(&tone(HZ, RATE, half - 200), HZ, LEVEL),
            Heard::Changed
        );
    }

    #[test]
    fn peak_reads_the_most_negative_sample_too() {
        assert_eq!(peak(&[3, i16::MIN, 7]), 32_768);
    }
}
