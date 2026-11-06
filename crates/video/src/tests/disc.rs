// SPDX-License-Identifier: MIT
//! The fronts over real clips: `VIDEO_CLIP_LIST` names a file listing one clip's
//! path a line, outside this tree. It prints counts alone, never a name.
use std::env;
use std::fs;

use video_smacker::{Clip, Picture};

use super::fakes::{Buffer, Mute, holding, holding_with, read_handle};
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
    let (mut over, mut sounding, mut door) = (0, 0, 0);
    for path in list.lines().filter(|line| !line.is_empty()) {
        let bytes = fs::read(path).expect("the clip is readable");
        let (width, height, hashes) = direct_hashes(&bytes);
        clips += 1;

        let mut videos = holding_with(Mute::default(), &bytes, 0, Choices::default());
        let mut buffer = Buffer::new(SCREEN);
        let handle = videos.smack_open(b"a.smk", 0xfe000, 0);
        buffer.name_to(&mut videos, handle);
        for bit in 0..32 {
            if videos.smack_sound_in_track(handle, 1 << bit) != 0 {
                sounding += 1;
            }
        }
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
        videos.smack_do_frame(handle);
        if Some(&hash(&buffer.rect(width, height))) != hashes.last() {
            over += 1;
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
        door += videos.sound().calls;
    }
    println!(
        "clips {clips} frames {frames} next-then-do frames {reversed} differences {differences} short {short} over {over} sound answers {sounding} door calls {door}"
    );
    assert_eq!((differences, short, over, sounding, door), (0, 0, 0, 0, 0));
}

/// One clip's whole sound through the fronts and a door that keeps it: `VIDEO_SOUND_OUT`
/// names a folder outside this tree for each track's bytes, by its place in the list.
/// A line a clip, its numbers alone; a script outside this tree judges them against
/// another decoder.
#[test]
#[ignore = "reads clips off a disc, by the list at VIDEO_CLIP_LIST; writes to VIDEO_SOUND_OUT"]
fn every_listed_track_reaches_the_door() {
    let list = env::var("VIDEO_CLIP_LIST").expect("VIDEO_CLIP_LIST names a list");
    let out = env::var("VIDEO_SOUND_OUT").expect("VIDEO_SOUND_OUT names a folder");
    let list = fs::read_to_string(list).expect("the list is readable");
    for (index, path) in list.lines().filter(|line| !line.is_empty()).enumerate() {
        let bytes = fs::read(path).expect("the clip is readable");
        let header = *Clip::parse(&bytes).expect("the clip parses").header();
        let mut videos = holding(&bytes, 0, Choices::default());
        let handle = videos.smack_open(b"a.smk", 0xfe000, 0);
        let answer = videos.smack_sound_in_track(handle, 0x2000);
        let others: u32 = (0..32)
            .filter(|bit| *bit != 13)
            .map(|bit| videos.smack_sound_in_track(handle, 1 << bit))
            .sum();
        let (rate, channels) = header
            .track
            .map_or((0, 1), |track| (u64::from(track.hz), u64::from(track.channels)));
        let mut pushed = 0_u64;
        let mut most_ahead_us = i64::MIN;
        for frame in 0..u64::from(header.frames) {
            let before = videos.sound().pushed.len();
            videos.smack_do_frame(handle);
            videos.smack_next_frame(handle);
            pushed += videos.sound().pushed[before..]
                .iter()
                .map(|(_, samples)| samples.len() as u64)
                .sum::<u64>();
            if rate != 0 {
                let heard_us = pushed / channels * 1_000_000 / rate;
                let shown_us = (frame + 1) * u64::from(header.frame_us);
                let ahead_us = i64::try_from(heard_us).expect("fits")
                    - i64::try_from(shown_us).expect("fits");
                most_ahead_us = most_ahead_us.max(ahead_us);
            }
        }
        videos.smack_close(handle);
        let door = videos.sound();
        if !door.pushed.is_empty() {
            let whole: Vec<u8> = door
                .pushed
                .iter()
                .flat_map(|(_, samples)| samples.iter().flat_map(|sample| sample.to_le_bytes()))
                .collect();
            fs::write(format!("{out}/{index}.s16le"), whole).expect("the folder is writable");
        }
        println!(
            "clip {index} frames {} period_us {} rate {rate} channels {channels} answer {answer} others {others} opens {} closes {} calls {} samples {pushed} most_ahead_us {}",
            header.frames,
            header.frame_us,
            door.opened.len(),
            door.closed.len(),
            door.calls,
            if rate == 0 { 0 } else { most_ahead_us },
        );
    }
}
