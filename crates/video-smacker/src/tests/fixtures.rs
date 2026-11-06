// SPDX-License-Identifier: MIT
//! The clips the tests read, built here and nowhere else.
use super::writer::{
    FULL, Frame, HEADER_BYTES, MONO, SOLID, Trees, VOID, clip, kind, palette_chunk, sound_chunk,
    sound_from_bits,
};
use crate::{Clip, Picture};

/// Markers for a kinds tree, kind words no fixture paints with.
const KIND_MARKS: [u16; 3] = [0x00f4, 0x00f8, 0x00fc];
const MARKS: [u16; 3] = [0xaaa0, 0xaaa1, 0xaaa2];
pub(super) const SIXTEEN_PACKED: u32 = (1 << 31) | (1 << 30) | (1 << 29);
pub(super) const STEREO: u32 = 1 << 28;
pub(super) const HZ: u32 = 22050;

/// The first palette's stored entry `index`: each one a different colour.
pub(super) fn stored_entry(index: usize) -> [u8; 3] {
    let byte = |value: usize| u8::try_from(value).expect("under 64");
    [byte(index % 64), byte(index / 4), byte(63 - index % 64)]
}

/// All 256 entries as `00` runs.
pub(super) fn whole_palette() -> Vec<u8> {
    let runs: Vec<u8> = (0..256).flat_map(stored_entry).collect();
    palette_chunk(&runs)
}

/// A `1ccccccc` run keeping `count` entries where they are.
fn keep(count: u8) -> [u8; 1] {
    [0x80 | (count - 1)]
}

/// A `01cccccc` run copying `count` entries from `from`.
fn copy(count: u8, from: u8) -> [u8; 2] {
    [0x40 | (count - 1), from]
}

/// Paints all 256 entries as a block each, then keeps them on show while
/// later frames rewrite the palette by each kind of run.
pub(super) fn palette_clip() -> Vec<u8> {
    let mut kinds: Vec<u16> = (0..256).map(|colour| kind(SOLID, 0, colour)).collect();
    kinds.push(kind(VOID, 63, 0));
    let trees = Trees::new(None, None, None, Some((&kinds, KIND_MARKS)));
    let mut painted = trees.video();
    for colour in 0..256 {
        painted = painted.kind(kind(SOLID, 0, colour));
    }
    let unchanged = || trees.video().kind(kind(VOID, 63, 0)).bytes();

    let mut every_run = Vec::new();
    every_run.extend_from_slice(&keep(10));
    for value in 0..10u8 {
        every_run.extend_from_slice(&[63 - value, 5, 9]);
    }
    every_run.extend_from_slice(&copy(20, 100));
    every_run.extend_from_slice(&copy(10, 10));
    every_run.extend_from_slice(&keep(128));
    every_run.extend_from_slice(&copy(5, 175));
    every_run.extend_from_slice(&copy(64, 0));
    every_run.extend_from_slice(&keep(9));

    let mut from_the_last_frame = Vec::new();
    from_the_last_frame.extend_from_slice(&copy(64, 192));
    from_the_last_frame.extend_from_slice(&keep(128));
    from_the_last_frame.extend_from_slice(&keep(64));

    let frames = [
        Frame {
            palette: Some(whole_palette()),
            video: painted.bytes(),
            ..Frame::default()
        },
        Frame {
            palette: Some(palette_chunk(&every_run)),
            video: unchanged(),
            ..Frame::default()
        },
        Frame {
            palette: Some(palette_chunk(&from_the_last_frame)),
            video: unchanged(),
            ..Frame::default()
        },
        Frame {
            video: unchanged(),
            ..Frame::default()
        },
    ];
    clip(64, 64, &trees, &frames, None)
}

/// A clip whose second frame's runs stop short of entry 255: `runs`, as the
/// chunk's own padding leaves them.
pub(super) fn short_palette_clip(runs: &[u8]) -> Vec<u8> {
    let kinds = [kind(VOID, 63, 0), kind(SOLID, 63, 0)];
    let trees = Trees::new(None, None, None, Some((&kinds, KIND_MARKS)));
    let frames = [
        Frame {
            palette: Some(whole_palette()),
            video: trees.video().kind(kind(SOLID, 63, 0)).bytes(),
            ..Frame::default()
        },
        Frame {
            palette: Some(palette_chunk(runs)),
            video: trees.video().kind(kind(VOID, 63, 0)).bytes(),
            ..Frame::default()
        },
    ];
    clip(8, 8, &trees, &frames, None)
}

