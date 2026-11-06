// SPDX-License-Identifier: MIT
//! A store's Ogg through the door, over a stand-in file made by `ffmpeg` and kept as text in
//! `test_ogg.b64`: 8 pages of 440 Hz on the left and 880 Hz on the right, 44,100 pairs. It
//! came from no game, as `test_ogg_mono.b64`, a second one on one channel, did not. A real
//! store's file is not tested here, and nothing reaches a speaker.

use std::cell::{Cell, RefCell};
use std::ffi::CStr;
use std::io::Cursor;

use kernel_files::{Error, Folder, Kind, Mode, Origin};
use lewton::inside_ogg::OggStreamReader;
use logical_disc::{Audio, Encoded, Format, Track};

use crate::disc::spans;
use crate::door::{DoorPlayer, FRAME_SAMPLES, Samples};
use crate::door_tests::Doors;
use crate::ogg::{OggSamples, last_granule};
use crate::play::Player;
use crate::table::{Place, Tracks};
use crate::test_log::debug_lines;
use crate::tests::Panics;

/// The stand-in's own numbers, by `ffprobe` and by walking its pages.
const PAIRS: usize = 44_100;
const FRAMES: u32 = 75;
const NAME: &str = "TWO.OGG";
const TRACK: u8 = 2;
/// Where three of its pages start: an early one holding sound, and the last two.
const EARLY_PAGE: usize = 4385;
const SECOND_LAST_PAGE: usize = 6465;
const LAST_PAGE: usize = 6882;
/// In its first page: the rate.
const RATE_AT: usize = 40;

fn standin() -> Vec<u8> {
    from_base64(include_str!("test_ogg.b64"))
}

fn mono() -> Vec<u8> {
    from_base64(include_str!("test_ogg_mono.b64"))
}

fn from_base64(kept: &str) -> Vec<u8> {
    const LETTERS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let text: Vec<u8> = kept
        .lines()
        .filter(|line| !line.starts_with('#'))
        .flat_map(str::bytes)
        .filter(|&letter| letter != b'=')
        .collect();
    let mut bytes = Vec::new();
    for group in text.chunks(4) {
        let mut word = 0u32;
        for slot in 0..4 {
            let value = group.get(slot).map_or(0, |letter| {
                LETTERS
                    .iter()
                    .position(|each| each == letter)
                    .expect("base64")
            });
            word = word << 6 | u32::try_from(value).expect("under 64");
        }
        bytes.extend_from_slice(&word.to_be_bytes()[1..group.len()]);
    }
    bytes
}

/// A page's checksum (RFC 3533 section 6).
fn checksum(page: &[u8]) -> u32 {
    page.iter().fold(0, |sum, &byte| {
        (0..8).fold(sum ^ u32::from(byte) << 24, |sum, _| {
            if sum & 0x8000_0000 == 0 {
                sum << 1
            } else {
                sum << 1 ^ 0x04c1_1db7
            }
        })
    })
}

/// The stand-in with one page changed and its checksum made right again.
fn with_page(at: usize, change: impl FnOnce(&mut [u8])) -> Vec<u8> {
    let mut bytes = standin();
    let segments = usize::from(bytes[at + 26]);
    let body: usize = bytes[at + 27..at + 27 + segments]
        .iter()
        .map(|&size| usize::from(size))
        .sum();
    let page = &mut bytes[at..at + 27 + segments + body];
    change(page);
    seal(page);
    bytes
}

fn seal(page: &mut [u8]) {
    page[22..26].fill(0);
    let sum = checksum(page);
    page[22..26].copy_from_slice(&sum.to_le_bytes());
}

