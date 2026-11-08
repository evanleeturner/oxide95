// SPDX-License-Identifier: MIT
//! Writes clips with a known period out, for a page's pace against a stopwatch. Not part
//! of the fence: asked for by name, to a folder named by `VIDEO_PAGE_CLIP_DIR`.
use std::path::Path;

use super::clips::long_clip;

const FRAMES: usize = 60;

#[test]
#[ignore = "writes clips out for a page's pace"]
fn write_the_page_clips() {
    let folder =
        std::env::var("VIDEO_PAGE_CLIP_DIR").expect("VIDEO_PAGE_CLIP_DIR names the folder");
    for (name, rate) in [("long-6666", -6666), ("long-7100", -7100)] {
        let bytes = long_clip(FRAMES, rate);
        std::fs::write(Path::new(&folder).join(format!("{name}.smk")), bytes)
            .expect("the clip folder takes a file");
    }
}
