// SPDX-License-Identifier: MIT
//! A disc made from a formula, and the walk that judges what came out of the mix against it.
//! Taken by `#[path]` from the test and from the route, and it names no route. Nothing on the
//! disc came from any game.
//!
//! The tones are integers, so a second program in another language can work the same answer.
//! Each pair's place shows in its value: a wrong track or a wrong frame cannot read right.
//!
//! The walk knows only what the script asked for and what the mix handed over. Each pair it
//! expects is next; a pair it does not expect is searched for on the disc, and the place
//! found decides what happened: the next run's start, a hole the clock explains, or a wrong
//! sound.
//!
//! Hazard: a wrong sound is judged by where it was found, so a stream no window of which is on
//! the disc says only the place it was expected.
//!
//! Hazard: `pump_max_ms` is this bench's own reading from outside the player, on this box.
#![allow(
    dead_code,
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "each caller takes its part of this file, and the worked numbers fit by their formulas"
)]

#[path = "../src/test_disc.rs"]
mod test_disc;

use std::cell::{Cell, RefCell};
use std::ffi::CStr;
use std::io::Cursor;
use std::rc::Rc;
use std::sync::mpsc;
use std::thread::{self, sleep};
use std::time::{Duration, Instant};

use cd_audio::disc::spans;
use cd_audio::door::{DoorPlayer, FRAME_SAMPLES, ImageSamples, LEAD_FRAMES, Samples};
use cd_audio::ogg::OggSamples;
use cd_audio::play::PlayFront;
use cd_audio::table::{Calls, Place, Row, Span, Tracks};
use cd_audio::tmsf::{pack, unpack};
use cd_audio::{Back, Record};
use kernel_files::{Error, Folder, Kind, Mode, Origin};
use lewton::inside_ogg::OggStreamReader;
use logical_disc::{Audio, Disc, Encoded, Format, ReadAt, Source, Track};
use sound_mix::output::Output;
use sound_mix::table::Row as MixRow;
use sound_mix::witness::{HASH_START, Witness, hash_samples};
use sound_mix::{Front as Mix, Host};

pub const PAIRS_A_FRAME: u32 = 588;
const PAIRS: u64 = PAIRS_A_FRAME as u64;
const FRAME_BYTES: usize = PAIRS_A_FRAME as usize * 4;
/// How far past the clock the door is fed, in milliseconds.
pub const LEAD_MS: u64 = LEAD_FRAMES as u64 * 1_000 / 75;
/// The mix's 10 ms tick and a timer's reach, set before any data and not moved by it.
const TOLERANCE: u64 = 3 * PAIRS;
const WINDOW_PAIRS: usize = 32;
const TICK: Duration = Duration::from_millis(10);
const TICK_US: u64 = 10_000;
const MIX_HZ: u32 = 44_100;
/// How long the back may take to play out what it holds, once the script is done.
const TAIL: Duration = Duration::from_secs(2);
/// A read on a truthful file ends far inside this.
const SEEK_DEADLINE: Duration = Duration::from_secs(10);
/// The mix's own report's unit, in frames.
const BLOCK_FRAMES: u64 = 1_024;

const ID: u32 = 7;
const PLAY: u32 = 0x806;
const STOP: u32 = 0x808;
const PAUSE: u32 = 0x809;
const STATUS: u32 = 0x814;
const RESUME: u32 = 0x855;
const FROM: u32 = 4;
const TO: u32 = 8;
const STATUS_ITEM: u32 = 0x100;
const ITEM_POSITION: u32 = 2;
const ITEM_MODE: u32 = 4;
const MODE_STOPPED: u32 = 525;

const WORD_RETURN: usize = 1;
const WORD_ITEM: usize = 2;
const WORD_FROM: usize = 1;
const WORD_TO: usize = 2;

/// The 2 frames of silence before track 2's `INDEX 01`, so a pregap misread shows as a wrong place.
const PREGAP_FRAMES: usize = 2;
/// What the planted table claims of track 2.
const SHORT_TRACK_FRAMES: u32 = 100;
/// The planted reader's frames past where it is asked.
const PLACE_PLANT_FRAMES: u32 = 7;

const OGG_NAME: &str = "TWO.OGG";
const OGG_TRACK: u8 = 2;
/// Frame 30 of the short file, where the seek arms read.
const SEEK_FRAME: u32 = 30;
/// The seeks of a file's first look, `length()`'s three: the lying folder answers those truly.
const TRUE_SEEKS: u32 = 3;

static ROW: Row = Row {
    file: "a made disc",
    calls: Calls {
        send_command: "WINMM!mciSendCommandA",
        aux_devices: "WINMM!auxGetNumDevs",
        error_string: "WINMM!mciGetErrorStringA",
        aux_caps: "WINMM!auxGetDevCapsA",
        aux_set_volume: "WINMM!auxSetVolume",
        aux_get_volume: "WINMM!auxGetVolume",
    },
    letters: 1,
    tracks: 5,
    device_name: "cdaudio",
    device_id: ID,
};

static MIX_ROW: MixRow = MixRow {
    file: "a made disc",
    module: "made",
};

/// One audio track of the made disc.
struct Made {
    number: u8,
    frames: u32,
    left: u32,
    right: u32,
    file: &'static str,
    big_endian: bool,
}

const MADE: [Made; 4] = [
    Made {
        number: 2,
        frames: 150,
        left: 101,
        right: 73,
        file: "TWO.BIN",
        big_endian: false,
    },
    Made {
        number: 3,
        frames: 150,
        left: 89,
        right: 67,
        file: "THREE.BIN",
        big_endian: true,
    },
    Made {
        number: 4,
        frames: 225,
        left: 79,
        right: 61,
        file: "FOUR.BIN",
        big_endian: false,
    },
    Made {
        number: 5,
        frames: 75,
        left: 71,
        right: 59,
        file: "FIVE.BIN",
        big_endian: false,
    },
];

const MADE_TABLE: [Span; 4] = [
    Span {
        number: 2,
        frames: 150,
    },
    Span {
        number: 3,
        frames: 150,
    },
    Span {
        number: 4,
        frames: 225,
    },
    Span {
        number: 5,
        frames: 75,
    },
];

/// Track 1 is `test_disc`'s data track, as `Disc::open` refuses a disc with no volume.
const SHEET: &str = "FILE \"DATA.BIN\" BINARY\n  TRACK 01 MODE1/2352\n    INDEX 01 00:00:00\n\
FILE \"TWO.BIN\" BINARY\n  TRACK 02 AUDIO\n    INDEX 00 00:00:00\n    INDEX 01 00:00:02\n\
FILE \"THREE.BIN\" MOTOROLA\n  TRACK 03 AUDIO\n    INDEX 01 00:00:00\n\
FILE \"FOUR.BIN\" BINARY\n  TRACK 04 AUDIO\n    INDEX 01 00:00:00\n\
FILE \"FIVE.BIN\" BINARY\n  TRACK 05 AUDIO\n    INDEX 01 00:00:00\n";

