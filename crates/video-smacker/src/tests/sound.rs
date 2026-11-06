// SPDX-License-Identifier: MIT
//! A track's samples, its forms and the chunks refused.
use super::fixtures::{HZ, sound_chunks, sound_clip};
use super::test_log::debug_lines;
use super::writer::sound_from_bits;
use crate::sound::unpack;
use crate::{Clip, Part, Picture, Refused, Track};

/// Each frame's samples, in order, painting as it goes.
fn samples_by_frame(bytes: &[u8]) -> Vec<Vec<i16>> {
    let mut clip = Clip::parse(bytes).expect("a fixture parses");
    let mut picture = Picture::new();
    (0..clip.header().frames)
        .map(|_| {
            let samples = clip.frame_samples().expect("a fixture's sound unpacks");
            clip.decode_next_frame(&mut picture)
                .expect("a fixture's frame paints");
            samples
        })
        .collect()
}

/// Whole tracks come back, interleaved left then right when stereo, whatever
/// a delta's size and across a 16-bit wrap; frames with no sound give none.
#[test]
fn a_track_unpacks_to_its_own_samples() {
    for channels in [1, 2] {
        let [first, second, third] = sound_chunks(channels);
        assert_eq!(
            samples_by_frame(&sound_clip(channels)),
            [first, second, vec![], vec![], third],
            "{channels} channels"
        );
    }
}

/// The header says what the track is.
#[test]
fn the_header_says_the_tracks_form() {
    for channels in [1, 2u8] {
        let clip = Clip::parse(&sound_clip(usize::from(channels))).expect("a fixture parses");
        let track = clip.header().track.expect("the clip declares a track");
        assert_eq!(
            (track.channels, track.hz, track.largest_unpacked),
            (channels, HZ, 12 * u32::from(channels))
        );
    }
}

/// Reading a frame's sound leaves the cursor, and says how much it read.
#[test]
fn reading_sound_stays_on_the_frame_and_says_how_many_samples() {
    let clip = Clip::parse(&sound_clip(2)).expect("a fixture parses");
    let lines = debug_lines(|| {
        assert_eq!(clip.frame_samples(), clip.frame_samples());
    });
    assert_eq!(lines, ["smk.sound i=0 n=12", "smk.sound i=0 n=12"]);
    assert_eq!(clip.cursor(), 0);
}

const STEREO_TRACK: Track = Track {
    channels: 2,
    hz: HZ,
    largest_unpacked: 16,
};

/// A chunk's first byte: present, stereo and 16-bit flags, then 0s.
const fn flags(present: bool, stereo: bool, sixteen: bool) -> u8 {
    (present as u8) | ((stereo as u8) << 1) | ((sixteen as u8) << 2)
}

/// A chunk whose form does not fit its track, whose length does not, or whose
/// bits run out, is refused by name.
#[test]
fn a_chunk_that_does_not_fit_its_track_is_refused_by_name() {
    let cases: [(u32, u8, Refused); 6] = [
        (20, flags(true, true, true), Refused::SoundTooLarge),
        (3, flags(true, true, true), Refused::SoundChunk),
        (6, flags(true, true, true), Refused::SoundChunk),
        (8, flags(true, false, true), Refused::SoundChunk),
        (8, flags(true, true, false), Refused::EightBitSound),
        (8, flags(true, true, true), Refused::Truncated(Part::Sound)),
    ];
    for (length, first, refused) in cases {
        let chunk = sound_from_bits(length, &[first]);
        assert_eq!(
            unpack(&chunk[4..], &STEREO_TRACK),
            Err(refused),
            "{length} {first}"
        );
    }
    assert_eq!(
        unpack(&[1], &STEREO_TRACK),
        Err(Refused::Truncated(Part::Sound))
    );
}

/// A chunk whose first bit says no data gives no samples, whatever it states.
#[test]
fn a_chunk_saying_no_data_gives_no_samples() {
    let chunk = sound_from_bits(8, &[flags(false, false, false)]);
    assert_eq!(unpack(&chunk[4..], &STEREO_TRACK), Ok(vec![]));
}
