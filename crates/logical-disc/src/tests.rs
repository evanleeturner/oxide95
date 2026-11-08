// SPDX-License-Identifier: MIT
//! The synthetic image, written here from the public layouts,
//! a few sectors and never a game's: a cue sheet; a Mode 1 bin holding an
//! ISO 9660 tree of 3 files, one of them across 3 sectors; 2 audio tracks
//! of known samples, each with a pregap. The same tree as an iso alone.
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use crate::disc::{RAW, USER};
use crate::{Disc, OpenError, Source};

mod folder;
mod records;
mod sheets;
mod stamped;
mod test_log;
mod tracks;

/// The tree's files: the name a game asks, its sector, its length, its
/// bytes' seed.
const FILES: [(&str, u32, usize, u8); 3] = [
    ("README.TXT", 20, 100, 1),
    ("DATA\\LEVEL.DAT", 21, 2048, 2),
    ("DATA\\SOUND.RAW", 22, 5000, 3),
];
const SECTORS: u32 = 25;
const LABEL: &[u8] = b"SYNTH_DISC";

/// The audio files: the name as picked, frames, the seed.
const AUDIO: [(&str, usize, u8); 2] = [("track 02.bin", 40, 0x40), ("Track 03.bin", 30, 0x80)];

const SHEET: &str = "\u{feff}REM a synthetic disc\r\n\
FILE \"Synth.bin\" BINARY\r\n  TRACK 01 MODE1/2352\r\n    INDEX 01 00:00:00\r\n\
FILE \"Track 02.bin\" BINARY\r\n  TRACK 02 AUDIO\r\n    INDEX 00 00:00:00\r\n    INDEX 01 00:00:10\r\n\
FILE \"Track 03.bin\" MOTOROLA\r\n  TRACK 03 AUDIO\r\n    INDEX 00 00:00:00\r\n    INDEX 01 00:00:05\r\n";

/// Known bytes, a different run each seed.
fn fill(length: usize, seed: u8) -> Vec<u8> {
    (0u8..=255)
        .cycle()
        .take(length)
        .map(|byte| byte.wrapping_mul(7).wrapping_add(seed))
        .collect()
}

/// A folder's record (ECMA-119 9.1): the extent and size both-endian, the
/// flags, the volume's sequence number 1, the name, padded to even.
fn record(name: &[u8], extent: u32, size: u32, folder: bool) -> Vec<u8> {
    let length = 33 + name.len() + (name.len() + 1) % 2;
    let mut bytes = vec![0; length];
    bytes[0] = u8::try_from(length).expect("a record's length");
    bytes[2..6].copy_from_slice(&extent.to_le_bytes());
    bytes[6..10].copy_from_slice(&extent.to_be_bytes());
    bytes[10..14].copy_from_slice(&size.to_le_bytes());
    bytes[14..18].copy_from_slice(&size.to_be_bytes());
    bytes[25] = if folder { 2 } else { 0 };
    bytes[28..30].copy_from_slice(&1u16.to_le_bytes());
    bytes[30..32].copy_from_slice(&1u16.to_be_bytes());
    bytes[32] = u8::try_from(name.len()).expect("a name's length");
    bytes[33..33 + name.len()].copy_from_slice(name);
    bytes
}

/// The tree as an iso: the descriptor at 16, its terminator at 17, the
/// root at 18, `DATA` at 19, the files from 20.
fn iso() -> Vec<u8> {
    let block = usize::try_from(USER).expect("a sector");
    let mut image = vec![0; SECTORS as usize * block];
    let mut put = |sector: u32, at: usize, bytes: &[u8]| {
        let from = sector as usize * block + at;
        image[from..from + bytes.len()].copy_from_slice(bytes);
    };
    let mut label = [b' '; 32];
    label[..LABEL.len()].copy_from_slice(LABEL);
    for (at, bytes) in [
        (0, &[1][..]),
        (1, b"CD001"),
        (6, &[1]),
        (40, &label),
        (80, &SECTORS.to_le_bytes()),
        (84, &SECTORS.to_be_bytes()),
        (128, &2048u16.to_le_bytes()),
        (130, &2048u16.to_be_bytes()),
        (156, &record(&[0], 18, 2048, true)),
    ] {
        put(16, at, bytes);
    }
    put(17, 0, &[255]);
    put(17, 1, b"CD001");
    put(17, 6, &[1]);
    let root = [
        record(&[0], 18, 2048, true),
        record(&[1], 18, 2048, true),
        record(b"DATA", 19, 2048, true),
        record(b"README.TXT;1", 20, 100, false),
    ];
    put(18, 0, &root.concat());
    let data = [
        record(&[0], 19, 2048, true),
        record(&[1], 18, 2048, true),
        record(b"LEVEL.DAT;1", 21, 2048, false),
        record(b"SOUND.RAW;1", 22, 5000, false),
    ];
    put(19, 0, &data.concat());
    for (_, sector, length, seed) in FILES {
        put(sector, 0, &fill(length, seed));
    }
    image
}

