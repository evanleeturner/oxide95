// SPDX-License-Identifier: MIT
//! Every form with no code behind it stops with its name, and no clip's
//! bytes, cut or damaged, can do worse than a refusal.
use std::collections::BTreeSet;

use super::fixtures::{
    SIXTEEN_PACKED, blocks_clip, long_chain_clip, palette_clip, short_palette_clip, sound_clip,
};
use super::test_log::debug_lines;
use super::writer::{HEADER_BYTES, LARGEST_WORD, TRACK_WORD, set_word};
use crate::{Clip, Part, Picture, Refused};

const WIDTH_WORD: usize = 1;
const HEIGHT_WORD: usize = 2;
const FRAMES_WORD: usize = 3;
const RATE_WORD: usize = 4;
const FLAGS_WORD: usize = 5;

/// `bytes` with one header word changed.
fn with_word(bytes: &[u8], word: usize, value: u32) -> Vec<u8> {
    let mut changed = bytes.to_vec();
    set_word(&mut changed, word, value);
    changed
}

fn parse_refusal(bytes: &[u8]) -> Option<Refused> {
    Clip::parse(bytes).err()
}

/// Every frame's sound and picture in order; the first refusal ends the clip.
fn read_whole(bytes: &[u8]) -> Result<(), Refused> {
    let mut clip = Clip::parse(bytes)?;
    let mut picture = Picture::new();
    for _ in 0..clip.header().frames {
        clip.frame_samples()?;
        clip.decode_next_frame(&mut picture)?;
    }
    Ok(())
}

fn every_clip() -> Vec<Vec<u8>> {
    vec![
        blocks_clip(),
        long_chain_clip(),
        palette_clip(),
        short_palette_clip(&[0x40, 9]),
        sound_clip(1),
        sound_clip(2),
    ]
}

/// Each signature and flag with no code stops at `parse`, named.
#[test]
fn each_form_with_no_code_is_refused_by_name() {
    let clip = sound_clip(1);
    let mut smk4 = clip.clone();
    smk4[3] = b'4';
    let mut other = clip.clone();
    other[0] = b'X';
    let cases = [
        (smk4, Refused::Smk4),
        (other, Refused::NotSmacker),
        (with_word(&clip, FLAGS_WORD, 1), Refused::RingFrame),
        (with_word(&clip, FLAGS_WORD, 2), Refused::Interlaced),
        (with_word(&clip, FLAGS_WORD, 4), Refused::Doubled),
        (with_word(&clip, FLAGS_WORD, 8), Refused::UnknownFlags),
        (with_word(&clip, RATE_WORD, 0), Refused::Rate),
        (with_word(&clip, RATE_WORD, 1000), Refused::Rate),
        (with_word(&clip, RATE_WORD, 0x8000_0000), Refused::Rate),
        (with_word(&clip, WIDTH_WORD, 0), Refused::SizeNotBlocks),
        (with_word(&clip, HEIGHT_WORD, 6), Refused::SizeNotBlocks),
        (with_word(&clip, FRAMES_WORD, 0), Refused::NoFrames),
    ];
    for (bytes, refused) in cases {
        assert_eq!(parse_refusal(&bytes), Some(refused), "{}", refused.name());
    }
}

/// A size past what any clip comes near takes no memory: it stops at `parse`.
#[test]
fn a_size_past_the_limit_is_refused_before_any_memory_is_taken() {
    let clip = with_word(&sound_clip(1), WIDTH_WORD, 1 << 16);
    let clip = with_word(&clip, HEIGHT_WORD, 1 << 16);
    assert_eq!(parse_refusal(&clip), Some(Refused::TooLarge));
    let clip = with_word(&sound_clip(1), LARGEST_WORD, (1 << 24) + 1);
    assert_eq!(parse_refusal(&clip), Some(Refused::TooLarge));
}

