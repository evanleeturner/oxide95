// SPDX-License-Identifier: MIT
//! The clips these tests read, built here by the decoder's test-only writer.
use super::writer::{
    Frame, SOLID, Trees, VOID, clip, kind, palette_chunk, set_word, sound_chunk, sound_from_bits,
};

/// Markers for a kinds tree, kind words no clip here paints with.
const KIND_MARKS: [u16; 3] = [0x00f4, 0x00f8, 0x00fc];
/// The header word of the frame rate.
const RATE_WORD: usize = 4;
const SIXTEEN_PACKED: u32 = (1 << 31) | (1 << 30) | (1 << 29);
const STEREO: u32 = 1 << 28;
const HZ: u32 = 22050;

/// All 256 entries, each its own colour.
fn whole_palette() -> Vec<u8> {
    let byte = |value: usize| u8::try_from(value).expect("under 64");
    let runs: Vec<u8> = (0..256)
        .flat_map(|at| [byte(at % 64), byte(at / 4), byte(63 - at % 64)])
        .collect();
    palette_chunk(&runs)
}

const ALL_SOLID_3: u16 = kind(SOLID, 7, 3);
const ALL_SOLID_7: u16 = kind(SOLID, 7, 7);
const ALL_VOID: u16 = kind(VOID, 7, 0);

/// 16 by 8, so 8 blocks, 5 frames: whole, then a block, then the lower half, then
/// a block, then whole; the middle three hold void blocks.
pub(super) fn painting_clip() -> Vec<u8> {
    let (one, skip_six) = (kind(SOLID, 0, 4), kind(VOID, 6, 0));
    let (skip_four, half) = (kind(VOID, 3, 0), kind(SOLID, 3, 5));
    let (skip_one, block, skip_five) = (kind(VOID, 0, 0), kind(SOLID, 0, 6), kind(VOID, 5, 0));
    let kinds = [
        ALL_SOLID_3,
        one,
        skip_six,
        skip_four,
        half,
        skip_one,
        block,
        skip_five,
        ALL_SOLID_7,
    ];
    let trees = Trees::new(None, None, None, Some((&kinds, KIND_MARKS)));
    let frames = [
        Frame {
            palette: Some(whole_palette()),
            video: trees.video().kind(ALL_SOLID_3).bytes(),
            ..Frame::default()
        },
        Frame {
            video: trees.video().kind(one).kind(skip_six).bytes(),
            ..Frame::default()
        },
        Frame {
            video: trees.video().kind(skip_four).kind(half).bytes(),
            ..Frame::default()
        },
        Frame {
            video: trees
                .video()
                .kind(skip_one)
                .kind(block)
                .kind(skip_five)
                .bytes(),
            ..Frame::default()
        },
        Frame {
            video: trees.video().kind(ALL_SOLID_7).bytes(),
            ..Frame::default()
        },
    ];
    clip(16, 8, &trees, &frames, None)
}

/// `count` frames at `rate`, a whole frame and then void ones.
pub(super) fn long_clip(count: usize, rate: i32) -> Vec<u8> {
    let kinds = [ALL_SOLID_3, ALL_VOID];
    let trees = Trees::new(None, None, None, Some((&kinds, KIND_MARKS)));
    let frames: Vec<Frame> = (0..count)
        .map(|at| Frame {
            palette: (at == 0).then(whole_palette),
            video: trees
                .video()
                .kind(if at == 0 { ALL_SOLID_3 } else { ALL_VOID })
                .bytes(),
            ..Frame::default()
        })
        .collect();
    let mut bytes = clip(16, 8, &trees, &frames, None);
    set_word(&mut bytes, RATE_WORD, rate.cast_unsigned());
    bytes
}

/// What a frame's sound chunk holds.
pub(super) enum Chunk {
    /// Interleaved samples; none is a frame with no chunk.
    Samples(Vec<i16>),
    /// An unpacked length past the track's most, so the decoder refuses it.
    TooLarge,
}

/// One frame an entry at `rate`, on a track of `hz` and `channels`.
pub(super) fn chunk_clip(hz: u32, rate: i32, channels: usize, chunks: &[Chunk]) -> Vec<u8> {
    let kinds = [ALL_SOLID_3, ALL_VOID];
    let trees = Trees::new(None, None, None, Some((&kinds, KIND_MARKS)));
    let largest = chunks.iter().fold(0, |most, chunk| match chunk {
        Chunk::Samples(samples) => most.max(samples.len() * 2),
        Chunk::TooLarge => most,
    });
    let frames: Vec<Frame> = chunks
        .iter()
        .enumerate()
        .map(|(at, chunk)| Frame {
            palette: (at == 0).then(whole_palette),
            sound: match chunk {
                Chunk::Samples(samples) if samples.is_empty() => None,
                Chunk::Samples(samples) => Some(sound_chunk(channels, samples)),
                Chunk::TooLarge => Some(sound_from_bits(u32::MAX, &[0x80])),
            },
            video: trees
                .video()
                .kind(if at == 0 { ALL_SOLID_3 } else { ALL_VOID })
                .bytes(),
        })
        .collect();
    let stereo = if channels == 2 { STEREO } else { 0 };
    let largest = u32::try_from(largest).expect("a test's sound fits");
    let track = (SIXTEEN_PACKED | stereo | hz, largest);
    let mut bytes = clip(16, 8, &trees, &frames, Some(track));
    set_word(&mut bytes, RATE_WORD, rate.cast_unsigned());
    bytes
}

/// One frame for each entry, holding those samples.
pub(super) fn samples_clip(hz: u32, rate: i32, channels: usize, frames: &[Vec<i16>]) -> Vec<u8> {
    let chunks: Vec<Chunk> = frames.iter().cloned().map(Chunk::Samples).collect();
    chunk_clip(hz, rate, channels, &chunks)
}

/// 2 frames with a mono track.
pub(super) fn track_clip() -> Vec<u8> {
    let kinds = [ALL_SOLID_3, ALL_VOID];
    let trees = Trees::new(None, None, None, Some((&kinds, KIND_MARKS)));
    let samples = [1, -2, 3, -4];
    let frames = [
        Frame {
            palette: Some(whole_palette()),
            sound: Some(sound_chunk(1, &samples)),
            video: trees.video().kind(ALL_SOLID_3).bytes(),
        },
        Frame {
            sound: Some(sound_chunk(1, &samples)),
            video: trees.video().kind(ALL_VOID).bytes(),
            ..Frame::default()
        },
    ];
    clip(16, 8, &trees, &frames, Some((SIXTEEN_PACKED | HZ, 8)))
}
