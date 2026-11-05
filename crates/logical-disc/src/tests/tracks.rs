// SPDX-License-Identifier: MIT
//! The audio side: the tracks on the synthetic image, each read one
//! `disc.call` line, and the store folder through the in-memory folder, a
//! host back being left for later.
use alloc::vec;
use alloc::vec::Vec;

use kernel_files::memory::MemoryFolder;
use kernel_files::{Create, Error, Folder, Mode};

use super::test_log::debug_lines;
use super::{AUDIO, SHEET, fill, image_source, iso};
use crate::disc::RAW;
use crate::{Audio, Disc, Encoded, Format, Source, Track, TrackFiles};

const FRAME: usize = 2352;

/// Every track's start and length to the frame: the data track's 25
/// sectors first, each file's frames after the last's, the pregaps out.
#[test]
fn every_track_starts_and_runs_to_the_frame() {
    let disc = Disc::open(image_source(SHEET)).expect("the image opens");
    let found: Vec<_> = disc
        .tracks()
        .iter()
        .map(|track| match &track.audio {
            Audio::Pcm(pcm) => (track.number, pcm.start, pcm.frames, pcm.big_endian),
            Audio::Encoded(_) => panic!("an image's track is raw"),
        })
        .collect();
    assert_eq!(found, [(2, 35, 30, false), (3, 70, 25, true)]);
    let iso = Disc::open(Source::Iso(iso())).expect("the iso opens");
    assert!(iso.tracks().is_empty());
}

/// A track reads its own frames, from `INDEX 01` to its end, short there;
/// a number with no track is not found; each read is one line.
#[test]
fn a_track_reads_its_own_frames() {
    let disc = Disc::open(image_source(SHEET)).expect("the image opens");
    let two = fill(AUDIO[0].1 * FRAME, AUDIO[0].2);
    let three = fill(AUDIO[1].1 * FRAME, AUDIO[1].2);
    let mut whole = vec![0; 30 * FRAME + 10];
    let mut tail = vec![0; 4000];
    let lines = debug_lines(|| {
        assert_eq!(disc.read_track(2, 0, &mut whole), Ok(30 * FRAME));
        assert_eq!(whole[..30 * FRAME], two[10 * FRAME..]);
        let at = 24 * FRAME + 100;
        assert_eq!(disc.read_track(3, at as u64, &mut tail), Ok(FRAME - 100));
        assert_eq!(tail[..FRAME - 100], three[5 * FRAME + at..]);
        assert_eq!(disc.read_track(1, 0, &mut tail), Err(Error::NotFound));
        assert_eq!(disc.read_track(9, 0, &mut tail), Err(Error::NotFound));
    });
    let reads: Vec<_> = lines
        .iter()
        .filter(|line| line.starts_with("disc.call c=read_track"))
        .collect();
    assert_eq!(reads.len(), 4, "{lines:?}");
    assert_eq!(reads.len(), lines.len(), "{lines:?}");
    assert_eq!(
        reads[0],
        &format!(
            "disc.call c=read_track a=2 p= v={0:x} n={0:x} e=ok",
            30 * FRAME
        )
    );
    assert_eq!(reads[3], "disc.call c=read_track a=9 p= v=0 n=0 e=NotFound");
    assert_eq!(usize::try_from(RAW), Ok(FRAME));
}

/// The rule finds `Track02` on in a store folder, the case
/// folded; the first gap ends the run; the data and each track's bytes
/// come through the folder itself.
#[test]
fn a_store_folders_tracks_follow_its_rule() {
    let rule = TrackFiles {
        stem: "Track",
        first: 2,
        extension: ".ogg",
        format: Format::Ogg,
    };
    let folder = MemoryFolder::default()
        .give("Track01.ogg", b"not a track")
        .give("TRACK02.OGG", b"OggS two")
        .give("track03.ogg", b"OggS three")
        .give("Track04.ogg", b"OggS four")
        .give("Track06.ogg", b"OggS six, past the gap")
        .give("DATA\\LEVEL.DAT", &fill(3000, 2));
    let tracks = rule.find(&folder).expect("the rule finds");
    let numbers: Vec<_> = tracks.iter().map(|track| track.number).collect();
    assert_eq!(numbers, [2, 3, 4]);
    assert_eq!(
        tracks[0],
        Track {
            number: 2,
            audio: Audio::Encoded(Encoded {
                name: b"Track02.ogg".to_vec(),
                format: Format::Ogg,
            }),
        }
    );
    let read = |name: &[u8], length: usize| {
        let mut asked = name.to_vec();
        asked.push(0);
        let asked = core::ffi::CStr::from_bytes_with_nul(&asked).expect("a name");
        let mode = Mode {
            read: true,
            write: false,
            create: Create::Existing,
        };
        let file = folder.open(asked, mode).expect("it opens");
        let mut bytes = vec![0; length];
        let count = folder.read(file, &mut bytes).expect("it reads");
        folder.close(file).expect("it closes");
        bytes.truncate(count as usize);
        bytes
    };
    let Audio::Encoded(three) = &tracks[1].audio else {
        panic!("a store's track is encoded")
    };
    assert_eq!(read(&three.name, 100), b"OggS three");
    assert_eq!(read(b"DATA\\LEVEL.DAT", 4000), fill(3000, 2));
    let none = MemoryFolder::default().give("Music\\Track02.ogg", b"OggS");
    assert_eq!(rule.find(&none), Ok(Vec::new()));
}