/// A triangle wave of `period` pairs and height `level`, in whole numbers.
fn tri(n: u32, period: u32, level: u32) -> i16 {
    let q = 4 * (n % period) * level / period;
    let value = if q < level {
        q as i32
    } else if q < 3 * level {
        (2 * level) as i32 - q as i32
    } else {
        q as i32 - (4 * level) as i32
    };
    value as i16
}

/// The height at disc frame `frame`: a 64-frame stair, so a place is told by its height too.
fn level(frame: u32) -> u32 {
    6_000 + (frame % 64) * 64
}

fn made(track: u8) -> &'static Made {
    MADE.iter()
        .find(|each| each.number == track)
        .expect("a track of the made disc")
}

/// Pair `n` of `track`, counted from its `INDEX 01`.
#[must_use]
pub fn recipe(track: u8, n: u32) -> (i16, i16) {
    let track = made(track);
    let height = level(n / PAIRS_A_FRAME);
    (tri(n, track.left, height), tri(n, track.right, height))
}

/// FNV-1a 64 over a whole track.
#[must_use]
pub fn track_hash(track: u8) -> u64 {
    let pairs = made(track).frames * PAIRS_A_FRAME;
    (0..pairs).fold(HASH_START, |hash, n| {
        let (left, right) = recipe(track, n);
        hash_samples(hash, &[left, right])
    })
}

/// A track's file: its pregap, if any, then its pairs in the file's own order.
fn track_file(track: &Made) -> Vec<u8> {
    let pregap = if track.number == 2 { PREGAP_FRAMES } else { 0 };
    let mut bytes = vec![0; pregap * FRAME_BYTES];
    for n in 0..track.frames * PAIRS_A_FRAME {
        let (left, right) = recipe(track.number, n);
        for sample in [left, right] {
            bytes.extend_from_slice(&if track.big_endian {
                sample.to_be_bytes()
            } else {
                sample.to_le_bytes()
            });
        }
    }
    bytes
}

/// A file's bytes, in memory.
pub struct Bytes(Vec<u8>);

impl ReadAt for Bytes {
    fn size(&self) -> u64 {
        self.0.len() as u64
    }

    fn read_at(&self, at: u64, into: &mut [u8]) -> Result<(), Error> {
        let from = usize::try_from(at).map_err(|_| Error::BadArgument)?;
        let bytes = self
            .0
            .get(from..from + into.len())
            .ok_or(Error::BadArgument)?;
        into.copy_from_slice(bytes);
        Ok(())
    }
}

fn made_disc() -> Disc<Bytes> {
    let mut files: Vec<(String, Bytes)> = MADE
        .iter()
        .map(|track| (track.file.to_owned(), Bytes(track_file(track))))
        .collect();
    files.push(("DATA.BIN".to_owned(), Bytes(test_disc::raw_track())));
    Disc::open(Source::Image {
        cue: Bytes(SHEET.as_bytes().to_vec()),
        files,
    })
    .expect("the made disc opens")
}

/// The answer a stream is judged by, every place counted in pairs across the true table.
pub enum Truth {
    Recipe,
    Decoded { table: [Span; 1], samples: Vec<i16> },
}

impl Truth {
    fn table(&self) -> &[Span] {
        match self {
            Self::Recipe => &MADE_TABLE,
            Self::Decoded { table, .. } => table,
        }
    }

    fn total(&self) -> u64 {
        u64::from(Tracks(self.table()).end()) * PAIRS
    }

    fn pair(&self, mut count: u64) -> Option<(i16, i16)> {
        match self {
            Self::Recipe => {
                for span in &MADE_TABLE {
                    let pairs = u64::from(span.frames) * PAIRS;
                    if count < pairs {
                        return Some(recipe(span.number, count as u32));
                    }
                    count -= pairs;
                }
                None
            }
            Self::Decoded { samples, .. } => {
                let at = usize::try_from(count).ok()? * 2;
                let pair = samples.get(at..at + 2)?;
                Some((pair[0], pair[1]))
            }
        }
    }

    fn place(&self, count: u64) -> Place {
        Tracks(self.table()).place((count / PAIRS) as u32)
    }
}

/// Where `stream`'s opening is on the disc. A pad of zeros ends the window: it is no sound.
fn locate(truth: &Truth, stream: &[(i16, i16)]) -> Option<u64> {
    let window: Vec<(i16, i16)> = stream
        .iter()
        .take(WINDOW_PAIRS)
        .enumerate()
        .take_while(|(index, pair)| *index == 0 || **pair != (0, 0))
        .map(|(_, pair)| *pair)
        .collect();
    (0..truth.total()).find(|&start| {
        window
            .iter()
            .zip(start..)
            .all(|(pair, count)| truth.pair(count) == Some(*pair))
    })
}

/// The clock of a run: the bench moves it, or real time does.
pub trait Time {
    fn now_us(&self) -> u64;
    /// One step of the loop, 10 ms.
    fn tick(&self);
}

/// A clock only the bench moves.
#[derive(Default)]
pub struct Held(Cell<u64>);

impl Time for Held {
    fn now_us(&self) -> u64 {
        self.0.get()
    }

    fn tick(&self) {
        self.0.set(self.0.get() + TICK_US);
    }
}

/// Real time, from when it was made.
pub struct Real(Instant);

impl Real {
    #[must_use]
    pub fn new() -> Self {
        Self(Instant::now())
    }
}

impl Time for Real {
    fn now_us(&self) -> u64 {
        self.0.elapsed().as_micros() as u64
    }

    fn tick(&self) {
        sleep(TICK);
    }
}

fn now_ms(time: &dyn Time) -> u32 {
    (time.now_us() / 1_000) as u32
}

struct Hosted(Rc<dyn Time>);

impl Host for Hosted {
    fn now_us(&self) -> u64 {
        self.0.now_us()
    }

    fn call_back(&self, _callback: u32, _handle: u32) {}

    fn read_file(&self, _name: &str) -> Option<u32> {
        None
    }

    fn free_block(&self, _block: u32) {}
}

/// A stop ends a run where a target ends the program, and an arm that dies reads as dead.
struct Quit;

impl Back for Quit {
    fn stop(&self, name: &str) -> ! {
        panic!("stopped: {name}")
    }
}

