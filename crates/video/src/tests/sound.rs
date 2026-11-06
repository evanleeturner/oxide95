// SPDX-License-Identifier: MIT
//! The sound on fakes, over built clips: what `smack_sound_in_track` answers and
//! what the door takes.
use std::ptr;

use super::clips::{Chunk, chunk_clip, painting_clip, samples_clip};
use super::fakes::{Mute, holding, holding_with};
use super::test_log::debug_lines;
use crate::{Choices, Handle, Track};

/// The library's bit for a clip's first track.
const TRACK_1: u32 = 0x2000;
/// All 7 tracks' bits.
const ALL_7: u32 = 0xfe000;
const HZ: u32 = 22_050;
const RATE: i32 = -1000;

fn masks() -> impl Iterator<Item = u32> {
    (0..32).map(|bit| 1 << bit).chain([0, ALL_7])
}

/// 5 mono frames, each's samples its own numbers.
fn five_frames() -> Vec<u8> {
    let frames: Vec<Vec<i16>> = (0..5)
        .map(|at| vec![at * 10 + 1, at * 10 + 2, at * 10 + 3, at * 10 + 5])
        .collect();
    samples_clip(HZ, RATE, 1, &frames)
}

/// Frame `at` of [`five_frames`].
fn frame(at: i16) -> Vec<i16> {
    vec![at * 10 + 1, at * 10 + 2, at * 10 + 3, at * 10 + 5]
}

#[test]
fn the_answer_is_1_where_the_mask_names_a_track_the_clip_holds() {
    let bytes = five_frames();
    for flags in [0, 0x200, ALL_7] {
        let mut videos = holding(&bytes, 0, Choices::default());
        let handle = videos.smack_open(b"a.smk", flags, 0);
        for mask in masks() {
            let expected = u32::from(mask & TRACK_1 != 0);
            assert_eq!(
                videos.smack_sound_in_track(handle, mask),
                expected,
                "flags {flags:x} mask {mask:x}"
            );
        }
    }
}

#[test]
fn a_clip_with_no_track_answers_0_for_every_mask_and_takes_no_door() {
    let bytes = painting_clip();
    let mut videos = holding(&bytes, 0, Choices::default());
    let handle = videos.smack_open(b"a.smk", ALL_7, 0);
    for mask in masks() {
        assert_eq!(
            videos.smack_sound_in_track(handle, mask),
            0,
            "mask {mask:x}"
        );
    }
    videos.smack_do_frame(handle);
    videos.smack_close(handle);
    assert_eq!(videos.sound().calls, 0);
}

#[test]
fn a_track_of_no_rate_answers_0_and_opens_no_door() {
    let bytes = samples_clip(0, RATE, 1, &[frame(0)]);
    let mut videos = holding(&bytes, 0, Choices::default());
    let handle = videos.smack_open(b"a.smk", ALL_7, 0);
    assert!(!handle.is_null());
    assert_eq!(videos.smack_sound_in_track(handle, TRACK_1), 0);
    videos.smack_do_frame(handle);
    videos.smack_close(handle);
    assert_eq!(videos.sound().calls, 0);
}

#[test]
fn an_unknown_handle_answers_0_and_is_turned_away_by_name() {
    let bytes = five_frames();
    let mut videos = holding(&bytes, 0, Choices::default());
    let lines = debug_lines(|| {
        assert_eq!(
            videos.smack_sound_in_track(ptr::dangling::<Handle>(), TRACK_1),
            0
        );
    });
    assert!(
        lines
            .iter()
            .any(|one| one == "video.refused c=SMACKW32!SmackSoundInTrack w=unknown-handle"),
        "{lines:?}"
    );
}

#[test]
fn a_door_that_plays_nothing_answers_0_and_takes_no_call() {
    let bytes = five_frames();
    let mut videos = holding_with(Mute::default(), &bytes, 0, Choices::default());
    for flags in [0, 0x200, ALL_7] {
        let handle = videos.smack_open(b"a.smk", flags, 0);
        for mask in masks() {
            assert_eq!(
                videos.smack_sound_in_track(handle, mask),
                0,
                "mask {mask:x}"
            );
        }
        for _ in 0..5 {
            videos.smack_do_frame(handle);
            videos.smack_next_frame(handle);
        }
        videos.smack_close(handle);
    }
    assert_eq!(videos.sound().calls, 0);
}