/// The stand-in with its last two pages made one, so a seek can land on the last page, where
/// the decoder leaves the last packet uncut.
fn with_a_long_last_page() -> Vec<u8> {
    let bytes = standin();
    let (second_last, last) = (&bytes[SECOND_LAST_PAGE..LAST_PAGE], &bytes[LAST_PAGE..]);
    let (second_count, last_count) = (usize::from(second_last[26]), usize::from(last[26]));
    let mut page = last[..26].to_vec();
    page[18..22].copy_from_slice(&second_last[18..22]);
    page.push(u8::try_from(second_count + last_count).expect("few segments"));
    page.extend_from_slice(&second_last[27..27 + second_count]);
    page.extend_from_slice(&last[27..27 + last_count]);
    page.extend_from_slice(&second_last[27 + second_count..]);
    page.extend_from_slice(&last[27 + last_count..]);
    seal(&mut page);
    [&bytes[..SECOND_LAST_PAGE], &page].concat()
}

/// The stand-in with a byte of an early page's sound changed and its checksum left, so a
/// straight decode must fail there.
fn with_a_bad_early_page() -> Vec<u8> {
    let mut bytes = standin();
    bytes[EARLY_PAGE + 100] ^= 0xff;
    bytes
}

/// The stand-in with the first packet to start on an early page marked as a header, and its
/// checksum right. Its page reads, so a seek's walk over the pages passes it; a straight
/// decode must refuse it, as a header has no place among the sound.
fn with_a_refused_early_packet() -> Vec<u8> {
    with_page(EARLY_PAGE, |page| {
        let segments = usize::from(page[26]);
        let lacing = &page[27..27 + segments];
        let opens_mid_packet = page[5] & 1 == 1;
        let skipped = if opens_mid_packet {
            let end = lacing
                .iter()
                .position(|&size| size < 255)
                .expect("a packet ends here");
            lacing[..=end].iter().map(|&size| usize::from(size)).sum()
        } else {
            0
        };
        page[27 + segments + skipped] |= 1;
    })
}

/// A folder of files in memory, which counts what it is asked.
#[derive(Default)]
struct Files {
    kept: Vec<(String, Vec<u8>)>,
    handles: RefCell<Vec<Option<(usize, usize)>>>,
    opens: Cell<u32>,
    closes: Cell<u32>,
    bytes_read: Cell<usize>,
}

impl Files {
    fn with(files: Vec<(&str, Vec<u8>)>) -> Self {
        Self {
            kept: files
                .into_iter()
                .map(|(name, bytes)| (name.to_owned(), bytes))
                .collect(),
            ..Self::default()
        }
    }

    fn one(bytes: Vec<u8>) -> Self {
        Self::with(vec![(NAME, bytes)])
    }
}

impl Folder for Files {
    fn open(&self, name: &CStr, _: Mode) -> Result<u32, Error> {
        let name = name.to_str().map_err(|_| Error::BadArgument)?;
        let file = self
            .kept
            .iter()
            .position(|(kept, _)| kept == name)
            .ok_or(Error::Other(2))?;
        self.opens.set(self.opens.get() + 1);
        let mut handles = self.handles.borrow_mut();
        handles.push(Some((file, 0)));
        Ok(u32::try_from(handles.len() - 1).expect("few handles"))
    }

    fn read(&self, file: u32, into: &mut [u8]) -> Result<u32, Error> {
        let mut handles = self.handles.borrow_mut();
        let (kept, at) = handles[file as usize].as_mut().ok_or(Error::BadHandle)?;
        let bytes = &self.kept[*kept].1;
        let moved = into.len().min(bytes.len().saturating_sub(*at));
        into[..moved].copy_from_slice(&bytes[*at..*at + moved]);
        *at += moved;
        self.bytes_read.set(self.bytes_read.get() + moved);
        Ok(u32::try_from(moved).expect("a small read"))
    }

    fn seek(&self, file: u32, distance: i32, origin: Origin) -> Result<u32, Error> {
        let mut handles = self.handles.borrow_mut();
        let (kept, at) = handles[file as usize].as_mut().ok_or(Error::BadHandle)?;
        let base = match origin {
            Origin::Start => 0,
            Origin::Here => *at,
            Origin::End => self.kept[*kept].1.len(),
        };
        let to = base
            .checked_add_signed(distance as isize)
            .ok_or(Error::BadSeek)?;
        *at = to;
        Ok(u32::try_from(to).expect("a small file"))
    }