/// One block the mix handed over: where it starts in the stream, and the clock then.
struct Block {
    at: usize,
    ms: u32,
}

/// What the mix handed over.
#[derive(Default)]
struct Stream {
    samples: Vec<i16>,
    blocks: Vec<Block>,
    took: u32,
    kept: u64,
}

/// Keeps each block and hands it on. Every answer is the back's own.
struct Tap<O> {
    back: Rc<RefCell<O>>,
    stream: Rc<RefCell<Stream>>,
    time: Rc<dyn Time>,
}

impl<O: Output> Output for Tap<O> {
    fn open(&mut self, rate_hz: u32) -> u32 {
        let took = self.back.borrow_mut().open(rate_hz);
        self.stream.borrow_mut().took = took;
        took
    }

    fn room(&self) -> usize {
        self.back.borrow().room()
    }

    fn write(&mut self, frames: &[i16]) {
        let room = self.back.borrow().room();
        let mut stream = self.stream.borrow_mut();
        let at = stream.samples.len() / 2;
        stream.blocks.push(Block {
            at,
            ms: now_ms(&*self.time),
        });
        stream.samples.extend_from_slice(frames);
        stream.kept += (frames.len() / 2).min(room) as u64;
        drop(stream);
        self.back.borrow_mut().write(frames);
    }

    fn close(&mut self) {
        self.back.borrow_mut().close();
    }
}

/// A record's words, on no pointer.
struct Words([u32; 5]);

impl Record for Words {
    fn word(&self, index: usize) -> u32 {
        self.0[index]
    }

    fn set_word(&mut self, index: usize, value: u32) {
        self.0[index] = value;
    }

    fn write_through(&mut self, _: usize, _: &[&[u8]]) {
        unreachable!("no call of a script writes through a pointer")
    }
}

/// Whichever reader an arm chose.
struct Picked<'s>(Box<dyn Samples + 's>);

impl Samples for Picked<'_> {
    fn read(&mut self, place: Place, into: &mut [i16]) -> Option<usize> {
        self.0.read(place, into)
    }
}

/// The plant: reads another track where asked for one.
struct ReadsTrack<S> {
    inner: S,
    asked: u8,
    read: u8,
}

impl<S: Samples> Samples for ReadsTrack<S> {
    fn read(&mut self, place: Place, into: &mut [i16]) -> Option<usize> {
        let track = if place.track == self.asked {
            self.read
        } else {
            place.track
        };
        self.inner.read(
            Place {
                track,
                frame: place.frame,
            },
            into,
        )
    }
}

/// The plant: reads further on than asked.
struct ReadsOn<S> {
    inner: S,
    frames: u32,
}

impl<S: Samples> Samples for ReadsOn<S> {
    fn read(&mut self, place: Place, into: &mut [i16]) -> Option<usize> {
        self.inner.read(
            Place {
                track: place.track,
                frame: place.frame + self.frames,
            },
            into,
        )
    }
}

/// What a folder does with a seek past a file's first look.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeekLie {
    Truth,
    /// Lands at byte 0, answering as asked: a tell rewinds the file.
    Rewinds,
    /// Moves nothing, answering as asked.
    Stays,
}

struct Handle {
    file: usize,
    at: usize,
    seeks: u32,
}

/// A folder of files in memory, whose seeks may lie.
pub struct MemFolder {
    kept: Vec<(String, Vec<u8>)>,
    lie: SeekLie,
    handles: RefCell<Vec<Option<Handle>>>,
}

impl MemFolder {
    #[must_use]
    pub fn one(name: &str, bytes: Vec<u8>, lie: SeekLie) -> Self {
        Self {
            kept: vec![(name.to_owned(), bytes)],
            lie,
            handles: RefCell::new(Vec::new()),
        }
    }
}

impl Folder for MemFolder {
    fn open(&self, name: &CStr, _: Mode) -> Result<u32, Error> {
        let name = name.to_str().map_err(|_| Error::BadArgument)?;
        let file = self
            .kept
            .iter()
            .position(|(kept, _)| kept == name)
            .ok_or(Error::Other(2))?;
        let mut handles = self.handles.borrow_mut();
        handles.push(Some(Handle {
            file,
            at: 0,
            seeks: 0,
        }));
        Ok(u32::try_from(handles.len() - 1).expect("few handles"))
    }

    fn read(&self, file: u32, into: &mut [u8]) -> Result<u32, Error> {
        let mut handles = self.handles.borrow_mut();
        let handle = handles[file as usize].as_mut().ok_or(Error::BadHandle)?;
        let bytes = &self.kept[handle.file].1;
        let moved = into.len().min(bytes.len().saturating_sub(handle.at));
        into[..moved].copy_from_slice(&bytes[handle.at..handle.at + moved]);
        handle.at += moved;
        Ok(u32::try_from(moved).expect("a small read"))
    }

    fn seek(&self, file: u32, distance: i32, origin: Origin) -> Result<u32, Error> {
        let mut handles = self.handles.borrow_mut();
        let handle = handles[file as usize].as_mut().ok_or(Error::BadHandle)?;
        let base = match origin {
            Origin::Start => 0,
            Origin::Here => handle.at,
            Origin::End => self.kept[handle.file].1.len(),
        };
        let to = base
            .checked_add_signed(distance as isize)
            .ok_or(Error::BadSeek)?;
        handle.seeks += 1;
        handle.at = match self.lie {
            _ if handle.seeks <= TRUE_SEEKS => to,
            SeekLie::Truth => to,
            SeekLie::Rewinds => 0,
            SeekLie::Stays => handle.at,
        };
        Ok(u32::try_from(to).expect("a small file"))
    }