/// Each track form with no code stops at `parse`, named.
#[test]
fn each_track_form_with_no_code_is_refused_by_name() {
    let clip = sound_clip(1);
    let present = 1 << 30;
    let cases = [
        (SIXTEEN_PACKED & !(1 << 29), Refused::EightBitSound),
        (SIXTEEN_PACKED | (1 << 26), Refused::BinkAudio),
        (SIXTEEN_PACKED | (1 << 27), Refused::BinkAudio),
        (present | (1 << 29), Refused::RawSound),
    ];
    for (form, refused) in cases {
        let bytes = with_word(&clip, TRACK_WORD, form);
        assert_eq!(parse_refusal(&bytes), Some(refused), "{}", refused.name());
    }
    let second = with_word(&clip, TRACK_WORD + 1, present);
    assert_eq!(parse_refusal(&second), Some(Refused::SecondTrack));
}

/// A frame's type byte may not claim sound the header never declared, nor a
/// track past the first.
#[test]
fn a_frame_type_past_the_headers_word_is_refused() {
    let silent = blocks_clip();
    let types_at = HEADER_BYTES + 3 * 4;
    let mut says_sound = silent.clone();
    says_sound[types_at] |= 2;
    assert_eq!(parse_refusal(&says_sound), Some(Refused::SoundWithoutTrack));
    let mut second_track = silent;
    second_track[types_at] |= 4;
    assert_eq!(parse_refusal(&second_track), Some(Refused::SecondTrack));
}

/// The refusal's line carries its name.
#[test]
fn a_refusal_writes_its_name_to_the_log() {
    let mut smk4 = blocks_clip();
    smk4[3] = b'4';
    let lines = debug_lines(|| {
        assert_eq!(parse_refusal(&smk4), Some(Refused::Smk4));
    });
    assert_eq!(lines, ["smk.refused w=smk4"]);
}

/// A size table claiming less than a chunk needs is refused at the frame
/// that reads it, by the chunk's own name.
#[test]
fn a_frame_too_short_for_its_chunk_names_the_chunk() {
    let sized = |bytes: &[u8], frames: usize, frame: usize, size: u32| {
        let mut changed = bytes.to_vec();
        changed[HEADER_BYTES + frame * 4..HEADER_BYTES + frame * 4 + 4]
            .copy_from_slice(&size.to_le_bytes());
        assert!(frame < frames);
        changed
    };
    let palette_cut = sized(&palette_clip(), 4, 0, 4);
    assert_eq!(
        read_whole(&palette_cut),
        Err(Refused::Truncated(Part::Palette))
    );
    let sound_cut = sized(&sound_clip(1), 5, 1, 4);
    assert_eq!(read_whole(&sound_cut), Err(Refused::Truncated(Part::Sound)));
    let video_cut = sized(&blocks_clip(), 3, 1, 0);
    assert_eq!(read_whole(&video_cut), Err(Refused::Truncated(Part::Video)));
}

/// 100 cuts evenly over each whole clip, header, tables, trees and frames:
/// each one a refusal naming the part that ran out.
#[test]
fn a_hundred_cuts_over_a_clip_each_name_the_part_that_ran_out() {
    for clip in every_clip() {
        let mut parts = BTreeSet::new();
        for point in 1..=100 {
            let cut = &clip[..clip.len() * point / 101];
            let refused = parse_refusal(cut).expect("a cut clip is refused at parse");
            assert!(matches!(refused, Refused::Truncated(_)), "{refused}");
            parts.insert(refused.name());
        }
        for part in ["truncated-header", "truncated-frames"] {
            assert!(parts.contains(part), "{parts:?}");
        }
    }
    assert_eq!(parse_refusal(&[]), Some(Refused::Truncated(Part::Header)));
}

/// Whatever bytes are damaged, a clip reads whole or is refused; a panic
/// would fail this test, and nothing here catches one.
#[test]
fn damaged_bytes_end_in_a_frame_or_a_refusal() {
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    let mut outcomes = BTreeSet::new();
    for clip in every_clip() {
        for _ in 0..2000 {
            let mut damaged = clip.clone();
            for _ in 0..4 {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                let at =
                    usize::try_from(state % damaged.len() as u64).expect("a clip's length fits");
                damaged[at] ^= state.to_le_bytes()[7] | 1;
            }
            let answer = read_whole(&damaged);
            outcomes.insert(answer.map_or_else(Refused::name, |()| "read-whole"));
        }
    }
    assert!(outcomes.len() > 5, "{outcomes:?}");
}
