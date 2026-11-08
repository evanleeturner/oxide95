// SPDX-License-Identifier: MIT
//! The audio tracks against sheets of known frames: each frame's 588
//! stereo samples stamped with its track and its frame, so a byte read
//! back names its place. Samples are read by the track's own byte order,
//! as a player would; each check prints one line under `--nocapture`.
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::fmt::Write as _;

use kernel_files::Error;

use super::{bin, iso};
use crate::{Audio, Disc, Source};

/// A frame's bytes (IEC 60908: 588 stereo samples), as 16-bit words.
const FRAME: usize = 2352;
const WORDS: usize = FRAME / 2;

/// A track's own frames carry `0x7100` plus its number; its pregap's
/// `0x7200` plus it.
const TRACK: u16 = 0x7100;
const PREGAP: u16 = 0x7200;

/// One frame: word 0 its mark, word 1 its frame counted in that place,
/// each later word `0x2000` past its index; big-endian in a `MOTOROLA`
/// file.
fn stamp(mark: u16, frame: u16, big: bool) -> Vec<u8> {
    (0..WORDS)
        .flat_map(|word| {
            let value = match word {
                0 => mark,
                1 => frame,
                _ => 0x2000 + u16::try_from(word).expect("a word's index"),
            };
            if big {
                value.to_be_bytes()
            } else {
                value.to_le_bytes()
            }
        })
        .collect()
}

/// `count` frames of one mark, from frame 0.
fn frames(mark: u16, count: u16, big: bool) -> Vec<u8> {
    (0..count).flat_map(|frame| stamp(mark, frame, big)).collect()
}

/// One bin: the data track's 25 sectors, track 2's 10 frames with no
/// pregap, track 3's pregap of 150 frames, then its 1 frame.
const ONE_BIN: &str = "FILE \"ONE.BIN\" BINARY
  TRACK 01 MODE1/2352
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    INDEX 01 00:00:25
  TRACK 03 AUDIO
    INDEX 00 00:00:35
    INDEX 01 00:02:35
";

/// One file a track: a pregap of 0 frames, one of 150 in a `MOTOROLA`
/// file, and a track of 1 frame.
const FILE_A_TRACK: &str = "FILE \"D.BIN\" BINARY
  TRACK 01 MODE1/2352
    INDEX 01 00:00:00
FILE \"T2.BIN\" BINARY
  TRACK 02 AUDIO
    INDEX 00 00:00:00
    INDEX 01 00:00:00
FILE \"T3.BIN\" MOTOROLA
  TRACK 03 AUDIO
    INDEX 00 00:00:00
    INDEX 01 00:02:00
FILE \"T4.BIN\" BINARY
  TRACK 04 AUDIO
    INDEX 01 00:00:00
";

/// An image: its name in the printed rows, its sheet and files, and each
/// audio track by hand from the sheet: number, first frame on the disc,
/// frames, big-endian.
struct Image {
    name: &'static str,
    sheet: &'static str,
    files: Vec<(String, Vec<u8>)>,
    tracks: &'static [(u8, u32, u32, bool)],
}

fn one_bin() -> Image {
    let mut bytes = bin(&iso());
    bytes.extend(frames(TRACK + 2, 10, false));
    bytes.extend(frames(PREGAP + 3, 150, false));
    bytes.extend(frames(TRACK + 3, 1, false));
    Image {
        name: "one_bin",
        sheet: ONE_BIN,
        files: vec![("ONE.BIN".to_string(), bytes)],
        tracks: &[(2, 25, 10, false), (3, 185, 1, false)],
    }
}

fn file_a_track() -> Image {
    let mut third = frames(PREGAP + 3, 150, true);
    third.extend(frames(TRACK + 3, 7, true));
    Image {
        name: "file_a_track",
        sheet: FILE_A_TRACK,
        files: vec![
            ("D.BIN".to_string(), bin(&iso())),
            ("T2.BIN".to_string(), frames(TRACK + 2, 12, false)),
            ("T3.BIN".to_string(), third),
            ("T4.BIN".to_string(), frames(TRACK + 4, 1, false)),
        ],
        tracks: &[(2, 25, 12, false), (3, 187, 7, true), (4, 194, 1, false)],
    }
}

/// The printed rows of one image, and the names that differ; `kind` is
/// `DI3`, or `DI3x` for an ignored test's rows.
struct Rows {
    kind: &'static str,
    image: &'static str,
    differ: Vec<String>,
}

impl Rows {
    fn row(&mut self, what: &str, got: &str, want: &str) {
        let verdict = if got == want {
            "ok"
        } else {
            self.differ.push(format!("{} {what}", self.image));
            "DIFF"
        };
        println!("{} {} {what} {got} want {want} {verdict}", self.kind, self.image);
    }
}

fn shown(tracks: &[(u8, u32, u32, bool)]) -> String {
    let each: Vec<String> = tracks
        .iter()
        .map(|&(number, start, frames, big)| {
            let order = if big { "be" } else { "le" };
            format!("{number}:{start}:{frames}:{order}")
        })
        .collect();
    each.join(",")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut out, byte| {
        write!(out, "{byte:02x}").expect("a string takes a write");
        out
    })
}