    fn close(&self, file: u32) -> Result<(), Error> {
        self.handles.borrow_mut()[file as usize]
            .take()
            .ok_or(Error::BadHandle)?;
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

/// 1 s of 440 Hz and 880 Hz, 75 frames, made by `ffmpeg` for this crate's tests.
fn short_ogg() -> Vec<u8> {
    from_base64(include_str!("../src/test_ogg.b64"))
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

fn seal(page: &mut [u8]) {
    page[22..26].fill(0);
    let sum = checksum(page);
    page[22..26].copy_from_slice(&sum.to_le_bytes());
}

/// The file with its last page's granule set to 1 and sealed again, so no seek's landing can
/// be trusted and a read decodes from the start.
fn with_last_granule_one(bytes: &[u8]) -> Vec<u8> {
    let mut bytes = bytes.to_vec();
    let mut at = 0;
    let mut last = 0..0;
    while bytes.get(at..at + 4).is_some_and(|tag| tag == b"OggS") {
        let segments = usize::from(bytes[at + 26]);
        let body: usize = bytes[at + 27..at + 27 + segments]
            .iter()
            .map(|&size| usize::from(size))
            .sum();
        last = at..at + 27 + segments + body;
        at = last.end;
    }
    let page = &mut bytes[last];
    page[6..14].copy_from_slice(&1u64.to_le_bytes());
    seal(page);
    bytes
}

/// A straight decode, no seek in it: the answer an Ogg stream is judged by.
fn plain_decode(bytes: &[u8], frames: u32) -> Vec<i16> {
    let mut reader = OggStreamReader::new(Cursor::new(bytes)).expect("a true file opens");
    let mut samples = Vec::new();
    while let Some(packet) = reader.read_dec_packet_itl().expect("a true file decodes") {
        samples.extend_from_slice(&packet);
    }
    samples.truncate(frames as usize * FRAME_SAMPLES);
    samples
}

fn store_tracks() -> Vec<Track> {
    vec![Track {
        number: OGG_TRACK,
        audio: Audio::Encoded(Encoded {
            name: OGG_NAME.as_bytes().to_vec(),
            format: Format::Ogg,
        }),
    }]
}

fn at(track: u8, frame: u32) -> Place {
    Place { track, frame }
}

/// One thing the script asked to play, by the true table.
#[derive(Clone, Copy, Debug)]
struct Run {
    since_ms: u32,
    from: u64,
    end: u64,
}

impl Run {
    /// Where the clock says this run stands, in whole frames as the player counts them.
    fn clock_place(&self, now: u32) -> u64 {
        self.from + u64::from(now.wrapping_sub(self.since_ms)) * 75 / 1_000 * PAIRS
    }
}

/// What the script did and how the calls went.
#[derive(Default)]
struct Stats {
    runs: Vec<Run>,
    pumps: u64,
    pump_max_us: u64,
    over_lead: u64,
    late: u64,
    paused_at: Option<Place>,
}

/// A script's hands on the player, the mix and the clock.
struct Stage<'s> {
    time: &'s dyn Time,
    table: Tracks<'s>,
    mix: &'s dyn Fn(),
    pump: &'s dyn Fn(),
    send: &'s dyn Fn(u32, u32, &mut Words) -> u32,
    pumping: Cell<bool>,
    last_tick: Cell<Instant>,
    natural_end: Cell<u64>,
    stats: RefCell<Stats>,
}

impl Stage<'_> {
    fn now(&self) -> u32 {
        now_ms(self.time)
    }

    fn timed<R>(&self, call: impl FnOnce() -> R) -> R {
        let from = Instant::now();
        let answer = call();
        let waited = from.elapsed().as_micros() as u64;
        let mut stats = self.stats.borrow_mut();
        stats.pump_max_us = stats.pump_max_us.max(waited);
        if waited >= LEAD_MS * 1_000 {
            stats.over_lead += 1;
        }
        answer
    }

    fn pump_once(&self) {
        self.stats.borrow_mut().pumps += 1;
        self.timed(self.pump);
    }

    /// One step: the clock, the mix, and the player unless the arm has stopped it.
    fn tick(&self) {
        self.time.tick();
        let now = Instant::now();
        if now.duration_since(self.last_tick.get()).as_millis() > u128::from(LEAD_MS) {
            self.stats.borrow_mut().late += 1;
        }
        self.last_tick.set(now);
        (self.mix)();
        if self.pumping.get() {
            self.pump_once();
        }
    }

    fn wait(&self, ms: u32) {
        let from = self.now();
        while self.now().wrapping_sub(from) < ms {
            self.tick();
        }
    }

    fn order(&self, message: u32, flags: u32, words: &mut Words) {
        let answer = self.timed(|| (self.send)(message, flags, words));
        assert_eq!(answer, 0, "the player refused {message:#x}");
    }

    fn status(&self, item: u32) -> u32 {
        let mut words = Words([0; 5]);
        words.0[WORD_ITEM] = item;
        self.order(STATUS, STATUS_ITEM, &mut words);
        words.0[WORD_RETURN]
    }

    fn count(&self, place: Place) -> u64 {
        u64::from(self.table.end_of(place).expect("a place on the disc")) * PAIRS
    }

    /// The run in hand ends where the clock has it, or where it was going, the sooner.
    fn close_run(&self) {
        let now = self.now();
        if let Some(run) = self.stats.borrow_mut().runs.last_mut() {
            run.end = run.end.min(run.clock_place(now));
        }
    }

    fn open_run(&self, from: u64, end: u64) {
        let since_ms = self.now();
        self.stats.borrow_mut().runs.push(Run {
            since_ms,
            from,
            end,
        });
    }

    /// Plays from `from` to `to`, or to the disc's end.
    fn play(&self, from: Place, to: Option<Place>) {
        self.close_run();
        let end = to.map_or_else(|| u64::from(self.table.end()) * PAIRS, |to| self.count(to));
        self.open_run(self.count(from), end);
        let mut words = Words([0; 5]);
        words.0[WORD_FROM] = pack(from.track, from.frame);
        let mut flags = FROM;
        if let Some(to) = to {
            words.0[WORD_TO] = pack(to.track, to.frame);
            flags |= TO;
        }
        self.order(PLAY, flags, &mut words);
    }

    /// Pauses, and reads where the player says it stands.
    fn pause(&self) {
        self.order(PAUSE, 0, &mut Words([0; 5]));
        let (track, frame) = unpack(self.status(ITEM_POSITION));
        let place = at(track, frame);
        let count = self.count(place);
        let mut stats = self.stats.borrow_mut();
        stats.paused_at = Some(place);
        if let Some(run) = stats.runs.last_mut() {
            self.natural_end.set(run.end);
            run.end = run.end.min(count);
        }
    }

    fn resume(&self) {
        let place = self.stats.borrow().paused_at.expect("a pause came first");
        self.open_run(self.count(place), self.natural_end.get());
        self.order(RESUME, 0, &mut Words([0; 5]));
    }

    fn stop(&self) {
        self.close_run();
        self.order(STOP, 0, &mut Words([0; 5]));
    }

    fn until_stopped(&self) {
        loop {
            self.tick();
            if self.status(ITEM_MODE) == MODE_STOPPED {
                break;
            }
        }
        self.close_run();
    }
}

/// Across little to big-endian, a second place, and a pause in it: about 5.4 s.
fn clean_script(stage: &Stage<'_>) {
    stage.play(at(2, 30), Some(at(3, 60)));
    stage.until_stopped();
    stage.play(at(4, 15), Some(at(4, 225)));
    stage.wait(600);
    stage.pause();
    stage.wait(200);
    stage.resume();
    stage.until_stopped();
}

/// The player is never asked for more, past the play's own first fill.
fn no_pump_script(stage: &Stage<'_>) {
    stage.pumping.set(false);
    stage.play(at(4, 0), Some(at(4, 225)));
    stage.wait(1_500);
    stage.stop();
}

/// The player is asked only every `every_ms`, by a status and a pump, as a program's calls are.
fn pace_script(stage: &Stage<'_>, every_ms: u32) {
    stage.pumping.set(false);
    stage.play(at(4, 0), Some(at(5, 75)));
    let from = stage.now();
    let mut last = from;
    while stage.now().wrapping_sub(from) < 4_000 {
        stage.tick();
        if stage.now().wrapping_sub(last) >= every_ms {
            last = stage.now();
            stage.status(ITEM_MODE);
            stage.pump_once();
        }
    }
    stage.stop();
}

/// Far into a long file, twice back and forth, a pause and a resume on the first.
fn ogg_script(stage: &Stage<'_>) {
    stage.play(at(OGG_TRACK, 15_000), None);
    stage.wait(300);
    stage.pause();
    stage.wait(100);
    stage.resume();
    stage.wait(300);
    stage.play(at(OGG_TRACK, 7_500), None);
    stage.wait(600);
    stage.play(at(OGG_TRACK, 22_000), None);
    stage.wait(600);
    stage.stop();
}

fn ogg_lie_script(stage: &Stage<'_>) {
    stage.play(at(OGG_TRACK, 15_000), None);
    stage.wait(600);
    stage.stop();
}

fn ogg_short_script(stage: &Stage<'_>) {
    stage.play(at(OGG_TRACK, SEEK_FRAME), None);
    stage.until_stopped();
}

/// One run of pairs the answer holds, or zeros the mix padded a chunk with.
struct Piece {
    from: Option<u64>,
    pairs: u64,
}

/// What the walk found.
struct Walked {
    worked: u64,
    pieces: Vec<Piece>,
    first_wrong: Option<u64>,
    wrong: u64,
    wrong_place: Option<Place>,
    holes: u64,
    skipped: u64,
    largest_gap: u64,
    zero_pairs: u64,
    started: Vec<u64>,
    end_short: u64,
}

impl Walked {
    fn new() -> Self {
        Self {
            worked: HASH_START,
            pieces: Vec::new(),
            first_wrong: None,
            wrong: 0,
            wrong_place: None,
            holes: 0,
            skipped: 0,
            largest_gap: 0,
            zero_pairs: 0,
            started: Vec::new(),
            end_short: 0,
        }
    }