/// A clip with every tree absent and no video bytes: each read answers 0.
pub(super) fn absent_trees_clip() -> Vec<u8> {
    let trees = Trees::new(None, None, None, None);
    let frames = [Frame {
        palette: Some(whole_palette()),
        ..Frame::default()
    }];
    clip(8, 8, &trees, &frames, None)
}

/// The values `blocks_clip`'s full blocks read, its escapes aside.
pub(super) const FULL_VALUES: [u16; 5] = [0x2211, 0x4433, 0x6655, 0x8877, 0xa0a9];
/// The words `reads_through_absent_clip`'s present trees answer.
const COLOURS: [u16; 2] = [0x0201, 0x0403];
const MAPS: [u16; 2] = [0x8001, 0x00ff];

/// A mono block, a full block and two solid ones, reading through the map,
/// colours and full trees only where the argument says present. The solid
/// blocks after them come out right only if an absent tree's read took no
/// bits from the stream.
pub(super) fn reads_through_absent_clip(map: bool, colours: bool, full: bool) -> Vec<u8> {
    let kinds = [kind(MONO, 0, 0), kind(FULL, 0, 0), kind(SOLID, 0, 9)];
    let trees = Trees::new(
        map.then_some((&MAPS[..], MARKS)),
        colours.then_some((&COLOURS[..], MARKS)),
        full.then_some((&FULL_VALUES[..], MARKS)),
        Some((&kinds, KIND_MARKS)),
    );
    let mut video = trees.video().kind(kind(MONO, 0, 0));
    if colours {
        video = video.colours(COLOURS[0]);
    }
    if map {
        video = video.map(MAPS[0]);
    }
    video = video.kind(kind(FULL, 0, 0));
    if full {
        let [one, two, three, four, five] = FULL_VALUES;
        video = video.full([one, two, three, four, five, one, two, three]);
    }
    let frames = [Frame {
        palette: Some(whole_palette()),
        video: video
            .kind(kind(SOLID, 0, 9))
            .kind(kind(SOLID, 0, 9))
            .bytes(),
        ..Frame::default()
    }];
    clip(8, 8, &trees, &frames, None)
}

/// A clip with the kinds tree alone absent: every block a mono block of a
/// chain of one, reading through the map and colours trees.
pub(super) fn absent_kinds_clip() -> Vec<u8> {
    let trees = Trees::new(
        Some((&MAPS, MARKS)),
        Some((&COLOURS, MARKS)),
        Some((&FULL_VALUES, MARKS)),
        None,
    );
    let frames = [Frame {
        palette: Some(whole_palette()),
        video: trees
            .video()
            .mono(COLOURS[0], MAPS[0])
            .mono(COLOURS[1], MAPS[1])
            .mono(COLOURS[0], MAPS[1])
            .mono(COLOURS[1], MAPS[0])
            .bytes(),
        ..Frame::default()
    }];
    clip(8, 8, &trees, &frames, None)
}

/// `blocks_clip` with the low 2 bits of its 3 frame sizes set: 1, 2 and 3.
pub(super) fn marked_sizes_clip() -> Vec<u8> {
    let mut bytes = blocks_clip();
    for (frame, mark) in [1, 2, 3].into_iter().enumerate() {
        bytes[HEADER_BYTES + frame * 4] |= mark;
    }
    bytes
}

