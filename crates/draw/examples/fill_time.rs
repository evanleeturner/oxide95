// SPDX-License-Identifier: MIT
//! Times the scaled fill on this machine: a 640 by 480 picture into a 2560
//! by 1440 surface at the whole factor (3, centred, black around), 100 fills,
//! each printed in microseconds, then the median, the fastest and the
//! slowest; the same 100 for the corner fill beside it. Run it with
//! `--release`: a debug build times its own checks, not the fill.

use std::hint::black_box;
use std::time::{Duration, Instant};

use draw::easel::{Palette, Scale, Shown};

const RUNS: usize = 100;
const SURFACE: (u32, u32) = (2560, 1440);

fn main() {
    let shown = picture(640, 480);
    let words = usize::try_from(SURFACE.0 * SURFACE.1).expect("the surface fits in memory");
    let mut surface = vec![0_u32; words];

    let mut fit = None;
    let scaled = timed(|| {
        fit = shown.to_zero_rgb_scaled(black_box(&mut surface), SURFACE.0, Scale::Whole);
    });
    let corner = timed(|| shown.to_zero_rgb(black_box(&mut surface), SURFACE.0));

    println!(
        "picture 640x480 surface {}x{} fit {fit:?}",
        SURFACE.0, SURFACE.1
    );
    for (run, time) in scaled.iter().enumerate() {
        println!("scaled {run} {:.1} us", micros(*time));
    }
    report("scaled", scaled);
    report("corner", corner);
}

/// A picture each of whose pixels differs from its neighbours, none black,
/// its rows 3 bytes of padding apart: the shape the fill meets in a game.
fn picture(width: u32, height: u32) -> Shown {
    let mut palette = Palette([0; 256]);
    for (index, entry) in (0_u32..).zip(palette.0.iter_mut()) {
        *entry = 0x80_0000 | (index * 0x101);
    }
    let pitch = width + 3;
    let pixels = (0..height)
        .flat_map(|y| {
            (0..pitch).map(move |x| {
                if x < width {
                    (x + 3 * y).to_le_bytes()[0]
                } else {
                    0xee
                }
            })
        })
        .collect();
    Shown {
        width,
        height,
        pitch,
        pixels,
        palette,
    }
}

/// `RUNS` calls of `fill`, each one's time.
fn timed(mut fill: impl FnMut()) -> Vec<Duration> {
    (0..RUNS)
        .map(|_| {
            let start = Instant::now();
            fill();
            start.elapsed()
        })
        .collect()
}

/// One line: the median of `times` (the mean of the middle two), the fastest
/// and the slowest, in microseconds.
fn report(name: &str, mut times: Vec<Duration>) {
    times.sort_unstable();
    let middle = times.len() / 2;
    let median = (times[middle - 1] + times[middle]) / 2;
    println!(
        "{name} runs {} median {:.1} us fastest {:.1} us slowest {:.1} us",
        times.len(),
        micros(median),
        micros(times[0]),
        micros(times[times.len() - 1]),
    );
}

fn micros(time: Duration) -> f64 {
    time.as_secs_f64() * 1e6
}