    fn close(&self, file: u32) -> Result<(), Error> {
        self.handles.borrow_mut()[file as usize]
            .take()
            .ok_or(Error::BadHandle)?;
        self.closes.set(self.closes.get() + 1);
        Ok(())
    }

    fn kind(&self, _: u32) -> Result<Kind, Error> {
        unreachable!("a decoder asks no file's kind")
    }

    fn write(&self, _: u32, _: &[u8]) -> Result<u32, Error> {
        unreachable!("a decoder writes nothing")
    }

    fn delete(&self, _: &CStr) -> Result<(), Error> {
        unreachable!("a decoder deletes nothing")
    }

    fn rename(&self, _: &CStr, _: &CStr) -> Result<(), Error> {
        unreachable!("a decoder renames nothing")
    }

    fn find_first(&self, _: &CStr, _: &mut [u8]) -> Result<u32, Error> {
        unreachable!("a decoder finds nothing")
    }

    fn find_next(&self, _: u32, _: &mut [u8]) -> Result<(), Error> {
        unreachable!("a decoder finds nothing")
    }

    fn find_close(&self, _: u32) -> Result<(), Error> {
        unreachable!("a decoder finds nothing")
    }

    fn fail(&self, _: u32) {}
}

fn store_tracks() -> Vec<Track> {
    vec![Track {
        number: TRACK,
        audio: Audio::Encoded(Encoded {
            name: NAME.as_bytes().to_vec(),
            format: Format::Ogg,
        }),
    }]
}

fn place(frame: u32) -> Place {
    Place {
        track: TRACK,
        frame,
    }
}

/// The stand-in as `lewton` gives it with nothing of this crate's in the way.
fn plain_decode() -> Vec<i16> {
    let mut reader = OggStreamReader::new(Cursor::new(standin())).expect("the stand-in opens");
    let mut numbers = Vec::new();
    while let Some(packet) = reader.read_dec_packet_itl().expect("the stand-in decodes") {
        numbers.extend(packet);
    }
    numbers
}

fn crossings(numbers: impl Iterator<Item = i16>) -> usize {
    let signs: Vec<bool> = numbers.map(|number| number >= 0).collect();
    signs.windows(2).filter(|pair| pair[0] != pair[1]).count()
}

#[test]
fn a_files_length_is_its_last_granule_in_whole_frames() {
    let files = Files::one(standin());
    let tracks = store_tracks();
    let samples = OggSamples::new(&files, &tracks);
    assert_eq!(
        spans(&Panics, &tracks, |file| samples.measure(file))[0].frames,
        FRAMES
    );
}

#[test]
fn a_length_between_frames_rounds_down_and_the_read_ends_on_it() {
    let files = Files::one(with_page(LAST_PAGE, |page| {
        page[6..14].copy_from_slice(&44_000u64.to_le_bytes());
    }));
    let tracks = store_tracks();
    let mut samples = OggSamples::new(&files, &tracks);
    let Audio::Encoded(file) = &tracks[0].audio else {
        unreachable!("the store's track")
    };
    assert_eq!(samples.measure(file), Some(74));
    let mut into = vec![0; PAIRS * 4];
    assert_eq!(samples.read(place(0), &mut into), Some(88_000));
}

#[test]
fn a_read_never_passes_the_length_though_a_last_page_decodes_to_more() {
    // With its end-of-stream mark gone, the decoder has no word to cut the last packet.
    let files = Files::one(with_page(LAST_PAGE, |page| page[5] = 0));
    let tracks = store_tracks();
    let mut samples = OggSamples::new(&files, &tracks);
    let mut into = vec![0; PAIRS * 4];
    assert_eq!(samples.read(place(0), &mut into), Some(PAIRS * 2));
}