/// 8 by 4 blocks of every kind, chains running past the picture's end and
/// recent-value escapes, over 3 frames.
pub(super) fn blocks_clip() -> Vec<u8> {
    let kinds = [
        kind(MONO, 0, 0),
        kind(MONO, 1, 0),
        kind(FULL, 0, 0),
        kind(SOLID, 0, 9),
        kind(VOID, 0, 0),
        kind(SOLID, 63, 10),
        kind(VOID, 2, 0),
        kind(SOLID, 58, 11),
        kind(VOID, 63, 0),
    ];
    let colours = [0x0201, 0x0403];
    let maps = [0x8001, 0x00ff, 0x1234];
    let trees = Trees::new(
        Some((&maps, MARKS)),
        Some((&colours, MARKS)),
        Some((&FULL_VALUES, MARKS)),
        Some((&kinds, KIND_MARKS)),
    );
    let [one, two, three, four, five] = FULL_VALUES;
    let [slot0, slot1, slot2] = MARKS;
    let first = trees
        .video()
        .kind(kind(MONO, 0, 0))
        .mono(0x0201, 0x8001)
        .kind(kind(MONO, 1, 0))
        .mono(0x0403, 0x1234)
        .mono(0x0201, 0x00ff)
        .kind(kind(FULL, 0, 0))
        .full([one, two, three, four, slot0, slot1, slot2, five])
        .kind(kind(SOLID, 0, 9))
        .kind(kind(VOID, 0, 0))
        .kind(kind(SOLID, 63, 10));
    let second = trees
        .video()
        .kind(kind(VOID, 2, 0))
        .kind(kind(FULL, 0, 0))
        .full([slot0, one, slot1, slot2, two, slot2, three, slot1])
        .kind(kind(SOLID, 58, 11));
    let third = trees.video().kind(kind(VOID, 63, 0));
    let frames = [
        Frame {
            palette: Some(whole_palette()),
            video: first.bytes(),
            ..Frame::default()
        },
        Frame {
            video: second.bytes(),
            ..Frame::default()
        },
        Frame {
            video: third.bytes(),
            ..Frame::default()
        },
    ];
    clip(32, 16, &trees, &frames, None)
}

/// 128 blocks all by one solid chain of code 59: the first chain length past
/// the counted ones.
pub(super) fn long_chain_clip() -> Vec<u8> {
    let kinds = [kind(SOLID, 59, 77), kind(VOID, 59, 0), kind(VOID, 0, 0)];
    let trees = Trees::new(None, None, None, Some((&kinds, KIND_MARKS)));
    let frames = [Frame {
        palette: Some(whole_palette()),
        video: trees.video().kind(kind(SOLID, 59, 77)).bytes(),
        ..Frame::default()
    }];
    clip(64, 32, &trees, &frames, None)
}

/// The samples of `sound_clip`'s three chunks, left then right when stereo:
/// jumps both ways, a wrap of 16 bits, and the two channels' firsts apart.
pub(super) fn sound_chunks(channels: usize) -> [Vec<i16>; 3] {
    let mono: [Vec<i16>; 3] = [
        vec![-300, -100, 900, 902, -30000, 30000],
        vec![5, 5, 5, 6],
        vec![1000, -1000],
    ];
    if channels == 1 {
        return mono;
    }
    mono.map(|left| {
        left.iter()
            .flat_map(|&sample| [sample, sample.wrapping_mul(3).wrapping_add(7)])
            .collect()
    })
}

/// A clip of 5 frames: sound, sound, none, a chunk saying no data, sound.
pub(super) fn sound_clip(channels: usize) -> Vec<u8> {
    let kinds = [kind(SOLID, 63, 3), kind(VOID, 63, 0)];
    let trees = Trees::new(None, None, None, Some((&kinds, KIND_MARKS)));
    let [first, second, third] = sound_chunks(channels);
    let unchanged = || trees.video().kind(kind(VOID, 63, 0)).bytes();
    let frames = [
        Frame {
            palette: Some(whole_palette()),
            sound: Some(sound_chunk(channels, &first)),
            video: trees.video().kind(kind(SOLID, 63, 3)).bytes(),
        },
        Frame {
            sound: Some(sound_chunk(channels, &second)),
            video: unchanged(),
            ..Frame::default()
        },
        Frame {
            video: unchanged(),
            ..Frame::default()
        },
        Frame {
            sound: Some(sound_from_bits(0, &[0])),
            video: unchanged(),
            ..Frame::default()
        },
        Frame {
            sound: Some(sound_chunk(channels, &third)),
            video: unchanged(),
            ..Frame::default()
        },
    ];
    let form = SIXTEEN_PACKED | HZ | if channels == 2 { STEREO } else { 0 };
    let largest = u32::try_from(first.len() * 2).expect("a test's sound fits");
    clip(8, 8, &trees, &frames, Some((form, largest)))
}

/// Every frame of `bytes` as the crate paints it.
pub(super) fn all_pictures(bytes: &[u8]) -> Vec<Picture> {
    let mut read = Clip::parse(bytes).expect("a fixture parses");
    (0..read.header().frames)
        .map(|_| {
            let mut picture = Picture::new();
            read.decode_next_frame(&mut picture)
                .expect("a fixture's frame paints");
            picture
        })
        .collect()
}
