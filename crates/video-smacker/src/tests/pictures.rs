// SPDX-License-Identifier: MIT
//! A clip's pixels: each block kind, the chains, the recent-value escapes,
//! the frame-to-frame state, and the lines and calls around them.
use super::fixtures::{absent_trees_clip, all_pictures, blocks_clip, long_chain_clip};
use super::test_log::debug_lines;
use crate::{Clip, Picture, Refused};

const WIDTH: usize = 32;

/// Block `index` of a 32-pixel-wide picture, row by row.
fn block(picture: &Picture, index: usize) -> [[u8; 4]; 4] {
    let at = (index / 8) * 4 * WIDTH + (index % 8) * 4;
    core::array::from_fn(|y| core::array::from_fn(|x| picture.pixels[at + y * WIDTH + x]))
}

/// Map bit 0 is the top-left pixel, bit 15 the bottom-right, along each row;
/// a set bit takes the colour word's high byte.
#[test]
fn a_mono_blocks_set_bits_take_the_high_byte_from_the_top_left_on() {
    let pictures = all_pictures(&blocks_clip());
    assert_eq!(
        block(&pictures[0], 0),
        [[2, 1, 1, 1], [1, 1, 1, 1], [1, 1, 1, 1], [1, 1, 1, 2]]
    );
    assert_eq!(
        block(&pictures[0], 1),
        [[3, 3, 4, 3], [4, 4, 3, 3], [3, 4, 3, 3], [4, 3, 3, 3]]
    );
    assert_eq!(
        block(&pictures[0], 2),
        [[2, 2, 2, 2], [2, 2, 2, 2], [1, 1, 1, 1], [1, 1, 1, 1]]
    );
}

/// A row's first value paints its right pair, the second its left, each low
/// byte the pair's left pixel. A marker leaf reads a recent value, and any
/// value other than the newest shifts the three along, not moves to the
/// front: after slot 1's read, slot 2 holds the same value slot 0 does.
#[test]
fn a_full_block_reads_each_rows_right_pair_first_and_escapes_to_recent_values() {
    let pictures = all_pictures(&blocks_clip());
    assert_eq!(
        block(&pictures[0], 3),
        [
            [0x33, 0x44, 0x11, 0x22],
            [0x77, 0x88, 0x55, 0x66],
            [0x55, 0x66, 0x77, 0x88],
            [0xa9, 0xa0, 0x55, 0x66],
        ]
    );
}

/// The first frame's reads left recent values behind; the second's escapes
/// answer 0 until its own reads replace them.
#[test]
fn each_frame_starts_with_its_recent_values_at_0() {
    let pictures = all_pictures(&blocks_clip());
    assert_eq!(
        block(&pictures[1], 3),
        [
            [0x11, 0x22, 0x00, 0x00],
            [0x00, 0x00, 0x00, 0x00],
            [0x11, 0x22, 0x33, 0x44],
            [0x11, 0x22, 0x55, 0x66],
        ]
    );
}

/// A void block on the first frame leaves 0s, and on a later one the last
/// frame's pixels.
#[test]
fn a_void_block_keeps_what_the_last_frame_left() {
    let pictures = all_pictures(&blocks_clip());
    assert_eq!(block(&pictures[0], 5), [[0; 4]; 4]);
    for index in 0..3 {
        assert_eq!(block(&pictures[1], index), block(&pictures[0], index));
    }
    assert_eq!(pictures[2].pixels, pictures[1].pixels);
}

/// A chain stops at the picture's end however long its code says it is.
#[test]
fn a_chain_past_the_pictures_end_stops_there() {
    let pictures = all_pictures(&blocks_clip());
    for index in 6..32 {
        assert_eq!(block(&pictures[0], index), [[10; 4]; 4]);
    }
    for index in 4..32 {
        assert_eq!(block(&pictures[1], index), [[11; 4]; 4]);
    }
}

/// Code 59 is 128 blocks, not 60: a count of 60 would read another kind and
/// run out of bits.
#[test]
fn chain_code_59_is_128_blocks() {
    let bytes = long_chain_clip();
    let lines = debug_lines(|| {
        let pictures = all_pictures(&bytes);
        assert!(pictures[0].pixels.iter().all(|&pixel| pixel == 77));
    });
    assert!(lines[1].ends_with("m=0 u=0 s=128 v=0"), "{lines:?}");
}