#[test]
fn a_straight_read_gives_the_plain_decode_and_the_two_tones() {
    let files = Files::one(standin());
    let tracks = store_tracks();
    let mut samples = OggSamples::new(&files, &tracks);
    let mut heard = Vec::new();
    for frame in (0..FRAMES).step_by(7) {
        let mut into = vec![0; 7 * FRAME_SAMPLES];
        let moved = samples.read(place(frame), &mut into).expect("it reads");
        heard.extend_from_slice(&into[..moved]);
    }
    assert_eq!(heard.len(), PAIRS * 2);
    assert_eq!(heard, plain_decode());
    let left = crossings(heard.iter().step_by(2).copied());
    let right = crossings(heard.iter().skip(1).step_by(2).copied());
    assert!(left.abs_diff(880) < 20, "left crossed {left} times");
    assert!(right.abs_diff(1760) < 40, "right crossed {right} times");
}

#[test]
fn every_frame_reads_the_same_by_a_seek_as_straight_through() {
    let straight = plain_decode();
    let files = Files::one(standin());
    let tracks = store_tracks();
    let mut shared = OggSamples::new(&files, &tracks);
    // Every frame in turn from a new start, then by a jump from wherever the last one left it.
    let frames = (0..FRAMES).rev().chain([60, 3, 74, 0, 40, 41, 12]);
    for frame in frames {
        let mut fresh = OggSamples::new(&files, &tracks);
        let at = frame as usize * FRAME_SAMPLES;
        let want = &straight[at..straight.len().min(at + 3 * FRAME_SAMPLES)];
        for samples in [&mut fresh, &mut shared] {
            let mut into = vec![0; 3 * FRAME_SAMPLES];
            let moved = samples.read(place(frame), &mut into).expect("it reads");
            assert_eq!(&into[..moved], want, "frame {frame}");
        }
    }
}

#[test]
fn a_resume_past_a_bad_page_reads_so_it_sought_and_did_not_decode_from_the_start() {
    let straight = plain_decode();
    // A broken checksum will not serve: the library's seek reads every page of a file this
    // small, and refuses that one. See `a_broken_checksum_refuses_a_seek_as_well`.
    let files = Files::one(with_a_refused_early_packet());
    let tracks = store_tracks();
    let mut samples = OggSamples::new(&files, &tracks);
    let mut into = vec![0; 3 * FRAME_SAMPLES];
    assert_eq!(
        samples.read(place(0), &mut vec![0; PAIRS * 2]),
        None,
        "the bad packet is there"
    );
    let moved = samples
        .read(place(50), &mut into)
        .expect("the seek skips it");
    let at = 50 * FRAME_SAMPLES;
    assert_eq!(&into[..moved], &straight[at..at + moved]);
    assert_eq!(moved, 3 * FRAME_SAMPLES);
}

#[test]
fn a_seek_that_lands_on_the_last_page_looks_further_back() {
    let straight = plain_decode();
    let files = Files::one(with_a_long_last_page());
    let tracks = store_tracks();
    // That page holds frames 62 to 75; each of these aims a seek inside it.
    for frame in 66..FRAMES {
        let mut samples = OggSamples::new(&files, &tracks);
        let at = frame as usize * FRAME_SAMPLES;
        let want = &straight[at..straight.len().min(at + 3 * FRAME_SAMPLES)];
        let mut into = vec![0; 3 * FRAME_SAMPLES];
        let moved = samples.read(place(frame), &mut into).expect("it reads");
        assert_eq!(&into[..moved], want, "frame {frame}");
    }
}

#[test]
fn a_seek_that_lands_past_its_place_looks_further_back() {
    // The page a seek for this frame lands on claims more than it holds, so it reckons it
    // stands past the place. Trusted, the read would hand over the wrong sound.
    let files = Files::one(with_page(EARLY_PAGE, |page| {
        page[6..14].copy_from_slice(&15_000u64.to_le_bytes());
    }));
    let tracks = store_tracks();
    let mut samples = OggSamples::new(&files, &tracks);
    let mut into = vec![0; 3 * FRAME_SAMPLES];
    let moved = samples.read(place(14), &mut into).expect("it reads");
    let at = 14 * FRAME_SAMPLES;
    assert_eq!(&into[..moved], &plain_decode()[at..at + moved]);
    assert_eq!(moved, 3 * FRAME_SAMPLES);
}