    /// A run stopped sounding at `heard`; past the tolerance short of its end, that is a gap.
    fn ended(&mut self, end: u64, heard: u64) {
        let short = end.saturating_sub(heard);
        self.end_short = self.end_short.max(short);
        if short > TOLERANCE {
            self.largest_gap = self.largest_gap.max(short);
        }
    }

    fn zero(&mut self) {
        self.zero_pairs += 1;
        self.worked = hash_samples(self.worked, &[0, 0]);
        match self.pieces.last_mut() {
            Some(piece) if piece.from.is_none() => piece.pairs += 1,
            _ => self.pieces.push(Piece {
                from: None,
                pairs: 1,
            }),
        }
    }

    fn answer(&mut self, truth: &Truth, count: u64) {
        let (left, right) = truth.pair(count).unwrap_or((0, 0));
        self.worked = hash_samples(self.worked, &[left, right]);
        match self.pieces.last_mut() {
            Some(piece) if piece.from.is_some_and(|from| from + piece.pairs == count) => {
                piece.pairs += 1;
            }
            _ => self.pieces.push(Piece {
                from: Some(count),
                pairs: 1,
            }),
        }
    }
}

/// The walk's two cursors: where the stream stands, and where the answer stands. A wrong sound
/// moves the first alone, so the worked hash still reads what was asked for.
struct Walker<'w> {
    truth: &'w Truth,
    runs: &'w [Run],
    run: usize,
    heard: u64,
    expected: u64,
    search_from: usize,
    out: Walked,
}

impl Walker<'_> {
    /// A later run the clock has reached that starts exactly at `place`.
    fn run_starting_at(&self, place: u64, ms: u32) -> Option<usize> {
        self.runs[self.run + 1..]
            .iter()
            .position(|run| run.from == place && ms >= run.since_ms)
            .map(|index| index + self.run + 1)
    }

    fn switch(&mut self, run: usize) {
        self.out.ended(self.runs[self.run].end, self.heard);
        self.run = run;
        self.heard = self.runs[run].from;
        self.expected = self.heard;
        self.out.started.push(self.heard);
    }

    /// Forward in this run, on a frame, and where the clock says the door would land.
    fn is_hole(&self, place: u64, ms: u32) -> bool {
        let run = &self.runs[self.run];
        place > self.heard
            && place < run.end + TOLERANCE
            && place % PAIRS == 0
            && run.clock_place(ms).abs_diff(place) <= TOLERANCE
    }

    fn take(&mut self) {
        self.out.answer(self.truth, self.expected);
        self.heard += 1;
        self.expected += 1;
    }

    fn wrong(&mut self, index: usize, found: Option<u64>, ms: u32, next_block: usize) {
        self.out.wrong += 1;
        self.out.first_wrong.get_or_insert(index as u64);
        let place = self.truth.place(found.unwrap_or(self.heard));
        self.out.wrong_place.get_or_insert(place);
        match found {
            Some(found) => self.heard = found,
            None => self.search_from = next_block,
        }
        let latest = self.runs.iter().rposition(|run| ms >= run.since_ms);
        if let Some(latest) = latest.filter(|&latest| latest > self.run) {
            self.run = latest;
            self.expected = self.runs[latest].from;
        }
        self.take();
    }

    fn step(&mut self, index: usize, stream: &[(i16, i16)], ms: u32, next_block: usize) {
        if let Some(run) = self.run_starting_at(self.heard, ms) {
            self.switch(run);
        }
        let pair = stream[index];
        if self.truth.pair(self.heard) == Some(pair) {
            self.take();
            return;
        }
        if pair == (0, 0) {
            self.out.zero();
            return;
        }
        let found = if index < self.search_from {
            None
        } else {
            locate(self.truth, &stream[index..])
        };
        if let Some(place) = found {
            if let Some(run) = self.run_starting_at(place, ms) {
                self.switch(run);
                self.take();
                return;
            }
            if self.is_hole(place, ms) {
                let pairs = place - self.heard;
                self.out.holes += 1;
                self.out.skipped += pairs;
                self.out.largest_gap = self.out.largest_gap.max(pairs);
                self.heard = place;
                self.expected = place;
                self.take();
                return;
            }
        }
        self.wrong(index, found, ms, next_block);
    }
}