/// An absent tree answers 0 and reads no bits, so a kind of 0 is a chain of
/// one mono block of colour 0.
#[test]
fn an_absent_tree_answers_0_and_reads_no_bits() {
    let bytes = absent_trees_clip();
    let lines = debug_lines(|| {
        let pictures = all_pictures(&bytes);
        assert!(pictures[0].pixels.iter().all(|&pixel| pixel == 0));
    });
    assert!(lines[1].ends_with("m=4 u=0 s=0 v=0"), "{lines:?}");
}

/// The lines tell what a clip said and what each frame painted.
#[test]
fn the_open_and_frame_lines_count_the_clip_and_its_blocks() {
    let bytes = blocks_clip();
    let lines = debug_lines(|| {
        all_pictures(&bytes);
    });
    assert_eq!(lines[0], "smk.open w=32 h=16 f=3 t=10000 c=0");
    assert!(lines[1].starts_with("smk.frame i=0 "), "{lines:?}");
    assert!(lines[1].contains(" p=1 m=3 u=1 s=27 v=1"), "{lines:?}");
    assert!(lines[2].contains(" p=0 m=0 u=1 s=28 v=3"), "{lines:?}");
    assert!(lines[3].contains(" p=0 m=0 u=0 s=0 v=32"), "{lines:?}");
}

/// `Painted` says what changed; a void frame changed no block.
#[test]
fn painted_says_which_frame_and_how_many_blocks_changed() {
    let mut clip = Clip::parse(&blocks_clip()).expect("a fixture parses");
    let mut picture = Picture::new();
    let first = clip
        .decode_next_frame(&mut picture)
        .expect("frame 0 paints");
    assert_eq!(
        (first.frame, first.palette_changed, first.blocks_changed),
        (0, true, 31)
    );
    clip.decode_next_frame(&mut picture)
        .expect("frame 1 paints");
    let third = clip
        .decode_next_frame(&mut picture)
        .expect("frame 2 paints");
    assert_eq!(
        (third.frame, third.palette_changed, third.blocks_changed),
        (2, false, 0)
    );
}

/// The next frame paints from the clip's own pixels, so the caller may write
/// over the picture it was given.
#[test]
fn a_picture_is_the_callers_own_to_write_over() {
    let untouched = all_pictures(&blocks_clip());
    let mut clip = Clip::parse(&blocks_clip()).expect("a fixture parses");
    let mut picture = Picture::new();
    clip.decode_next_frame(&mut picture)
        .expect("frame 0 paints");
    picture.pixels.fill(0xee);
    picture.palette.fill(0xee);
    clip.decode_next_frame(&mut picture)
        .expect("frame 1 paints");
    assert_eq!(picture, untouched[1]);
}

/// Seeking paints the way there, and going back starts again from the first
/// frame; the frame after a seek matches a straight read.
#[test]
fn seeking_lands_on_the_same_picture_a_straight_read_gives() {
    let straight = all_pictures(&blocks_clip());
    let mut clip = Clip::parse(&blocks_clip()).expect("a fixture parses");
    let mut picture = Picture::new();
    clip.seek(2).expect("frame 2 exists");
    assert_eq!(clip.cursor(), 2);
    clip.decode_next_frame(&mut picture)
        .expect("frame 2 paints");
    assert_eq!(picture, straight[2]);
    clip.seek(1).expect("frame 1 exists");
    clip.decode_next_frame(&mut picture)
        .expect("frame 1 paints");
    assert_eq!(picture, straight[1]);
}

/// A frame past the last is refused by name, by every call that takes one.
#[test]
fn every_call_past_the_last_frame_is_refused_by_name() {
    let mut clip = Clip::parse(&blocks_clip()).expect("a fixture parses");
    let mut picture = Picture::new();
    assert_eq!(clip.seek(3), Err(Refused::PastEnd));
    for _ in 0..3 {
        clip.decode_next_frame(&mut picture)
            .expect("a frame paints");
    }
    assert_eq!(clip.cursor(), 3);
    assert_eq!(clip.decode_next_frame(&mut picture), Err(Refused::PastEnd));
    assert_eq!(clip.frame_samples(), Err(Refused::PastEnd));
}
