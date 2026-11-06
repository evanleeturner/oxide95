// SPDX-License-Identifier: MIT
//! The page's back through the real front, on a driven clock. Every expected number
//! was worked by hand before the code first ran: the mix runs at 44100, so 10 ms is
//! 441 frames, and a voice's frame `n` holds `n` left and `-n` right at its default
//! volume and pan, so a frame read names the frame it was.

use std::sync::Arc;

use super::{ROW, Seat, little, pcm, rig};
use crate::page::Page;
use crate::ring::RING_FRAMES;
use crate::test_log::debug_lines;
use crate::{Front, SMP_DONE, SMP_PLAYING};

/// `count` stereo frames at the mix's rate, numbered from 1.
fn numbered(count: i16) -> Vec<u8> {
    let samples: Vec<i16> = (1..=count).flat_map(|n| [n, -n]).collect();
    pcm(2, 44_100, 16, &little(&samples))
}

fn dropped_in(lines: &[String]) -> Vec<u64> {
    lines
        .iter()
        .filter_map(|line| line.strip_prefix("sound.frames.dropped frames="))
        .map(|rest| {
            let number = rest.split(' ').next().expect("a line has a number");
            u64::from_str_radix(number, 16).expect("frames are hex")
        })
        .collect()
}

/// 4410 frames are 100 ms: two 50 ms readings render 2205 each, and the second ends it.
#[test]
fn before_the_gesture_a_sample_ends_unheard_and_every_frame_is_counted() {
    let page = Page::new();
    let (seat, front, driver) = rig(page.clone());
    let handle = front.allocate_file_sample(driver, 0x10, &numbered(4410), 0);
    front.register_eos_callback(handle, 0x77);
    front.start_sample(handle);
    let lines = debug_lines(|| {
        seat.at(50_000);
        assert_eq!(front.sample_status(handle), SMP_PLAYING);
        seat.at(100_000);
        assert_eq!(front.sample_status(handle), SMP_DONE);
    });
    assert_eq!(
        dropped_in(&lines),
        [2205, 2205],
        "the frames rendered, all of them"
    );
    assert_eq!(
        seat.fired(),
        [(0x77, handle)],
        "its callback, as if it had been heard"
    );
    assert_eq!(page.read(&mut [0; 64]), 0);
    assert_eq!(page.kept(), 0);
}

/// At the gesture the clock stands at 20 ms and the last call was at 10: the start's own
/// pump drops those 441, so frame 882 is the last from before it and 883 the first read.
#[test]
fn the_first_frame_read_is_the_first_rendered_after_the_gesture() {
    let page = Page::new();
    let (seat, front, driver) = rig(page.clone());
    let handle = front.allocate_file_sample(driver, 0x10, &numbered(30_000), 0);
    front.start_sample(handle);
    seat.at(10_000);
    front.pump();
    seat.at(20_000);
    let lines = debug_lines(|| page.start(&front));
    assert_eq!(dropped_in(&lines), [441]);
    seat.at(30_000);
    front.pump();
    let mut into = vec![0; 2000];
    assert_eq!(page.read(&mut into), 441);
    assert_eq!(into[..4], [883, -883, 884, -884]);
    assert_eq!(into[880..882], [1323, -1323], "the last of the 441");
    assert_eq!(page.read(&mut into), 0, "nothing comes twice");
    assert_eq!(page.kept(), 441);
}

/// A 500 ms gap renders 22050 frames. The ring takes the oldest 16384, and 22050 less
/// 16384 is 5666 dropped; the next 10 ms go on from frame 22051, where the clock is.
#[test]
fn a_long_gap_fills_the_ring_with_its_oldest_and_counts_the_rest() {
    let page = Page::new();
    let (seat, front, driver) = rig(page.clone());
    let handle = front.allocate_file_sample(driver, 0x10, &numbered(30_000), 0);
    front.start_sample(handle);
    page.start(&front);
    seat.at(500_000);
    let lines = debug_lines(|| front.pump());
    assert_eq!(dropped_in(&lines), [5666]);
    let mut into = vec![0; RING_FRAMES * 2 + 2];
    assert_eq!(page.read(&mut into), RING_FRAMES);
    assert_eq!(into[..2], [1, -1]);
    assert_eq!(
        into[RING_FRAMES * 2 - 2..RING_FRAMES * 2],
        [16_384, -16_384]
    );
    seat.at(510_000);
    front.pump();
    assert_eq!(page.read(&mut into), 441);
    assert_eq!(into[..2], [22_051, -22_051]);
    assert_eq!(page.kept(), 16_384 + 441);
}

#[test]
fn a_read_hands_out_whole_frames_oldest_first() {
    let page = Page::new();
    let (seat, front, driver) = rig(page.clone());
    let handle = front.allocate_file_sample(driver, 0x10, &numbered(441), 0);
    front.start_sample(handle);
    page.start(&front);
    seat.at(10_000);
    front.pump();
    let mut three = [0; 3];
    assert_eq!(page.read(&mut three), 1, "3 numbers hold 1 whole frame");
    assert_eq!(three, [1, -1, 0]);
    assert_eq!(page.read(&mut three), 1);
    assert_eq!(three[..2], [2, -2]);
}

/// Nothing here may stop by name: a gesture can come before any open, twice, or between
/// a close and the next open. A close empties the ring, and the gesture outlives it.
#[test]
fn a_start_before_an_open_twice_and_across_a_close_stops_nothing() {
    let seat = Arc::new(Seat::default());
    let page = Page::new();
    let front = Front::new(&ROW, seat.clone(), page.clone());
    assert_eq!(page.rate_hz(), 0);
    page.start(&front);
    page.start(&front);
    let (_, driver) = front.wave_out_open(0, 11_025, 1, 8);
    assert_eq!(
        page.rate_hz(),
        44_100,
        "the mix's own rate, whatever the program asks"
    );
    let handle = front.allocate_file_sample(driver, 0x10, &numbered(4410), 0);
    front.start_sample(handle);
    seat.at(10_000);
    front.pump();
    let mut into = vec![0; 2000];
    assert_eq!(page.read(&mut into), 441);
    assert_eq!(into[..2], [1, -1]);
    seat.at(20_000);
    front.pump();
    let lines = debug_lines(|| front.wave_out_close(driver));
    assert!(
        lines
            .iter()
            .any(|line| line.starts_with("sound.page.closed frames=1b9")),
        "{lines:?}"
    );
    assert_eq!(page.read(&mut into), 0, "the ring empty at the close");
    let (_, driver) = front.wave_out_open(0, 11_025, 1, 8);
    let handle = front.allocate_file_sample(driver, 0x10, &numbered(4410), 0);
    front.start_sample(handle);
    seat.at(30_000);
    front.pump();
    assert_eq!(page.read(&mut into), 441, "no second gesture asked");
    assert_eq!(
        into[..2],
        [1, -1],
        "the new voice's first, nothing of the old"
    );
}