fn walk(truth: &Truth, stream: &Stream, runs: &[Run]) -> Walked {
    let Some(first) = runs.first() else {
        return Walked::new();
    };
    let pairs: Vec<(i16, i16)> = stream
        .samples
        .chunks_exact(2)
        .map(|pair| (pair[0], pair[1]))
        .collect();
    let mut walker = Walker {
        truth,
        runs,
        run: 0,
        heard: first.from,
        expected: first.from,
        search_from: 0,
        out: Walked::new(),
    };
    if !pairs.is_empty() {
        walker.out.started.push(first.from);
    }
    let mut block = 0;
    for index in 0..pairs.len() {
        while stream
            .blocks
            .get(block + 1)
            .is_some_and(|next| next.at <= index)
        {
            block += 1;
        }
        let next_block = stream
            .blocks
            .get(block + 1)
            .map_or(pairs.len(), |next| next.at);
        walker.step(index, &pairs, stream.blocks[block].ms, next_block);
    }
    if let Some(last) = runs.last() {
        walker.out.ended(last.end, walker.heard);
    }
    walker.out
}

/// The pieces as `track:frame:pairs` joined by `;`, split at each track's end; zeros are track 0.
fn runs_field(truth: &Truth, pieces: &[Piece]) -> String {
    let mut words = Vec::new();
    for piece in pieces {
        let Some(mut from) = piece.from else {
            words.push(format!("0:0:{}", piece.pairs));
            continue;
        };
        let mut left = piece.pairs;
        while left > 0 {
            if from >= truth.total() {
                words.push(format!("0:0:{left}"));
                break;
            }
            let place = truth.place(from);
            let track_end = u64::from(Tracks(truth.table()).track_end((from / PAIRS) as u32));
            let pairs = left.min(track_end * PAIRS - from);
            words.push(format!("{}:{}:{pairs}", place.track, place.frame));
            from += pairs;
            left -= pairs;
        }
    }
    words.join(";")
}

fn places_field(truth: &Truth, counts: &[u64]) -> String {
    let words: Vec<String> = counts
        .iter()
        .map(|&count| {
            let place = truth.place(count);
            format!("{}:{}", place.track, place.frame)
        })
        .collect();
    words.join(";")
}

/// What the logger counted, where a run has one.
#[derive(Clone, Copy, Default)]
pub struct LineCounts {
    pub dropped: u64,
    /// Lines by `cd-audio` other than `imp.call`.
    pub skip_lines: u64,
    pub unread: u64,
}

/// What one arm came to.
pub struct Outcome {
    pub took: u32,
    pub witness: u64,
    pub hash: u64,
    pub worked: u64,
    pub runs: String,
    pub first_wrong: Option<u64>,
    pub wrong: u64,
    pub kept: u64,
    pub pumps: u64,
    pub gap_ms: u64,
    pub holes: u64,
    pub skipped: u64,
    pub lines: LineCounts,
    pub zero_pairs: u64,
    pub pump_max_ms: u64,
    pub pump_over_lead: u64,
    pub late: u64,
    pub started: String,
    pub paused_at: String,
    pub end_short_frames: u64,
    pub reds: Vec<&'static str>,
    pub sentence: Option<String>,
}

impl Outcome {
    /// The one line a bench reads.
    #[must_use]
    pub fn line(&self, route: &str, arm: &str) -> String {
        format!(
            "route={route} arm={arm} took={took} witness={witness} hash={hash:016x} \
             worked={worked:016x} runs={runs} first_wrong={first_wrong} wrong={wrong} kept={kept} \
             dropped={dropped} pumps={pumps} gap_ms={gap_ms} holes={holes} skipped={skipped} \
             skip_lines={skip_lines} zero_pairs={zero_pairs} pump_max_ms={pump_max_ms} \
             pump_over_lead={over} late={late} ended=true started={started} paused_at={paused_at} \
             end_short_frames={end_short} unread={unread} reds={reds} sentence=\"{said}\"",
            took = self.took,
            witness = self.witness,
            hash = self.hash,
            worked = self.worked,
            runs = if self.runs.is_empty() {
                "none"
            } else {
                &self.runs
            },
            first_wrong = self
                .first_wrong
                .map_or_else(|| "none".to_owned(), |index| index.to_string()),
            wrong = self.wrong,
            kept = self.kept,
            dropped = self.lines.dropped,
            pumps = self.pumps,
            gap_ms = self.gap_ms,
            holes = self.holes,
            skipped = self.skipped,
            skip_lines = self.lines.skip_lines,
            zero_pairs = self.zero_pairs,
            pump_max_ms = self.pump_max_ms,
            over = self.pump_over_lead,
            late = self.late,
            started = self.started,
            paused_at = self.paused_at,
            end_short = self.end_short_frames,
            unread = self.lines.unread,
            reds = if self.reds.is_empty() {
                "none".to_owned()
            } else {
                self.reds.join(",")
            },
            said = self.sentence.as_deref().unwrap_or("none"),
        )
    }
}

/// S4 on the native route, S5 on Wine's: the back refused what the check's own call took.
#[cfg(not(windows))]
const REFUSED: &str =
    "The project's sound output could not open a device that the computer check opened.";
#[cfg(windows)]
const REFUSED: &str = "The project's `waveOut` output failed where the bench's own call worked.";