/// A read's answer as a row prints it: the count and the bytes moved.
fn answer(read: Result<usize, Error>, into: &[u8]) -> String {
    match read {
        Ok(count) => format!("count={count}:{}", hex(&into[..count.min(into.len())])),
        Err(error) => format!("refused={error:?}"),
    }
}

/// Frames of a read whose words are not their stamps, read by `big`.
fn wrong_frames(bytes: &[u8], mark: u16, big: bool) -> usize {
    bytes
        .chunks(FRAME)
        .enumerate()
        .filter(|&(frame, words)| {
            let frame = u16::try_from(frame).expect("a frame");
            let read = |pair: &[u8]| {
                let pair = [pair[0], pair[1]];
                if big {
                    u16::from_be_bytes(pair)
                } else {
                    u16::from_le_bytes(pair)
                }
            };
            words.len() != FRAME
                || words.chunks(2).enumerate().any(|(word, pair)| {
                    let want = match word {
                        0 => mark,
                        1 => frame,
                        _ => 0x2000 + u16::try_from(word).expect("a word's index"),
                    };
                    read(pair) != want
                })
        })
        .count()
}

/// The image's disc, opened from its sheet and its files.
fn opened(image: Image) -> Disc<Vec<u8>> {
    Disc::open(Source::Image {
        cue: image.sheet.as_bytes().to_vec(),
        files: image.files,
    })
    .expect("the image opens")
}

/// Every track's place and order against the sheet; each track read
/// whole, across a frame's edge, at its last byte and at its end, every
/// frame its own stamp; a number with no audio track not found.
fn check(image: Image) -> Vec<String> {
    let mut rows = Rows {
        kind: "DI3",
        image: image.name,
        differ: Vec::new(),
    };
    let tracks = image.tracks;
    let disc = opened(image);
    let found: Vec<_> = disc
        .tracks()
        .iter()
        .map(|track| match &track.audio {
            Audio::Pcm(pcm) => (track.number, pcm.start, pcm.frames, pcm.big_endian),
            Audio::Encoded(_) => panic!("an image's track is raw"),
        })
        .collect();
    rows.row("tracks", &shown(&found), &shown(tracks));
    for &(number, _, count, big) in tracks {
        let mark = TRACK + u16::from(number);
        let length = usize::try_from(count).expect("frames") * FRAME;
        let source = frames(mark, u16::try_from(count).expect("frames"), big);
        // Read by the reader's flag, as a player reads it.
        let flag = found
            .iter()
            .find(|track| track.0 == number)
            .is_some_and(|track| track.3);
        let mut whole = vec![0; length + 100];
        let read = disc.read_track(number, 0, &mut whole);
        let moved = read.map_or(0, |count| count.min(length));
        rows.row(
            &format!("track{number}_whole"),
            &format!("{read:?}"),
            &format!("{:?}", Ok::<usize, Error>(length)),
        );
        let wrong = wrong_frames(&whole[..moved], mark, flag) + (length - moved) / FRAME;
        rows.row(&format!("track{number}_frames_wrong"), &wrong.to_string(), "0");
        for (what, at) in [
            ("edge", FRAME - 2),
            ("last_byte", length - 1),
            ("at_end", length),
        ] {
            let mut into = [0; 4];
            let read = disc.read_track(number, u64::try_from(at).expect("a place"), &mut into);
            let from = at.min(length);
            let to = (at + 4).min(length);
            let want = format!("count={}:{}", to - from, hex(&source[from..to]));
            rows.row(&format!("track{number}_{what}"), &answer(read, &into), &want);
        }
    }
    for number in [1, 9] {
        let mut into = [0; 4];
        let read = disc.read_track(number, 0, &mut into);
        rows.row(
            &format!("track{number}_absent"),
            &answer(read, &into),
            &format!("refused={:?}", Error::NotFound),
        );
    }
    rows.differ
}

#[test]
fn every_track_reads_its_stamped_frames() {
    let mut differ = check(one_bin());
    differ.extend(check(file_a_track()));
    assert!(differ.is_empty(), "rows differ: {differ:?}");
}

/// A read past a track's end: its doc promises a read short at the end,
/// so 100 bytes past it should move nothing, whatever follows the track
/// in its file.
#[test]
#[ignore = "a read past a track's end is refused when its file ends there, else moves nothing"]
fn a_read_past_a_tracks_end_moves_nothing() {
    let mut differ = Vec::new();
    for image in [one_bin(), file_a_track()] {
        let mut rows = Rows {
            kind: "DI3x",
            image: image.name,
            differ: Vec::new(),
        };
        let tracks = image.tracks;
        let disc = opened(image);
        for &(number, _, count, _) in tracks {
            let at = usize::try_from(count).expect("frames") * FRAME + 100;
            let mut into = [0; 4];
            let read = disc.read_track(number, u64::try_from(at).expect("a place"), &mut into);
            rows.row(&format!("track{number}_past_end"), &answer(read, &into), "count=0:");
        }
        differ.extend(rows.differ);
    }
    assert!(differ.is_empty(), "rows differ: {differ:?}");
}
