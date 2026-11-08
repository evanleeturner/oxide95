// SPDX-License-Identifier: MIT
//! Writes clips with one fault each, cut by hand, so a page or a process can be
//! asked what it refuses them by; the expected names sit apart, in `video`'s
//! `bench/refusal_table.txt`. Not part of the fence: asked for by name, to a
//! folder named by `SMK_CUT_DIR`.
use std::path::Path;

use super::fixtures::{
    SIXTEEN_PACKED, absent_trees_clip, blocks_clip, palette_clip, short_palette_clip, sound_clip,
};
use super::writer::{HEADER_BYTES, LARGEST_WORD, TRACK_WORD, set_word};

const WIDTH_WORD: usize = 1;
const HEIGHT_WORD: usize = 2;
const FRAMES_WORD: usize = 3;
const RATE_WORD: usize = 4;
const FLAGS_WORD: usize = 5;
const TREES_SIZE_WORD: usize = 13;
const SECOND_TRACK_WORD: usize = TRACK_WORD + 1;
const SIZES_WORD: usize = HEADER_BYTES / 4;

fn word_at(bytes: &[u8], at: usize) -> u32 {
    let word: [u8; 4] = bytes[at..at + 4]
        .try_into()
        .expect("four bytes make a word");
    u32::from_le_bytes(word)
}

fn word_of(bytes: &[u8], word: usize) -> usize {
    usize::try_from(word_at(bytes, word * 4)).expect("a built clip's word fits")
}

/// `bytes` with one header word changed.
fn with_word(bytes: &[u8], word: usize, value: u32) -> Vec<u8> {
    let mut changed = bytes.to_vec();
    set_word(&mut changed, word, value);
    changed
}

/// `bytes` with frame `frame`'s stated size changed.
fn with_size(bytes: &[u8], frame: usize, value: u32) -> Vec<u8> {
    with_word(bytes, SIZES_WORD + frame, value)
}

/// `blocks_clip` with its first frame's kind byte holding `bits` as well.
fn with_kind_bits(bits: u8) -> Vec<u8> {
    let mut changed = blocks_clip();
    let first_kind = HEADER_BYTES + word_of(&changed, FRAMES_WORD) * 4;
    changed[first_kind] |= bits;
    changed
}

/// Where frame `frame` starts: past the header, the sizes, the kinds and the
/// trees, then every frame below it, a stated size's low 2 bits being marks.
fn frame_start(bytes: &[u8], frame: usize) -> usize {
    let frames = word_of(bytes, FRAMES_WORD);
    let below: usize = (0..frame)
        .map(|at| word_of(bytes, SIZES_WORD + at) & !3)
        .sum();
    HEADER_BYTES + frames * 4 + frames + word_of(bytes, TREES_SIZE_WORD) + below
}

/// `bytes` with a word of frame `frame`'s sound chunk changed.
fn with_sound_word(bytes: &[u8], frame: usize, index: usize, value: u32) -> Vec<u8> {
    let mut changed = bytes.to_vec();
    let at = frame_start(&changed, frame) + index * 4;
    changed[at..at + 4].copy_from_slice(&value.to_le_bytes());
    changed
}

/// One clip per row of the table; every cut starts from a clip that reads whole.
fn cuts() -> Vec<(&'static str, Vec<u8>)> {
    let sound = sound_clip(1);
    let blocks = blocks_clip();
    let palette = palette_clip();
    let mut first_byte = sound.clone();
    first_byte[0] = b'X';
    let mut smk4 = sound.clone();
    smk4[3] = b'4';
    let frames = word_of(&sound, FRAMES_WORD);
    let past_largest = word_at(&sound, LARGEST_WORD * 4) + 1;
    vec![
        ("first-byte", first_byte),
        ("smk4", smk4),
        ("flag-1", with_word(&sound, FLAGS_WORD, 1)),
        ("flag-2", with_word(&sound, FLAGS_WORD, 2)),
        ("flag-4", with_word(&sound, FLAGS_WORD, 4)),
        ("flag-8", with_word(&sound, FLAGS_WORD, 8)),
        ("rate-0", with_word(&sound, RATE_WORD, 0)),
        ("rate-positive", with_word(&sound, RATE_WORD, 1000)),
        ("rate-overflow", with_word(&sound, RATE_WORD, 0x8000_0000)),
        ("width-0", with_word(&sound, WIDTH_WORD, 0)),
        ("height-6", with_word(&sound, HEIGHT_WORD, 6)),
        (
            "size-past",
            with_word(
                &with_word(&sound, WIDTH_WORD, 1 << 16),
                HEIGHT_WORD,
                1 << 16,
            ),
        ),
        (
            "size-just-past",
            with_word(&with_word(&sound, WIDTH_WORD, 8192), HEIGHT_WORD, 8196),
        ),
        (
            "largest-past",
            with_word(&sound, LARGEST_WORD, (1 << 24) + 1),
        ),
        ("frames-0", with_word(&sound, FRAMES_WORD, 0)),
        (
            "track-eight",
            with_word(&sound, TRACK_WORD, SIXTEEN_PACKED & !(1 << 29)),
        ),
        (
            "track-bink-26",
            with_word(&sound, TRACK_WORD, SIXTEEN_PACKED | 1 << 26),
        ),
        (
            "track-bink-27",
            with_word(&sound, TRACK_WORD, SIXTEEN_PACKED | 1 << 27),
        ),
        (
            "track-raw",
            with_word(&sound, TRACK_WORD, 1 << 30 | 1 << 29),
        ),
        (
            "track-second",
            with_word(&sound, SECOND_TRACK_WORD, 1 << 30),
        ),
        ("kind-no-track", with_kind_bits(2)),
        ("kind-second", with_kind_bits(4)),
        ("no-trees", absent_trees_clip()),
        ("palette-past", short_palette_clip(&[0xff, 0xfe, 0x81])),
        ("palette-cut", with_size(&palette, 0, 4)),
        ("sound-length-4", with_sound_word(&sound, 1, 0, 4)),
        (
            "sound-unpacked-past",
            with_sound_word(&sound, 1, 1, past_largest),
        ),
        ("cut-header", sound[..50].to_vec()),
        ("cut-tables", sound[..HEADER_BYTES + 2].to_vec()),
        ("cut-trees", sound[..=HEADER_BYTES + frames * 5].to_vec()),
        ("trees-word-0", with_word(&blocks, TREES_SIZE_WORD, 0)),
        ("cut-frames", blocks[..blocks.len() - 1].to_vec()),
        ("sound-frame-cut", with_size(&sound, 1, 4)),
        ("video-chunk-empty", with_size(&blocks, 1, 0)),
    ]
}

#[test]
#[ignore = "writes hand-cut clips out for a page's or a process's answers"]
fn write_the_cuts() {
    let folder = std::env::var("SMK_CUT_DIR").expect("SMK_CUT_DIR names the folder");
    for (name, bytes) in cuts() {
        std::fs::write(Path::new(&folder).join(format!("{name}.smk")), bytes)
            .expect("the cut folder takes a file");
    }
}