/// The red words, each a rule the line breaks, and the sentence a person reads for the first.
/// A stalled decode is why the sound stopped, so it takes the gap's place.
fn judged(
    took: u32,
    gated: bool,
    hash: u64,
    walked: &Walked,
    stats: &Stats,
    lines: LineCounts,
) -> (Vec<&'static str>, Option<String>) {
    if took != MIX_HZ {
        return (vec!["open"], Some(REFUSED.to_owned()));
    }
    if !gated {
        return (vec!["gate"], None);
    }
    let mut reds = Vec::new();
    let mut sentence = None;
    if lines.dropped > 0 {
        reds.push("counts");
        let blocks = lines.dropped.div_ceil(BLOCK_FRAMES);
        sentence = Some(format!(
            "{blocks} blocks of sound went missing between the mixer and the output. \
             Your computer is cleared."
        ));
    }
    if stats.over_lead > 0 {
        reds.push("stall");
        let ms = stats.pump_max_us / 1_000;
        sentence = sentence.or_else(|| {
            Some(format!(
                "An Ogg decode held the sound up for {ms} ms, past its {LEAD_MS} ms lead."
            ))
        });
    }
    let gapped = walked.largest_gap > 0 && stats.over_lead == 0;
    if gapped {
        reds.push("gap");
    }
    if walked.first_wrong.is_some() {
        reds.push("mix");
        if let Some(place) = walked.wrong_place {
            let (track, frame) = (place.track, place.frame);
            sentence = sentence.or_else(|| {
                Some(format!(
                    "CD sound came from the wrong place: track {track} at {frame}. \
                     The fault is in `cd-audio`."
                ))
            });
        }
    }
    if gapped {
        let ms = walked.largest_gap * 10 / 441;
        sentence = sentence.or_else(|| {
            Some(if stats.pumps == 0 {
                format!("CD sound stopped since nothing asked for more. The gap was {ms} ms.")
            } else {
                format!("CD sound has a gap of {ms} ms, since calls came slower than its lead.")
            })
        });
    }
    if hash != walked.worked {
        reds.push("hash");
    }
    (reds, sentence)
}

/// What an arm hands the staging: the answer, the table the player is told, and its reader.
struct Setup<'s> {
    truth: &'s Truth,
    table: &'s [Span],
    samples: Box<dyn Samples + 's>,
}

/// The player over the mix, and the script run on them.
fn perform<H: Host, O: Output>(
    mix: &Mix<H, O>,
    setup: Setup<'_>,
    time: &Rc<dyn Time>,
    script: impl FnOnce(&Stage<'_>),
) -> Stats {
    let door_time = Rc::clone(time);
    let front_time = Rc::clone(time);
    let player = DoorPlayer::new(
        Quit,
        mix,
        Picked(setup.samples),
        Tracks(setup.table),
        move || now_ms(&*door_time),
    );
    let front = PlayFront::new(
        &ROW,
        Quit,
        Tracks(setup.table),
        move || now_ms(&*front_time),
        &player,
    );
    let mix_pump = || mix.pump();
    let player_pump = || player.pump();
    let send = |message: u32, flags: u32, words: &mut Words| front.send(ID, message, flags, words);
    let stage = Stage {
        time: &**time,
        table: Tracks(setup.truth.table()),
        mix: &mix_pump,
        pump: &player_pump,
        send: &send,
        pumping: Cell::new(true),
        last_tick: Cell::new(Instant::now()),
        natural_end: Cell::new(0),
        stats: RefCell::new(Stats::default()),
    };
    script(&stage);
    stage.stats.into_inner()
}

/// The mix over `back`, its output open and silent until `gate` says go; the script; the back
/// played out; the walk and the judgement.
fn staged<O: Output>(
    setup: Setup<'_>,
    back: O,
    time: &Rc<dyn Time>,
    gate: impl FnOnce() -> bool,
    lines: &dyn Fn() -> LineCounts,
    script: impl FnOnce(&Stage<'_>),
) -> Outcome {
    let truth = setup.truth;
    let back = Rc::new(RefCell::new(back));
    let stream = Rc::new(RefCell::new(Stream::default()));
    let witness = Witness::new(Tap {
        back: Rc::clone(&back),
        stream: Rc::clone(&stream),
        time: Rc::clone(time),
    });
    let seen = witness.seen();
    let mix = Mix::new(&MIX_ROW, Hosted(Rc::clone(time)), witness);
    // It holds nothing, so it adds nothing: it opens the output before anything can sound.
    let holder = mix.stream_open(MIX_HZ, 2);
    let took = stream.borrow().took;
    let room_at_open = back.borrow().room();
    let gated = took == MIX_HZ && gate();
    let stats = if gated {
        perform(&mix, setup, time, script)
    } else {
        Stats::default()
    };
    let tail = Instant::now();
    while gated && back.borrow().room() < room_at_open && tail.elapsed() < TAIL {
        time.tick();
        mix.pump();
    }
    mix.stream_close(holder);
    seen.log_summary();

    let summary = seen.summary();
    let stream = stream.borrow();
    let walked = walk(truth, &stream, &stats.runs);
    let lines = lines();
    let (reds, sentence) = judged(took, gated, summary.hash, &walked, &stats, lines);
    Outcome {
        took,
        witness: summary.frames,
        hash: summary.hash,
        worked: walked.worked,
        runs: runs_field(truth, &walked.pieces),
        first_wrong: walked.first_wrong,
        wrong: walked.wrong,
        kept: stream.kept,
        pumps: stats.pumps,
        gap_ms: walked.largest_gap * 10 / 441,
        holes: walked.holes,
        skipped: walked.skipped,
        lines,
        zero_pairs: walked.zero_pairs,
        pump_max_ms: stats.pump_max_us / 1_000,
        pump_over_lead: stats.over_lead,
        late: stats.late,
        started: places_field(truth, &walked.started),
        paused_at: stats.paused_at.map_or_else(
            || "none".to_owned(),
            |place| format!("{}:{}", place.track, place.frame),
        ),
        end_short_frames: walked.end_short / PAIRS,
        reds,
        sentence,
    }
}

/// One run's wrong input, or none.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Arm {
    Clean,
    WrongTrack,
    WrongPlace,
    ShortTable,
    NoPump,
    Pace(u32),
    OggClean(String),
    OggLie(String),
    OggShort,
    OggShortLie,
}

/// The arm a run's word names; the `seek-*` arms are [`seek_of`]'s.
#[must_use]
pub fn arm_of(word: &str) -> Option<Arm> {
    Some(match word {
        "clean" => Arm::Clean,
        "wrong-track" => Arm::WrongTrack,
        "wrong-place" => Arm::WrongPlace,
        "short-table" => Arm::ShortTable,
        "no-pump" => Arm::NoPump,
        "ogg-short" => Arm::OggShort,
        "ogg-short-lie" => Arm::OggShortLie,
        _ => {
            if let Some(ms) = word.strip_prefix("pace:") {
                Arm::Pace(ms.parse().ok()?)
            } else if let Some(path) = word.strip_prefix("ogg-clean:") {
                Arm::OggClean(path.to_owned())
            } else {
                Arm::OggLie(word.strip_prefix("ogg-lie:")?.to_owned())
            }
        }
    })
}

/// One arm over the made disc, over `back`, by `time`.
fn image_arm<O: Output>(
    arm: &Arm,
    back: O,
    time: &Rc<dyn Time>,
    gate: impl FnOnce() -> bool,
    lines: &dyn Fn() -> LineCounts,
) -> Outcome {
    let disc = made_disc();
    let mut table = spans(&Quit, disc.tracks(), |_| None);
    if *arm == Arm::ShortTable {
        table[0].frames = SHORT_TRACK_FRAMES;
    }
    let image = ImageSamples::new(&disc);
    let samples: Box<dyn Samples + '_> = match arm {
        Arm::WrongTrack => Box::new(ReadsTrack {
            inner: image,
            asked: 2,
            read: 3,
        }),
        Arm::WrongPlace => Box::new(ReadsOn {
            inner: image,
            frames: PLACE_PLANT_FRAMES,
        }),
        _ => Box::new(image),
    };
    let setup = Setup {
        truth: &Truth::Recipe,
        table: &table,
        samples,
    };
    match arm {
        Arm::NoPump => staged(setup, back, time, gate, lines, no_pump_script),
        Arm::Pace(every_ms) => staged(setup, back, time, gate, lines, |stage| {
            pace_script(stage, *every_ms);
        }),
        _ => staged(setup, back, time, gate, lines, clean_script),
    }
}