#[test]
fn flags_naming_no_first_track_open_no_stream() {
    let bytes = five_frames();
    for flags in [0, 0x200, 0x4000] {
        let mut videos = holding(&bytes, 0, Choices::default());
        let handle = videos.smack_open(b"a.smk", flags, 0);
        assert_eq!(videos.smack_sound_in_track(handle, TRACK_1), 1);
        videos.smack_do_frame(handle);
        videos.smack_close(handle);
        assert_eq!(videos.sound().calls, 0, "flags {flags:x}");
    }
}

#[test]
fn do_then_next_pushes_each_frame_to_one_stream_and_closes_it_once() {
    let bytes = five_frames();
    let mut videos = holding(&bytes, 0, Choices::default());
    let handle = videos.smack_open(b"a.smk", ALL_7, 0);
    let track = Track {
        rate_hz: HZ,
        channels: 1,
    };
    assert_eq!(videos.sound().opened, [(0x100, track)]);
    for _ in 0..5 {
        videos.smack_do_frame(handle);
        videos.smack_next_frame(handle);
    }
    videos.smack_close(handle);
    let expected: Vec<_> = (0..5).map(|at| (0x100, frame(at))).collect();
    assert_eq!(videos.sound().pushed, expected);
    assert_eq!(videos.sound().closed, [0x100]);
    assert_eq!(videos.sound().calls, 7);
}

#[test]
fn next_then_do_pushes_the_same() {
    let bytes = five_frames();
    let mut videos = holding(&bytes, 0, Choices::default());
    let handle = videos.smack_open(b"a.smk", ALL_7, 0);
    videos.smack_do_frame(handle);
    for _ in 1..5 {
        videos.smack_next_frame(handle);
        videos.smack_do_frame(handle);
    }
    let expected: Vec<_> = (0..5).map(|at| (0x100, frame(at))).collect();
    assert_eq!(videos.sound().pushed, expected);
}

#[test]
fn a_frame_painted_twice_pushes_once() {
    let bytes = five_frames();
    let mut videos = holding(&bytes, 0, Choices::default());
    let handle = videos.smack_open(b"a.smk", ALL_7, 0);
    videos.smack_do_frame(handle);
    videos.smack_do_frame(handle);
    assert_eq!(videos.sound().pushed, [(0x100, frame(0))]);
}

#[test]
fn a_frame_stepped_past_is_never_pushed() {
    let bytes = five_frames();
    let mut videos = holding(&bytes, 0, Choices::default());
    let handle = videos.smack_open(b"a.smk", ALL_7, 0);
    videos.smack_do_frame(handle);
    videos.smack_next_frame(handle);
    videos.smack_next_frame(handle);
    videos.smack_do_frame(handle);
    assert_eq!(
        videos.sound().pushed,
        [(0x100, frame(0)), (0x100, frame(2))]
    );
}

#[test]
fn a_goto_takes_no_door_call_and_the_sound_resumes_where_it_landed() {
    let bytes = five_frames();
    let mut videos = holding(&bytes, 0, Choices::default());
    let handle = videos.smack_open(b"a.smk", ALL_7, 0);
    videos.smack_do_frame(handle);
    let before = videos.sound().calls;
    videos.smack_goto(handle, 4);
    videos.smack_goto(handle, 3);
    assert_eq!(videos.sound().calls, before);
    videos.smack_do_frame(handle);
    videos.smack_goto(handle, 1);
    videos.smack_do_frame(handle);
    assert_eq!(
        videos.sound().pushed,
        [(0x100, frame(0)), (0x100, frame(2)), (0x100, frame(0))]
    );
}

