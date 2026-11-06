// SPDX-License-Identifier: MIT
//! The palette's three kinds of run, read off a clip that shows every entry
//! as a block: entry `n` is block `n`'s colour.
use super::fixtures::{all_pictures, palette_clip, short_palette_clip, stored_entry};
use super::test_log::debug_lines;
use crate::palette::{BYTES, apply};
use crate::{Clip, Part, Picture, Refused};

/// The description's own 6-bit to 8-bit table.
const WIDENED: [u8; 64] = [
    0x00, 0x04, 0x08, 0x0C, 0x10, 0x14, 0x18, 0x1C, 0x20, 0x24, 0x28, 0x2C, 0x30, 0x34, 0x38, 0x3C,
    0x41, 0x45, 0x49, 0x4D, 0x51, 0x55, 0x59, 0x5D, 0x61, 0x65, 0x69, 0x6D, 0x71, 0x75, 0x79, 0x7D,
    0x82, 0x86, 0x8A, 0x8E, 0x92, 0x96, 0x9A, 0x9E, 0xA2, 0xA6, 0xAA, 0xAE, 0xB2, 0xB6, 0xBA, 0xBE,
    0xC3, 0xC7, 0xCB, 0xCF, 0xD3, 0xD7, 0xDB, 0xDF, 0xE3, 0xE7, 0xEB, 0xEF, 0xF3, 0xF7, 0xFB, 0xFF,
];

type Entry = [u8; 3];

/// Entry `index` of the first frame's palette, as colours.
fn first_palette_entry(index: usize) -> Entry {
    stored_entry(index).map(|stored| WIDENED[usize::from(stored)])
}

/// The palette a frame shows, read off its 256 blocks.
fn shown(picture: &Picture) -> Vec<Entry> {
    (0..256)
        .map(|block: usize| {
            let at = (block / 16) * 4 * 64 + (block % 16) * 4;
            let start = usize::from(picture.pixels[at]) * 3;
            core::array::from_fn(|byte| picture.palette[start + byte])
        })
        .collect()
}

/// The first byte of a set run is red, whatever the description's order.
#[test]
fn a_set_run_stores_red_then_green_then_blue() {
    let pictures = all_pictures(&palette_clip());
    let frame_0 = shown(&pictures[0]);
    assert_eq!(frame_0[1], [0x04, 0x00, 0xfb]);
    assert_eq!(frame_0[255], [0xff, 0xff, 0x00]);
    for (index, entry) in frame_0.iter().enumerate() {
        assert_eq!(*entry, first_palette_entry(index), "entry {index}");
    }
    let frame_1 = shown(&pictures[1]);
    assert_eq!(frame_1[10], [0xff, 0x14, 0x24]);
    assert_eq!(frame_1[19], [0xdb, 0x14, 0x24]);
}

/// A keep run moves past entries and leaves them, at any length up to 128.
#[test]
fn a_keep_run_leaves_its_entries_as_they_were() {
    let pictures = all_pictures(&palette_clip());
    let (before, after) = (shown(&pictures[0]), shown(&pictures[1]));
    assert_eq!(after[..10], before[..10]);
    assert_eq!(after[50..178], before[50..178]);
    assert_eq!(after[247..], before[247..]);
}

/// A copy run reads from its own source in the palette before the chunk,
/// even over entries this chunk has already rewritten or is rewriting.
#[test]
fn a_copy_run_reads_the_palette_as_it_stood_before_the_chunk() {
    let pictures = all_pictures(&palette_clip());
    let (before, after) = (shown(&pictures[0]), shown(&pictures[1]));
    assert_eq!(after[20..40], before[100..120]);
    assert_eq!(
        after[40..50],
        before[10..20],
        "entries 10-19 were rewritten above"
    );
    assert_eq!(
        after[178..183],
        before[175..180],
        "source and target overlap"
    );
    assert_eq!(after[183..247], before[..64]);
}

/// A later palette starts from the last frame's, and a frame with no palette
/// keeps it.
#[test]
fn a_palette_lasts_until_a_later_frame_changes_it() {
    let pictures = all_pictures(&palette_clip());
    let (frame_1, frame_2) = (shown(&pictures[1]), shown(&pictures[2]));
    assert_eq!(frame_2[..64], frame_1[192..]);
    assert_eq!(frame_2[64..], frame_1[64..]);
    assert_eq!(pictures[3].palette, pictures[2].palette);
}

/// Every stored value widens by the description's table.
#[test]
fn every_stored_value_widens_by_the_table() {
    let mut chunk = vec![0];
    for stored in 0..=255u8 {
        chunk.extend_from_slice(&[stored & 0x3f, 0, 0]);
    }
    let mut palette = [0; BYTES];
    apply(&chunk, &mut palette).expect("the runs fill the palette");
    for (entry, stored) in palette.chunks(3).zip(0..) {
        assert_eq!(entry[0], WIDENED[stored % 64], "stored {stored}");
    }
}

/// A run past the last entry, or a copy from past it, is refused.
#[test]
fn a_run_past_the_last_entry_is_refused() {
    let chunks: [&[u8]; 3] = [
        &[0, 0xff, 0xfe, 0x81],
        &[0, 0xff, 0xfe, 0x40 | 0x3f, 0],
        &[0, 0x40 | 0x3f, 200],
    ];
    for chunk in chunks {
        let mut palette = [0; BYTES];
        assert_eq!(
            apply(chunk, &mut palette),
            Err(Refused::Palette),
            "{chunk:?}"
        );
    }
}

/// Runs ending before the last entry are refused at that frame, whether the
/// chunk's bytes end with them or padding too short for a run follows.
#[test]
fn a_chunk_ending_before_the_last_entry_is_refused() {
    let runs: [&[u8]; 2] = [&[0x40, 9, 1, 2, 3, 0x40, 8], &[0x40, 9]];
    for run in runs {
        let mut clip = Clip::parse(&short_palette_clip(run)).expect("the clip's bytes are whole");
        let mut picture = Picture::new();
        clip.decode_next_frame(&mut picture)
            .expect("frame 0 is whole");
        let lines = debug_lines(|| {
            assert_eq!(
                clip.decode_next_frame(&mut picture),
                Err(Refused::Truncated(Part::Palette))
            );
        });
        assert_eq!(lines, ["smk.refused w=truncated-palette"]);
    }
}