/// One arm over an Ogg file, played as `played` and judged against `true_bytes`, whose length
/// the table keeps: a lying file's own length would have the play refused.
fn ogg_arm<O: Output>(
    true_bytes: &[u8],
    played: Vec<u8>,
    back: O,
    time: &Rc<dyn Time>,
    gate: impl FnOnce() -> bool,
    lines: &dyn Fn() -> LineCounts,
    script: impl FnOnce(&Stage<'_>),
) -> Outcome {
    let tracks = store_tracks();
    let true_folder = MemFolder::one(OGG_NAME, true_bytes.to_vec(), SeekLie::Truth);
    let measured = OggSamples::new(&true_folder, &tracks);
    let table = spans(&Quit, &tracks, |encoded| measured.measure(encoded));
    let truth = Truth::Decoded {
        table: [table[0]],
        samples: plain_decode(true_bytes, table[0].frames),
    };
    let folder = MemFolder::one(OGG_NAME, played, SeekLie::Truth);
    let setup = Setup {
        truth: &truth,
        table: &table,
        samples: Box::new(OggSamples::new(&folder, &tracks)),
    };
    staged(setup, back, time, gate, lines, script)
}

/// One arm over `back`, by `time`. A real back's output stays silent until `gate` says go.
pub fn run_arm<O: Output>(
    arm: &Arm,
    back: O,
    time: &Rc<dyn Time>,
    gate: impl FnOnce() -> bool,
    lines: &dyn Fn() -> LineCounts,
) -> Outcome {
    let read = |path: &str| std::fs::read(path).expect("the long Ogg reads");
    match arm {
        Arm::OggClean(path) => {
            let bytes = read(path);
            ogg_arm(&bytes, bytes.clone(), back, time, gate, lines, ogg_script)
        }
        Arm::OggLie(path) => {
            let bytes = read(path);
            let lying = with_last_granule_one(&bytes);
            ogg_arm(&bytes, lying, back, time, gate, lines, ogg_lie_script)
        }
        Arm::OggShort => {
            let bytes = short_ogg();
            ogg_arm(
                &bytes,
                bytes.clone(),
                back,
                time,
                gate,
                lines,
                ogg_short_script,
            )
        }
        Arm::OggShortLie => {
            let bytes = short_ogg();
            let lying = with_last_granule_one(&bytes);
            ogg_arm(&bytes, lying, back, time, gate, lines, ogg_short_script)
        }
        _ => image_arm(arm, back, time, gate, lines),
    }
}

/// The lie a `seek-*` word names.
#[must_use]
pub fn seek_of(word: &str) -> Option<SeekLie> {
    match word {
        "seek-truth" => Some(SeekLie::Truth),
        "seek-lie" => Some(SeekLie::Rewinds),
        "seek-stays" => Some(SeekLie::Stays),
        _ => None,
    }
}

/// One read on a folder that may lie, with no back and no stream.
#[derive(Clone, Copy, Debug)]
pub struct SeekRead {
    pub lie: SeekLie,
    pub ended: bool,
    pub ms: u64,
    pub moved: Option<usize>,
    /// What it read is a plain decode's.
    pub right: bool,
}

impl SeekRead {
    #[must_use]
    pub fn red(&self) -> Option<&'static str> {
        if !self.ended {
            Some("hung")
        } else if self.lie == SeekLie::Truth && !self.right {
            Some("wrong")
        } else {
            None
        }
    }

    #[must_use]
    pub fn sentence(&self) -> Option<&'static str> {
        (!self.ended).then_some(
            "A file's seek gave a wrong answer and the CD player hung. Expected red today, by reading.",
        )
    }

    #[must_use]
    pub fn line(&self, route: &str, arm: &str) -> String {
        format!(
            "route={route} arm={arm} ended={ended} seek_ms={ms} moved={moved} right={right} \
             reds={red} sentence=\"{said}\"",
            ended = self.ended,
            ms = self.ms,
            moved = self
                .moved
                .map_or_else(|| "none".to_owned(), |moved| moved.to_string()),
            right = self.right,
            red = self.red().unwrap_or("none"),
            said = self.sentence().unwrap_or("none"),
        )
    }
}

/// Reads frame 30 of the short file in a worker, as a read that hangs cannot be killed. A
/// hung worker runs on until the caller's process ends, so the caller owes that end.
#[must_use]
pub fn seek_arm(lie: SeekLie) -> SeekRead {
    let (sent, read) = mpsc::channel();
    let from = Instant::now();
    thread::spawn(move || {
        let bytes = short_ogg();
        let plain = plain_decode(&bytes, 75);
        let folder = MemFolder::one(OGG_NAME, bytes, lie);
        let tracks = store_tracks();
        let mut samples = OggSamples::new(&folder, &tracks);
        let mut into = vec![0; LEAD_FRAMES as usize * FRAME_SAMPLES];
        let place = at(OGG_TRACK, SEEK_FRAME);
        let moved = samples.read(place, &mut into);
        let start = SEEK_FRAME as usize * FRAME_SAMPLES;
        let right = moved.is_some_and(|moved| {
            moved > 0 && plain.get(start..start + moved) == Some(&into[..moved])
        });
        let _ = sent.send((moved, right));
    });
    let answer = read.recv_timeout(SEEK_DEADLINE);
    let ms = from.elapsed().as_millis() as u64;
    match answer {
        Ok((moved, right)) => SeekRead {
            lie,
            ended: true,
            ms,
            moved,
            right,
        },
        Err(_) => SeekRead {
            lie,
            ended: false,
            ms,
            moved: None,
            right: false,
        },
    }
}
