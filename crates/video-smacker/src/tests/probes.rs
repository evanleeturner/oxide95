// SPDX-License-Identifier: MIT
//! Writes the fixtures out so another decoder, run as a program, can answer
//! for them. Not part of the fence: asked for by name, to a folder named by
//! `SMK_PROBE_DIR`, it writes built clips only.
use std::path::Path;

use super::fixtures::{
    absent_kinds_clip, absent_trees_clip, blocks_clip, long_chain_clip, marked_sizes_clip,
    palette_clip, reads_through_absent_clip, short_palette_clip, sound_clip,
};

#[test]
#[ignore = "writes the built clips out for another decoder's answers"]
fn write_the_probes() {
    let folder = std::env::var("SMK_PROBE_DIR").expect("SMK_PROBE_DIR names the folder");
    let clips = [
        ("palette", palette_clip()),
        ("short", short_palette_clip(&[0x40, 9, 1, 2, 3, 0x40, 8])),
        ("padded", short_palette_clip(&[0x40, 9])),
        ("absent", absent_trees_clip()),
        ("absent-kinds", absent_kinds_clip()),
        ("absent-map", reads_through_absent_clip(false, true, true)),
        (
            "absent-colours",
            reads_through_absent_clip(true, false, true),
        ),
        ("absent-full", reads_through_absent_clip(true, true, false)),
        (
            "absent-three",
            reads_through_absent_clip(false, false, false),
        ),
        ("marked-sizes", marked_sizes_clip()),
        ("blocks", blocks_clip()),
        ("chain", long_chain_clip()),
        ("mono", sound_clip(1)),
        ("stereo", sound_clip(2)),
    ];
    for (name, bytes) in clips {
        std::fs::write(Path::new(&folder).join(format!("{name}.smk")), bytes)
            .expect("the probe folder takes a file");
    }
}