#[test]
fn a_second_seek_forgets_what_the_first_held() {
    // Both of this frame's tries land on the one page, which claims too much. The second
    // try, with the first's sound still held, would reckon it stands before the place and
    // hand that old sound over.
    const FRAME: u32 = 40;
    let files = Files::one(with_page(EARLY_PAGE, |page| {
        page[6..14].copy_from_slice(&31_000u64.to_le_bytes());
    }));
    let tracks = store_tracks();
    let mut samples = OggSamples::new(&files, &tracks);
    let mut into = vec![0; 3 * FRAME_SAMPLES];
    let moved = samples.read(place(FRAME), &mut into).expect("it reads");
    let at = FRAME as usize * FRAME_SAMPLES;
    assert_eq!(moved, 3 * FRAME_SAMPLES);
    assert_eq!(&into[..moved], &plain_decode()[at..at + moved]);
}

#[test]
fn a_read_of_another_track_at_the_same_place_does_not_go_on() {
    let files = Files::one(standin());
    let tracks = store_tracks();
    let mut samples = OggSamples::new(&files, &tracks);
    samples
        .read(place(0), &mut vec![0; 7 * FRAME_SAMPLES])
        .expect("it reads");
    assert_eq!(
        samples.read(Place { track: 9, frame: 7 }, &mut [0; FRAME_SAMPLES]),
        None
    );
}

#[test]
fn a_read_after_one_that_failed_starts_again() {
    let files = Files::one(with_a_refused_early_packet());
    let tracks = store_tracks();
    let mut samples = OggSamples::new(&files, &tracks);
    assert_eq!(samples.read(place(0), &mut vec![0; PAIRS * 2]), None);
    assert_eq!(files.opens.get(), 1);
    let mut into = [0; FRAME_SAMPLES];
    let moved = samples
        .read(place(0), &mut into)
        .expect("its head is sound");
    assert_eq!(files.opens.get(), 2, "the failed run is gone");
    assert_eq!(&into[..moved], &plain_decode()[..moved]);
}

#[test]
fn a_long_files_length_reads_only_its_tail() {
    const PADDING: usize = 200_000;
    let mut bytes = standin();
    bytes.splice(LAST_PAGE..LAST_PAGE, vec![0; PADDING]);
    let files = Files::one(bytes);
    let tracks = store_tracks();
    let samples = OggSamples::new(&files, &tracks);
    let Audio::Encoded(file) = &tracks[0].audio else {
        unreachable!("the store's track")
    };
    assert_eq!(samples.measure(file), Some(FRAMES));
    let read = files.bytes_read.get();
    assert!(
        read < PADDING / 2,
        "read {read} bytes of a file past {PADDING}"
    );
}

#[test]
fn a_broken_checksum_refuses_a_seek_as_well() {
    let files = Files::one(with_a_bad_early_page());
    let tracks = store_tracks();
    let mut samples = OggSamples::new(&files, &tracks);
    assert_eq!(samples.read(place(50), &mut [0; FRAME_SAMPLES]), None);
}

#[test]
fn a_read_that_follows_keeps_the_file_and_a_jump_lets_the_last_go() {
    let files = Files::one(standin());
    let tracks = store_tracks();
    let mut samples = OggSamples::new(&files, &tracks);
    let mut into = vec![0; 2 * FRAME_SAMPLES];
    for frame in [0, 2, 4, 6] {
        samples.read(place(frame), &mut into).expect("it reads");
    }
    assert_eq!(files.opens.get(), 1, "a read that follows opens nothing");
    samples.read(place(50), &mut into).expect("it reads");
    assert_eq!(files.opens.get(), 2, "a seek opens the file once");
    assert_eq!(files.closes.get(), 1);
    drop(samples);
    assert_eq!(files.closes.get(), 2, "nothing left open");
}