#[test]
fn a_frame_with_no_samples_pushes_nothing() {
    let bytes = samples_clip(HZ, RATE, 1, &[frame(0), Vec::new(), frame(2)]);
    let mut videos = holding(&bytes, 0, Choices::default());
    let handle = videos.smack_open(b"a.smk", ALL_7, 0);
    for _ in 0..3 {
        videos.smack_do_frame(handle);
        videos.smack_next_frame(handle);
    }
    assert_eq!(
        videos.sound().pushed,
        [(0x100, frame(0)), (0x100, frame(2))]
    );
}

#[test]
fn a_refused_chunk_pushes_nothing_and_a_clip_not_playing_never_reads_it() {
    let bytes = chunk_clip(HZ, RATE, 1, &[Chunk::TooLarge, Chunk::Samples(frame(1))]);
    let mut videos = holding(&bytes, 0, Choices::default());
    let handle = videos.smack_open(b"a.smk", ALL_7, 0);
    let lines = debug_lines(|| {
        videos.smack_do_frame(handle);
    });
    assert!(
        lines
            .iter()
            .any(|one| one == "video.refused c=SMACKW32!SmackDoFrame w=decoder"),
        "{lines:?}"
    );
    assert!(videos.sound().pushed.is_empty());
    videos.smack_next_frame(handle);
    videos.smack_do_frame(handle);
    assert_eq!(videos.sound().pushed, [(0x100, frame(1))]);

    let silent = videos.smack_open(b"a.smk", 0, 0);
    let lines = debug_lines(|| {
        videos.smack_do_frame(silent);
    });
    assert!(
        !lines.iter().any(|one| one.contains("w=decoder")),
        "{lines:?}"
    );
}

#[test]
fn a_stereo_track_is_pushed_interleaved_and_named_so() {
    let pair = vec![1, 2, 3, 4, 7, 9];
    let bytes = samples_clip(HZ, RATE, 2, std::slice::from_ref(&pair));
    let mut videos = holding(&bytes, 0, Choices::default());
    let handle = videos.smack_open(b"a.smk", ALL_7, 0);
    videos.smack_do_frame(handle);
    assert_eq!(
        videos.sound().opened,
        [(
            0x100,
            Track {
                rate_hz: HZ,
                channels: 2
            }
        )]
    );
    assert_eq!(videos.sound().pushed, [(0x100, pair)]);
}

#[test]
fn an_unknown_handle_at_close_calls_no_door() {
    let bytes = five_frames();
    let mut videos = holding(&bytes, 0, Choices::default());
    let handle = videos.smack_open(b"a.smk", ALL_7, 0);
    videos.smack_close(ptr::dangling::<Handle>());
    assert_eq!(videos.sound().calls, 1, "the open alone");
    videos.smack_close(handle);
    assert_eq!(videos.sound().closed, [0x100]);
}

#[test]
fn two_clips_at_once_keep_their_own_streams() {
    let bytes = five_frames();
    let mut videos = holding(&bytes, 0, Choices::default());
    let first = videos.smack_open(b"a.smk", ALL_7, 0);
    let second = videos.smack_open(b"a.smk", ALL_7, 0);
    videos.smack_next_frame(second);
    videos.smack_do_frame(second);
    videos.smack_do_frame(first);
    videos.smack_close(first);
    videos.smack_close(second);
    assert_eq!(
        videos.sound().pushed,
        [(0x101, frame(1)), (0x100, frame(0))]
    );
    assert_eq!(videos.sound().closed, [0x100, 0x101]);
}

#[test]
fn a_stream_open_and_close_each_write_a_line() {
    let bytes = five_frames();
    let mut videos = holding(&bytes, 0, Choices::default());
    let lines = debug_lines(|| {
        let handle = videos.smack_open(b"a.smk", ALL_7, 0);
        videos.smack_close(handle);
    });
    let streams: Vec<_> = lines
        .iter()
        .filter(|one| one.starts_with("video.stream"))
        .collect();
    assert_eq!(
        streams,
        [
            "video.stream.open s=256 r=22050 c=1",
            "video.stream.close s=256"
        ]
    );
}
