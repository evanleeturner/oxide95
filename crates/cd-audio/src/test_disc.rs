// SPDX-License-Identifier: MIT
//! A test's own disc, built in memory: a stand-in for a real cue and bin, an empty
//! ISO 9660 volume on its data track (ECMA-119) in raw sectors (ECMA-130), and two audio
//! files of known bytes. Nothing on it came from any game.

use logical_disc::{ReadAt, Source};

const USER: usize = 2048;
const SECTORS: u32 = 19;
pub(crate) const FRAME_BYTES: usize = 2352;

/// Two frames of pregap before track 2; track 3's file is big-endian.
pub(crate) const SHEET: &str = "FILE \"DATA.BIN\" BINARY\n  TRACK 01 MODE1/2352\n    INDEX 01 00:00:00\n\
FILE \"TWO.BIN\" BINARY\n  TRACK 02 AUDIO\n    INDEX 00 00:00:00\n    INDEX 01 00:00:02\n\
FILE \"THREE.BIN\" MOTOROLA\n  TRACK 03 AUDIO\n    INDEX 01 00:00:00\n";

pub(crate) const TWO_FILE_FRAMES: usize = 40;
pub(crate) const TWO_FRAMES: u32 = 38;
pub(crate) const THREE_FRAMES: u32 = 30;
pub(crate) const TWO_SEED: u8 = 0x40;
pub(crate) const THREE_SEED: u8 = 0x81;

/// Known bytes, no pair of them alike, so a turned sample shows.
pub(crate) fn fill(length: usize, seed: u8) -> Vec<u8> {
    (0u8..=250)
        .cycle()
        .take(length)
        .map(|byte| byte.wrapping_mul(7).wrapping_add(seed))
        .collect()
}

/// A folder's record (ECMA-119 9.1).
fn record(name: u8, extent: u32) -> Vec<u8> {
    let mut bytes = vec![0; 34];
    bytes[0] = 34;
    bytes[2..6].copy_from_slice(&extent.to_le_bytes());
    bytes[6..10].copy_from_slice(&extent.to_be_bytes());
    bytes[10..14].copy_from_slice(&2048u32.to_le_bytes());
    bytes[14..18].copy_from_slice(&2048u32.to_be_bytes());
    bytes[25] = 2;
    bytes[28..30].copy_from_slice(&1u16.to_le_bytes());
    bytes[30..32].copy_from_slice(&1u16.to_be_bytes());
    bytes[32] = 1;
    bytes[33] = name;
    bytes
}

/// The volume: its descriptor at 16, the terminator at 17, an empty root at 18.
fn volume() -> Vec<u8> {
    let mut image = vec![0; SECTORS as usize * USER];
    let mut put = |sector: usize, at: usize, bytes: &[u8]| {
        let from = sector * USER + at;
        image[from..from + bytes.len()].copy_from_slice(bytes);
    };
    for (at, bytes) in [
        (0, &[1][..]),
        (1, b"CD001"),
        (6, &[1]),
        (80, &SECTORS.to_le_bytes()),
        (84, &SECTORS.to_be_bytes()),
        (128, &2048u16.to_le_bytes()),
        (130, &2048u16.to_be_bytes()),
        (156, &record(0, 18)),
    ] {
        put(16, at, bytes);
    }
    put(17, 0, &[255]);
    put(17, 1, b"CD001");
    put(17, 6, &[1]);
    put(18, 0, &[record(0, 18), record(1, 18)].concat());
    image
}

/// The volume's sectors as a raw `MODE1/2352` track.
fn raw_track() -> Vec<u8> {
    let bcd = |value: usize| u8::try_from(value / 10 * 16 + value % 10).expect("a BCD byte");
    let mut sync = [0xff; 12];
    sync[0] = 0;
    sync[11] = 0;
    let mut raw = Vec::new();
    for (sector, user) in volume().chunks(USER).enumerate() {
        let address = sector + 150;
        raw.extend_from_slice(&sync);
        raw.extend_from_slice(&[
            bcd(address / 75 / 60),
            bcd(address / 75 % 60),
            bcd(address % 75),
            1,
        ]);
        raw.extend_from_slice(user);
        raw.extend_from_slice(&[0; 288]);
    }
    raw
}

/// The disc's files, each made into whatever a test reads them through.
pub(crate) fn image<R: ReadAt>(make: impl Fn(Vec<u8>) -> R) -> Source<R> {
    let two = fill(TWO_FILE_FRAMES * FRAME_BYTES, TWO_SEED);
    let three = fill(THREE_FRAMES as usize * FRAME_BYTES, THREE_SEED);
    Source::Image {
        cue: make(SHEET.as_bytes().to_vec()),
        files: vec![
            ("TWO.BIN".to_owned(), make(two)),
            ("THREE.BIN".to_owned(), make(three)),
            ("DATA.BIN".to_owned(), make(raw_track())),
        ],
    }
}