/// The iso's sectors as a raw `MODE1/2352` track (ECMA-130): the sync, the
/// header's address in BCD from 00:02:00 and mode 1, the user bytes, the
/// error codes left zero.
fn bin(iso: &[u8]) -> Vec<u8> {
    let bcd = |value: usize| u8::try_from(value / 10 * 16 + value % 10).expect("a BCD byte");
    let mut sync = [0xff; 12];
    sync[0] = 0;
    sync[11] = 0;
    let mut bin = Vec::new();
    for (sector, user) in iso.chunks(2048).enumerate() {
        let address = sector + 150;
        bin.extend_from_slice(&sync);
        let header = [
            bcd(address / 75 / 60),
            bcd(address / 75 % 60),
            bcd(address % 75),
            1,
        ];
        bin.extend_from_slice(&header);
        bin.extend_from_slice(user);
        bin.extend_from_slice(&[0; 288]);
    }
    bin
}

/// The image as picked: the audio first and the bin's name in capitals, so
/// the sheet's names are found, not assumed.
fn image_source(sheet: &str) -> Source<Vec<u8>> {
    let raw = usize::try_from(RAW).expect("a sector");
    let mut files: Vec<(String, Vec<u8>)> = AUDIO
        .iter()
        .map(|&(name, frames, seed)| (name.to_string(), fill(frames * raw, seed)))
        .collect();
    files.push(("picked/SYNTH.BIN".to_string(), bin(&iso())));
    Source::Image {
        cue: sheet.as_bytes().to_vec(),
        files,
    }
}

/// The data half: every file read back equal, from the image
/// and from the iso alone, by any spelling of its name; a read inside a
/// file, across a sector's edge, and at its end.
#[test]
fn every_file_reads_back_equal() {
    let image = Disc::open(image_source(SHEET)).expect("the image opens");
    let iso = Disc::open(Source::Iso(iso())).expect("the iso opens");
    for disc in [image, iso] {
        assert_eq!(disc.label(), LABEL);
        for (name, _, length, seed) in FILES {
            let entry = disc.entry(name.as_bytes()).expect(name);
            let mut got = vec![0; length + 10];
            let count = disc.read(entry, 0, &mut got).expect(name);
            assert_eq!(count, length, "{name}");
            assert_eq!(got[..count], fill(length, seed), "{name}");
        }
        let sound = disc.entry(b"/data/sound.raw").expect("folded");
        let mut got = [0; 100];
        assert_eq!(disc.read(sound, 2000, &mut got), Ok(100));
        assert_eq!(got[..], fill(5000, 3)[2000..2100]);
        assert_eq!(disc.read(sound, 5000, &mut got), Ok(0));
        assert!(disc.entry(b"DATA").is_some_and(|entry| entry.folder));
        assert!(disc.entry(b"DATA\\NONE.DAT").is_none());
    }
}

/// The sheet's refusals, each its line, and the volume's.
#[test]
fn a_broken_source_is_refused() {
    let sheets = [
        ("  TRACK 01 MODE1/2352\n", OpenError::Cue(1)),
        ("FILE \"Synth.bin\" WAVE\n", OpenError::Cue(1)),
        (
            "FILE \"Synth.bin\" BINARY\nTRACK 01 MODE2/2352\n",
            OpenError::Cue(2),
        ),
        (
            "FILE \"Synth.bin\" BINARY\nTRACK 01 MODE1/2352\n",
            OpenError::Cue(2),
        ),
        (
            "FILE \"Synth.bin\" BINARY\nTRACK 01 MODE1/2352\nINDEX 01 00:60:00\n",
            OpenError::Cue(3),
        ),
        (
            "FILE \"Synth.bin\" BINARY\nTRACK 01 AUDIO\nINDEX 01 00:00:00\n",
            OpenError::NoData,
        ),
        (
            "FILE \"Gone.bin\" BINARY\nTRACK 01 MODE1/2352\nINDEX 01 00:00:00\n",
            OpenError::Missing,
        ),
    ];
    for (sheet, refused) in sheets {
        assert_eq!(
            Disc::open(image_source(sheet)).err(),
            Some(refused),
            "{sheet}"
        );
    }
    let blank = vec![0; SECTORS as usize * 2048];
    assert_eq!(
        Disc::open(Source::Iso(blank)).err(),
        Some(OpenError::Volume)
    );
}