fn bad_files() -> Vec<(&'static str, Vec<u8>)> {
    let half = standin().len() / 2 + 1;
    vec![
        ("no bytes", Vec::new()),
        ("not an Ogg", vec![0x55; 4096]),
        ("cut in half", standin()[..half].to_vec()),
        (
            "another rate",
            with_page(0, |page| {
                page[RATE_AT..RATE_AT + 4].copy_from_slice(&22_050u32.to_le_bytes());
            }),
        ),
        ("one channel", mono()),
        (
            "another stream's last page",
            with_page(LAST_PAGE, |page| page[14] ^= 0xff),
        ),
        (
            "a last page with no granule",
            with_page(LAST_PAGE, |page| page[6..14].fill(0xff)),
        ),
    ]
}

#[test]
fn a_bad_ogg_measures_as_nothing_and_reads_as_nothing() {
    for (what, bytes) in bad_files() {
        let files = Files::one(bytes);
        let tracks = store_tracks();
        let mut samples = OggSamples::new(&files, &tracks);
        let Audio::Encoded(file) = &tracks[0].audio else {
            unreachable!("the store's track")
        };
        assert_eq!(samples.measure(file), None, "{what}");
        assert_eq!(
            samples.read(place(0), &mut [0; FRAME_SAMPLES]),
            None,
            "{what}"
        );
        drop(samples);
        assert_eq!(files.opens.get(), files.closes.get(), "{what}: left open");
    }
}

#[test]
fn a_file_the_folder_lacks_and_a_track_not_a_stores_read_as_nothing() {
    let files = Files::default();
    let tracks = store_tracks();
    let mut samples = OggSamples::new(&files, &tracks);
    assert_eq!(samples.read(place(0), &mut [0; FRAME_SAMPLES]), None);
    assert_eq!(
        samples.read(Place { track: 9, frame: 0 }, &mut [0; FRAME_SAMPLES]),
        None
    );
}

#[test]
fn a_name_holding_a_nul_reads_as_nothing_and_no_fault() {
    let files = Files::one(standin());
    let tracks = vec![Track {
        number: TRACK,
        audio: Audio::Encoded(Encoded {
            name: b"TWO\0.OGG".to_vec(),
            format: Format::Ogg,
        }),
    }];
    let mut samples = OggSamples::new(&files, &tracks);
    assert_eq!(samples.read(place(0), &mut [0; FRAME_SAMPLES]), None);
}

fn page_of(granule: u64, serial: u8, body: &[u8]) -> Vec<u8> {
    let mut page = b"OggS\0\0".to_vec();
    page.extend_from_slice(&granule.to_le_bytes());
    page.extend_from_slice(&[serial, 0, 0, 0]);
    page.extend_from_slice(&[0; 8]);
    page.push(1);
    page.push(u8::try_from(body.len()).expect("a short body"));
    page.extend_from_slice(body);
    page
}

#[test]
fn the_last_page_is_the_one_that_fits_the_end_not_any_capture_pattern() {
    const SERIAL: [u8; 4] = [7, 0, 0, 0];
    let lookalike = b"OggS\0 not a page at all, whatever it may hold";
    let tail = [page_of(100, 7, b"x"), page_of(200, 7, lookalike)].concat();
    assert_eq!(last_granule(&tail, &SERIAL), Some(200));
    let trailing = [page_of(200, 7, b"x"), b"junk".to_vec()].concat();
    assert_eq!(last_granule(&trailing, &SERIAL), None);
    assert_eq!(last_granule(&page_of(200, 8, b"x"), &SERIAL), None);
    assert_eq!(last_granule(&page_of(u64::MAX, 7, b"x"), &SERIAL), None);
    assert_eq!(last_granule(&[], &SERIAL), None);
    // Two that fit: the nearer the end, which is the scan's own order.
    let nested = page_of(100, 7, &page_of(200, 7, b"x"));
    assert_eq!(last_granule(&nested, &SERIAL), Some(200));
}

