// SPDX-License-Identifier: MIT
//! Writes built clips out so another program can play them and another decoder can
//! answer for them. Not part of the fence: asked for by name, to a folder named by
//! `VIDEO_PROBE_DIR`, it writes built clips only.
use std::fs;
use std::path::Path;

use super::clips::{long_clip, samples_clip};

/// A pace run's frames: at either rate, a clock 5% fast ends over 2 periods early.
const PACE_FRAMES: usize = 60;
const SOUND_FRAMES: usize = 12;
const SAMPLES_A_FRAME: usize = 64;
const SOUND_HZ: u32 = 22_050;
const SOUND_RATE: i32 = -6666;

/// A track's frames: every sample its own, so one out of place moves a hash.
fn track(channels: usize) -> Vec<Vec<i16>> {
    (0..SOUND_FRAMES)
        .map(|frame| {
            (0..SAMPLES_A_FRAME * channels)
                .map(|at| {
                    let value = (frame * 211 + at * 37) % 20_000;
                    i16::try_from(value).expect("under 20,000") - 10_000
                })
                .collect()
        })
        .collect()
}

#[test]
#[ignore = "writes the built clips out for another program's answers"]
fn write_the_clips() {
    let folder = std::env::var("VIDEO_PROBE_DIR").expect("VIDEO_PROBE_DIR names the folder");
    let clips = [
        ("long-6666", long_clip(PACE_FRAMES, -6666)),
        ("long-7100", long_clip(PACE_FRAMES, -7100)),
        (
            "samples-mono",
            samples_clip(SOUND_HZ, SOUND_RATE, 1, &track(1)),
        ),
        (
            "samples-stereo",
            samples_clip(SOUND_HZ, SOUND_RATE, 2, &track(2)),
        ),
    ];
    for (name, bytes) in clips {
        fs::write(Path::new(&folder).join(format!("{name}.smk")), bytes)
            .expect("the probe folder takes a file");
    }
}
