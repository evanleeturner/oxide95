// SPDX-License-Identifier: MIT
//! Writes one clip whose frames all differ, each built on the frames before it,
//! so a seek landing on the wrong frame, or one skipping a frame's painting,
//! shows in a hash. No other built clip can say so: each holds two frames alike
//! or has under 3 frames. Not part of the fence: asked for by name, to the folder
//! named by `SMK_PROBE_DIR`.
use std::path::Path;

use super::fixtures::whole_palette;
use super::writer::{Frame, SOLID, Trees, VOID, clip, kind};

/// Markers for a kinds tree, kind words this clip does not paint with.
const KIND_MARKS: [u16; 3] = [0x00f4, 0x00f8, 0x00fc];
const FRAMES: u16 = 6;
/// The colour of block `n`'s paint is this plus `n`.
const FIRST_COLOUR: u16 = 10;

/// Frame 0 fills 16 by 8 with one colour; frame `n` keeps all it can and paints
/// block `n` alone, so each frame holds one more painted block than the last.
fn ladder_clip() -> Vec<u8> {
    let filled = kind(SOLID, 63, 1);
    let the_rest = kind(VOID, 63, 0);
    let before = |block: u16| kind(VOID, block - 1, 0);
    let painted = |block: u16| kind(SOLID, 0, FIRST_COLOUR + block);
    let mut kinds: Vec<u16> = vec![filled, the_rest];
    for block in 1..FRAMES {
        kinds.extend([before(block), painted(block)]);
    }
    let trees = Trees::new(None, None, None, Some((&kinds, KIND_MARKS)));
    let mut frames = vec![Frame {
        palette: Some(whole_palette()),
        video: trees.video().kind(filled).bytes(),
        ..Frame::default()
    }];
    for block in 1..FRAMES {
        frames.push(Frame {
            video: trees
                .video()
                .kind(before(block))
                .kind(painted(block))
                .kind(the_rest)
                .bytes(),
            ..Frame::default()
        });
    }
    clip(16, 8, &trees, &frames, None)
}

#[test]
#[ignore = "writes a built clip out for another decoder's answers"]
fn write_the_seek_clip() {
    let folder = std::env::var("SMK_PROBE_DIR").expect("SMK_PROBE_DIR names the folder");
    std::fs::write(Path::new(&folder).join("ladder.smk"), ladder_clip())
        .expect("the probe folder takes a file");
}