#[test]
fn a_stores_ogg_reaches_the_door_as_it_decodes_and_resumes_mid_track() {
    let straight = plain_decode();
    let files = Files::one(standin());
    let tracks = store_tracks();
    let samples = OggSamples::new(&files, &tracks);
    let table = spans(&Panics, &tracks, |file| samples.measure(file));
    let doors = Doors::default();
    let ms = Cell::new(0u32);
    let player = DoorPlayer::new(Panics, &doors, samples, Tracks(&table), || ms.get());
    player.play(place(0), place(FRAMES));
    assert_eq!(
        doors.pushed.borrow().as_slice(),
        &straight[..18 * FRAME_SAMPLES]
    );
    player.pause();
    doors.pushed.borrow_mut().clear();
    player.play(place(40), place(FRAMES));
    for step in 1..20 {
        ms.set(step * 100);
        player.pump();
    }
    assert_eq!(
        doors.pushed.borrow().as_slice(),
        &straight[40 * FRAME_SAMPLES..],
        "a resume reaches the door from its own frame to the file's end"
    );
    player.stop();
    assert_eq!(doors.live.get(), None);
}

#[test]
fn a_bad_ogg_at_the_start_is_one_line_and_a_track_with_no_frames() {
    for (what, bytes) in bad_files() {
        let files = Files::one(bytes);
        let tracks = store_tracks();
        let samples = OggSamples::new(&files, &tracks);
        let mut table = Vec::new();
        let lines = debug_lines(|| table = spans(&Panics, &tracks, |file| samples.measure(file)));
        assert_eq!(table[0].frames, 0, "{what}");
        assert_eq!(lines.len(), 1, "{what}: {lines:?}");
        assert!(lines[0].starts_with("cd.track.unread"), "{what}: {lines:?}");
    }
}

#[test]
fn a_file_going_bad_mid_play_is_one_line_and_silence_and_the_door_stays_open() {
    let files = Files::one(with_a_bad_early_page());
    let tracks = store_tracks();
    let samples = OggSamples::new(&files, &tracks);
    let table = spans(&Panics, &tracks, |file| samples.measure(file));
    assert_eq!(table[0].frames, FRAMES, "its head and tail are sound");
    let doors = Doors::default();
    let ms = Cell::new(0u32);
    let player = DoorPlayer::new(Panics, &doors, samples, Tracks(&table), || ms.get());
    let lines = debug_lines(|| {
        player.play(place(0), place(FRAMES));
        for step in 1..20 {
            ms.set(step * 100);
            player.pump();
        }
    });
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].starts_with("cd.track.unread"), "{lines:?}");
    assert!(doors.live.get().is_some());
    player.stop();
    assert_eq!(doors.live.get(), None);
}

/// The names of the features whose rows hold `dep`.
fn features_naming<'m>(manifest: &'m str, dep: &str) -> Vec<&'m str> {
    manifest
        .lines()
        .filter(|line| line.contains(dep))
        .filter_map(|line| line.split(' ').next())
        .collect()
}

#[test]
fn lewton_stands_pinned_behind_its_feature_and_off_the_browsers_target() {
    let manifest = include_str!("../Cargo.toml");
    let mut section = "";
    let mut rows = Vec::new();
    for line in manifest.lines() {
        if line.starts_with('[') {
            section = line;
        } else if line.starts_with("lewton") {
            rows.push((section, line));
        }
    }
    assert_eq!(
        rows,
        [
            (
                r#"[target.'cfg(not(target_arch = "wasm32"))'.dependencies]"#,
                r#"lewton = { version = "=0.10.2", optional = true }"#,
            ),
            (
                r#"[target.'cfg(not(target_arch = "wasm32"))'.dev-dependencies]"#,
                r#"lewton = "=0.10.2""#,
            ),
        ]
    );
    assert_eq!(features_naming(manifest, "dep:lewton"), ["ogg"]);
    assert_eq!(features_naming(manifest, "dep:sound-mix"), ["door"]);
    assert!(manifest.contains(r#"disc = ["dep:logical_disc", "logical_disc/disc"]"#));
}

#[test]
fn the_samples_can_move_to_another_thread() {
    fn moves<T: Send>() {}
    // Never called: that it compiles, for every folder that may be shared, is the fence.
    #[allow(dead_code)]
    fn fence<F: Folder + Sync + 'static>() {
        moves::<OggSamples<'static, &'static F>>();
    }
}
