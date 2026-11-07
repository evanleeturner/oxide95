// SPDX-License-Identifier: MIT
//! On the fake clock: integers, stepped as a caller's loop steps.
use super::clips::long_clip;
use super::fakes::{Buffer, Fakes, holding};
use crate::{Choices, Handle};

const START_US: u64 = 5_000_000;

fn playing(frames: usize, rate: i32) -> (Fakes, *mut Handle, Buffer) {
    let mut videos = holding(&long_clip(frames, rate), START_US, Choices::default());
    let mut buffer = Buffer::sized(16, 8);
    let handle = videos.smack_open(b"a.smk", 0, 0);
    buffer.name_to(&mut videos, handle);
    (videos, handle, buffer)
}

#[test]
fn a_run_takes_its_frames_times_its_rate_within_one_frame() {
    for (rate, period) in [(-6666, 66_660), (-7100, 71_000)] {
        for step in [1_000, 13_000, 33_333] {
            let frames = 60u64;
            let (mut videos, handle, _buffer) = playing(60, rate);
            let mut shown = 0;
            while shown < frames {
                if videos.smack_wait(handle) == 0 {
                    videos.smack_do_frame(handle);
                    videos.smack_next_frame(handle);
                    shown += 1;
                }
                videos.clock_mut().0 += step;
            }
            let run = videos.clock_mut().0 - START_US;
            assert!(
                run.abs_diff(frames * period) <= period,
                "rate {rate} step {step}: {run} against {}",
                frames * period
            );
        }
    }
}

#[test]
fn the_first_frame_is_due_at_the_open_and_the_next_a_period_on() {
    let (mut videos, handle, _buffer) = playing(3, -6666);
    assert_eq!(videos.smack_wait(handle), 0);
    videos.smack_do_frame(handle);
    videos.smack_next_frame(handle);
    videos.clock_mut().0 = START_US + 66_659;
    assert_eq!(videos.smack_wait(handle), 1);
    videos.clock_mut().0 = START_US + 66_660;
    assert_eq!(videos.smack_wait(handle), 0);
}

#[test]
fn a_stall_of_100_periods_gives_one_frame_due_not_100() {
    let period = 66_660;
    let (mut videos, handle, _buffer) = playing(200, -6666);
    videos.smack_do_frame(handle);
    videos.smack_next_frame(handle);
    videos.clock_mut().0 += 100 * period;
    let mut due = 0;
    for _ in 0..100 {
        if videos.smack_wait(handle) == 0 {
            due += 1;
            videos.smack_do_frame(handle);
            videos.smack_next_frame(handle);
        }
    }
    assert_eq!(due, 1);
    videos.clock_mut().0 += period - 1;
    assert_eq!(videos.smack_wait(handle), 1);
    videos.clock_mut().0 += 1;
    assert_eq!(videos.smack_wait(handle), 0);
}

#[test]
fn late_by_a_period_or_less_keeps_the_anchor() {
    let period = 66_660;
    let (mut videos, handle, _buffer) = playing(10, -6666);
    videos.smack_do_frame(handle);
    videos.smack_next_frame(handle);
    videos.clock_mut().0 += 2 * period;
    assert_eq!(videos.smack_wait(handle), 0, "frame 1 is one period late");
    videos.smack_do_frame(handle);
    videos.smack_next_frame(handle);
    assert_eq!(videos.smack_wait(handle), 0, "frame 2 is due on the anchor");
}

#[test]
fn a_goto_re_anchors() {
    let period = 66_660;
    let (mut videos, handle, _buffer) = playing(10, -6666);
    videos.clock_mut().0 += 3 * period;
    videos.smack_goto(handle, 6);
    assert_eq!(videos.smack_wait(handle), 0, "the frame gone to is due now");
    videos.smack_do_frame(handle);
    videos.smack_next_frame(handle);
    assert_eq!(videos.smack_wait(handle), 1, "the next a period on");
    videos.clock_mut().0 += period;
    assert_eq!(videos.smack_wait(handle), 0);
}
