// SPDX-License-Identifier: MIT
//! Falsifier 1 over real clips: `VIDEO_CLIP_LIST` names a file listing one clip's
//! path a line, outside this tree. It prints counts alone, never a name.
use std::env;
use std::fs;

use video_smacker::{Clip, Picture};

use super::fakes::{Buffer, holding, read_handle};
use crate::{Choices, Place};

/// A screen as wide and tall as any clip needs, the clip off its corner.
const SCREEN: Place = Place {
    x: 16,
    y: 8,
    pitch: 640,
    height: 480,
};

/// FNV-1a: a test's own hash, needing no dependency.
fn hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |sum, byte| {
        (sum ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// Each frame's hash by a direct decode, and the clip's size.
fn direct_hashes(bytes: &[u8]) -> (usize, usize, Vec<u64>) {
    let mut clip = Clip::parse(bytes).expect("the clip parses");
    let header = *clip.header();
    let mut picture = Picture::new();
    let hashes = (0..header.frames)
        .map(|_| {
            clip.decode_next_frame(&mut picture)
                .expect("the clip decodes");
            hash(&picture.pixels)
        })
        .collect();
    (header.width as usize, header.height as usize, hashes)
}

#[test]
#[ignore = "reads clips off a disc, by the list at VIDEO_CLIP_LIST"]
fn every_clip_listed_plays_through_the_fronts_as_a_direct_decode() {
    let list = env::var("VIDEO_CLIP_LIST").expect("VIDEO_CLIP_LIST names a list");
    let list = fs::read_to_string(list).expect("the list is readable");
    let (mut clips, mut frames, mut reversed, mut differences, mut short) = (0, 0, 0, 0, 0);
    for path in list.lines().filter(|line| !line.is_empty()) {
        let bytes = fs::read(path).expect("the clip is readable");
        let (width, height, hashes) = direct_hashes(&bytes);
        clips += 1;

        let mut videos = holding(&bytes, 0, Choices::default());
        let mut buffer = Buffer::new(SCREEN);
        let handle = videos.smack_open(b"a.smk", 0, 0);
        buffer.name_to(&mut videos, handle);
        for expected in &hashes {
            videos.smack_do_frame(handle);
            frames += 1;
            if hash(&buffer.rect(width, height)) != *expected {
                differences += 1;
            }
            videos.smack_next_frame(handle);
        }
        if read_handle(handle).2 as usize != hashes.len() {
            short += 1;
        }
        videos.smack_close(handle);

        let handle = videos.smack_open(b"a.smk", 0, 0);
        let mut buffer = Buffer::sized(
            u32::try_from(width).expect("fits"),
            u32::try_from(height).expect("fits"),
        );
        buffer.name_to(&mut videos, handle);
        for expected in hashes.iter().skip(1) {
            videos.smack_next_frame(handle);
            videos.smack_do_frame(handle);
            reversed += 1;
            if hash(&buffer.rect(width, height)) != *expected {
                differences += 1;
            }
        }
    }
    println!(
        "clips {clips} frames {frames} next-then-do frames {reversed} differences {differences} short {short}"
    );
    assert_eq!((differences, short), (0, 0));
}
