// SPDX-License-Identifier: MIT
//! A disc's sound reaching a door: a test's own cue and bin, by name a stand-in, a door
//! that keeps what it is told, and `sound_mix::Front` itself. Nothing here reaches a
//! speaker or runs under Windows.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use kernel_files::Error;
use logical_disc::{Disc, ReadAt, Track};
use sound_mix::output::Output;
use sound_mix::table::Row as MixRow;
use sound_mix::{Host, NullBack};

use crate::disc::spans;
use crate::door::{Door, DoorPlayer, FRAME_SAMPLES, ImageSamples, LEAD_FRAMES, Samples};
use crate::play::PlayFront;
use crate::table::{Place, Row, Span, Tracks};
use crate::test_disc::{FRAME_BYTES, THREE_FRAMES, THREE_SEED, TWO_FRAMES, TWO_SEED, fill, image};
use crate::test_log::debug_lines;
use crate::tests::{CALLS, Panics, Untouched, record};
use crate::tmsf::pack;

const ID: u32 = 7;
const PLAY: u32 = 0x806;
const STOP: u32 = 0x808;
const PAUSE: u32 = 0x809;
const STATUS: u32 = 0x814;
const RESUME: u32 = 0x855;
const CLOSE: u32 = 0x804;
const MODE_PLAYING: u32 = 526;

static ROW: Row = Row {
    file: "a program",
    calls: CALLS,
    letters: 1,
    tracks: 22,
    device_name: "cdaudio",
    device_id: ID,
};

static MIX: MixRow = MixRow {
    file: "a program",
    module: "MSS32",
};

/// A data track 1 and no track 4 around them: 1 s, 2 s and 4 s of audio.
const DISC: [Span; 3] = [
    Span {
        number: 2,
        frames: 75,
    },
    Span {
        number: 3,
        frames: 150,
    },
    Span {
        number: 5,
        frames: 300,
    },
];

/// A door that keeps what it is told, and refuses what a real one would stop on.
#[derive(Default)]
pub(crate) struct Doors {
    pub(crate) live: Cell<Option<u32>>,
    pub(crate) opened: Cell<u32>,
    shapes: RefCell<Vec<(u32, u32)>>,
    pub(crate) pushed: RefCell<Vec<i16>>,
}

impl Door for Doors {
    fn open(&self, rate_hz: u32, channels: u32) -> u32 {
        assert_eq!(self.live.get(), None, "a second door");
        self.opened.set(self.opened.get() + 1);
        self.live.set(Some(self.opened.get()));
        self.shapes.borrow_mut().push((rate_hz, channels));
        self.opened.get()
    }

    fn push(&self, id: u32, samples: &[i16]) {
        assert_eq!(self.live.get(), Some(id), "a push on a dead door");
        self.pushed.borrow_mut().extend_from_slice(samples);
    }

    fn close(&self, id: u32) {
        assert_eq!(self.live.get(), Some(id), "a close of a dead door");
        self.live.set(None);
    }
}

impl Doors {
    /// Each frame pushed, by the number [`Numbered`] gave it.
    fn frames(&self) -> Vec<i16> {
        let pushed = self.pushed.borrow();
        pushed.chunks(FRAME_SAMPLES).map(|frame| frame[0]).collect()
    }
}

/// Samples that say where they came from, `track * 1000 + frame`, short at a track's end.
struct Numbered {
    refused: Option<u8>,
}

fn numbered(track: u8, frame: u32) -> i16 {
    i16::try_from(u32::from(track) * 1000 + frame).expect("a small disc")
}

impl Samples for Numbered {
    fn read(&mut self, place: Place, into: &mut [i16]) -> Option<usize> {
        if self.refused == Some(place.track) {
            return None;
        }
        let span = DISC.iter().find(|span| span.number == place.track)?;
        let left = (span.frames - place.frame) as usize;
        let frames = left.min(into.len() / FRAME_SAMPLES);
        for (index, frame) in into.chunks_mut(FRAME_SAMPLES).take(frames).enumerate() {
            let at = place.frame + u32::try_from(index).expect("a small read");
            frame.fill(numbered(place.track, at));
        }
        Some(frames * FRAME_SAMPLES)
    }
}

fn played(track: u8, frames: core::ops::Range<u32>) -> Vec<i16> {
    frames.map(|frame| numbered(track, frame)).collect()
}

fn mode_of(front: &PlayFront<'_, Panics, impl Fn() -> u32, impl crate::play::Player>) -> u32 {
    let mut asked = record([0, 0, 4, 0, 0]);
    assert_eq!(front.send(ID, STATUS, 0x100, &mut asked), 0);
    asked.words[1]
}

#[derive(Clone, Copy, Debug)]
enum Step {
    PlayFrom,
    PlayTo,
    Pause,
    Resume,
    Stop,
    Close,
    Later,
    PastTheEnd,
    Pump,
}

const STEPS: [Step; 9] = [
    Step::PlayFrom,
    Step::PlayTo,
    Step::Pause,
    Step::Resume,
    Step::Stop,
    Step::Close,
    Step::Later,
    Step::PastTheEnd,
    Step::Pump,
];

/// Every walk of 4 steps: a door opens only when none lives, is fed and closed only while
/// it lives, and lives exactly while the mode reads playing.
#[test]
fn no_walk_opens_a_second_door_or_touches_a_dead_one() {
    let many = STEPS.len();
    for walk in 0..many.pow(4) {
        let ms = Cell::new(0u32);
        let doors = Doors::default();
        let player = DoorPlayer::new(
            Panics,
            &doors,
            Numbered { refused: None },
            Tracks(&DISC),
            || ms.get(),
        );
        let front = PlayFront::new(&ROW, Panics, Tracks(&DISC), || ms.get(), &player);
        let mut left = walk;
        for _ in 0..4 {
            let step = STEPS[left % many];
            left /= many;
            let answer = match step {
                Step::PlayFrom => front.send(ID, PLAY, 4, &mut record([0, pack(2, 0), 0, 0, 0])),
                Step::PlayTo => front.send(
                    ID,
                    PLAY,
                    0xc,
                    &mut record([0, pack(3, 0), pack(5, 0), 0, 0]),
                ),
                Step::Pause => front.send(ID, PAUSE, 0, &mut Untouched),
                Step::Resume => front.send(ID, RESUME, 0, &mut Untouched),
                Step::Stop => front.send(ID, STOP, 0, &mut Untouched),
                Step::Close => front.send(ID, CLOSE, 0, &mut Untouched),
                Step::Later => {
                    ms.set(ms.get() + 500);
                    0
                }
                Step::PastTheEnd => {
                    ms.set(ms.get() + 100_000);
                    0
                }
                Step::Pump => {
                    player.pump();
                    0
                }
            };
            assert_eq!(answer, 0, "walk {walk}, {step:?}");
            let reading = mode_of(&front);
            assert_eq!(
                reading == MODE_PLAYING,
                doors.live.get().is_some(),
                "walk {walk}, {step:?}: mode {reading}"
            );
        }
    }
}

/// A rig of one door player over [`DISC`], the clock a number the test moves.
struct Rig {
    ms: Rc<Cell<u32>>,
    doors: Doors,
}

impl Rig {
    fn new() -> Self {
        Self {
            ms: Rc::new(Cell::new(0)),
            doors: Doors::default(),
        }
    }

    fn player(
        &self,
        refused: Option<u8>,
    ) -> DoorPlayer<'static, Panics, &Doors, Numbered, impl Fn() -> u32 + use<>> {
        let ms = Rc::clone(&self.ms);
        DoorPlayer::new(
            Panics,
            &self.doors,
            Numbered { refused },
            Tracks(&DISC),
            move || ms.get(),
        )
    }

    fn clock(&self) -> impl Fn() -> u32 + use<> {
        let ms = Rc::clone(&self.ms);
        move || ms.get()
    }
}

#[test]
fn a_push_never_passes_the_clock_plus_the_lead_and_never_arrives_late() {
    let rig = Rig::new();
    let player = rig.player(None);
    let front = PlayFront::new(&ROW, Panics, Tracks(&DISC), rig.clock(), &player);
    assert_eq!(
        front.send(ID, PLAY, 4, &mut record([0, pack(5, 0), 0, 0, 0])),
        0
    );
    // 13.33 ms a frame; pumped often, the door holds the lead and no more.
    assert_eq!(rig.doors.frames(), played(5, 0..LEAD_FRAMES));
    let mut pumps = 0;
    while pumps < 30 {
        pumps += 1;
        rig.ms.set(rig.ms.get() + 40);
        player.pump();
        let position = rig.ms.get() * 75 / 1000;
        let reached = u32::try_from(rig.doors.frames().len()).expect("a small run");
        assert_eq!(reached, position + LEAD_FRAMES, "at {} ms", rig.ms.get());
    }
    assert_eq!(rig.doors.frames(), played(5, 0..108));
    // A long wait: what the clock passed goes unheard, and the ear rejoins the position.
    rig.ms.set(3000);
    player.pump();
    let frames = rig.doors.frames();
    assert_eq!(frames[108..], played(5, 225..225 + LEAD_FRAMES));
}

#[test]
fn a_run_crosses_tracks_in_order_and_stops_at_its_end() {
    let rig = Rig::new();
    let player = rig.player(None);
    let front = PlayFront::new(&ROW, Panics, Tracks(&DISC), rig.clock(), &player);
    assert_eq!(
        front.send(
            ID,
            PLAY,
            0xc,
            &mut record([0, pack(2, 70), pack(5, 5), 0, 0])
        ),
        0
    );
    let mut wanted = played(2, 70..75);
    wanted.extend(played(3, 0..13));
    assert_eq!(rig.doors.frames(), wanted);
    // Far enough for every frame to its end, in small steps; 160 frames, 2,134 ms.
    for ms in (0..=2200).step_by(100) {
        rig.ms.set(ms);
        player.pump();
    }
    let mut wanted = played(2, 70..75);
    wanted.extend(played(3, 0..150));
    wanted.extend(played(5, 0..5));
    assert_eq!(rig.doors.frames(), wanted);
    assert_eq!(rig.doors.live.get(), Some(1));
    assert_ne!(mode_of(&front), MODE_PLAYING);
    assert_eq!(rig.doors.live.get(), None, "the run's end closes its door");
}

#[test]
fn a_pause_closes_the_door_and_a_resume_opens_a_new_one_where_the_clock_held() {
    let rig = Rig::new();
    let player = rig.player(None);
    let front = PlayFront::new(&ROW, Panics, Tracks(&DISC), rig.clock(), &player);
    assert_eq!(
        front.send(ID, PLAY, 4, &mut record([0, pack(5, 0), 0, 0, 0])),
        0
    );
    rig.ms.set(1000);
    assert_eq!(front.send(ID, PAUSE, 0, &mut Untouched), 0);
    assert_eq!(rig.doors.live.get(), None, "nothing sounds in a pause");
    rig.doors.pushed.borrow_mut().clear();
    rig.ms.set(9000);
    player.pump();
    assert!(
        rig.doors.frames().is_empty(),
        "a pump in a pause moves nothing"
    );
    assert_eq!(front.send(ID, RESUME, 0, &mut Untouched), 0);
    assert_eq!(rig.doors.opened.get(), 2);
    assert_eq!(rig.doors.frames(), played(5, 75..75 + LEAD_FRAMES));
}

#[test]
fn a_volume_scales_each_side_for_the_next_push() {
    let rig = Rig::new();
    let player = rig.player(None);
    let front = PlayFront::new(&ROW, Panics, Tracks(&DISC), rig.clock(), &player);
    let sample = numbered(5, 0);
    let first = |left: u16, right: u16| -> Vec<i16> {
        // A new play, so the pushes start again from the same frame.
        rig.doors.pushed.borrow_mut().clear();
        let word = u32::from(right) << 16 | u32::from(left);
        assert_eq!(front.aux_set_volume(0, word), 0);
        assert_eq!(
            front.send(ID, PLAY, 4, &mut record([0, pack(5, 0), 0, 0, 0])),
            0
        );
        rig.doors.pushed.borrow()[..2].to_vec()
    };
    assert_eq!(first(0xffff, 0xffff), [sample, sample], "the same bytes");
    assert_eq!(first(0, 0), [0, 0], "silence");
    let half = i16::try_from(i32::from(sample) * 0x8000 / 0xffff).expect("smaller");
    assert_eq!(first(0x8000, 0x8000), [half, half], "half");
    assert_eq!(first(0xffff, 0), [sample, 0], "the low half is the left");
    assert_eq!(first(0, 0xffff), [0, sample], "the high half is the right");
    assert_eq!(first(0x8000, 0xffff), [half, sample]);
}

#[test]
fn a_track_that_will_not_read_is_one_line_and_silence_and_the_clock_runs_on() {
    let rig = Rig::new();
    let player = rig.player(Some(3));
    let front = PlayFront::new(&ROW, Panics, Tracks(&DISC), rig.clock(), &player);
    let position = || {
        let mut asked = record([0, 0, 2, 0, 0]);
        assert_eq!(front.send(ID, STATUS, 0x100, &mut asked), 0);
        asked.words[1]
    };
    let lines = debug_lines(|| {
        assert_eq!(
            front.send(ID, PLAY, 4, &mut record([0, pack(3, 0), 0, 0, 0])),
            0
        );
        for ms in (0..=1900).step_by(100) {
            rig.ms.set(ms);
            player.pump();
        }
    });
    let unread: Vec<_> = lines
        .iter()
        .filter(|line| line.starts_with("cd.track.unread"))
        .collect();
    assert_eq!(unread, ["cd.track.unread a=3 by=cd-audio"]);
    // Silent to the track's end; the lead, 142 + 18, reaches 10 frames into the next.
    assert_eq!(
        rig.doors.frames(),
        played(5, 0..10),
        "silence, then the next"
    );
    assert_eq!(mode_of(&front), MODE_PLAYING);
    assert_eq!(position(), pack(3, 142), "the clock ran on");
    rig.ms.set(1934);
    player.pump();
    assert_eq!(rig.doors.frames(), played(5, 0..13));
}

/// A file that fails its reads once told to.
struct Flaky {
    bytes: Vec<u8>,
    failing: Rc<Cell<bool>>,
}

impl ReadAt for Flaky {
    fn size(&self) -> u64 {
        self.bytes.len() as u64
    }

    fn read_at(&self, at: u64, into: &mut [u8]) -> Result<(), Error> {
        if self.failing.get() {
            return Err(Error::Other(5));
        }
        self.bytes.as_slice().read_at(at, into)
    }
}

/// `sound_mix`'s own clock, moved by the test.
#[derive(Default)]
struct Seat(AtomicU64);

impl Host for &Seat {
    fn now_us(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }

    fn call_back(&self, _: u32, _: u32) {
        panic!("no sample ends here")
    }

    fn read_file(&self, _: &str) -> Option<u32> {
        None
    }

    fn free_block(&self, _: u32) {}
}

/// A back that keeps what the mix wrote, as a speaker would hear it.
#[derive(Clone, Default)]
struct Speaker {
    rates: Arc<Mutex<Vec<u32>>>,
    heard: Arc<Mutex<Vec<i16>>>,
}

impl Output for Speaker {
    fn open(&mut self, rate_hz: u32) -> u32 {
        self.rates
            .lock()
            .expect("no test panics here")
            .push(rate_hz);
        rate_hz
    }

    fn room(&self) -> usize {
        1 << 20
    }

    fn write(&mut self, frames: &[i16]) {
        self.heard
            .lock()
            .expect("no test panics here")
            .extend_from_slice(frames);
    }

    fn close(&mut self) {}
}

fn little(bytes: &[u8]) -> Vec<i16> {
    bytes
        .chunks_exact(2)
        .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
        .collect()
}

fn big(bytes: &[u8]) -> Vec<i16> {
    bytes
        .chunks_exact(2)
        .map(|pair| i16::from_be_bytes([pair[0], pair[1]]))
        .collect()
}

fn no_store_tracks(_: &logical_disc::Encoded) -> Option<u32> {
    panic!("an image's tracks say how long they run")
}

#[test]
fn a_cue_and_bin_of_a_tests_own_round_trips_to_spans_and_bytes() {
    let disc = Disc::open(image(|bytes| bytes)).expect("the stand-in opens");
    let found = spans(&Panics, disc.tracks(), no_store_tracks);
    assert_eq!(
        found,
        [
            Span {
                number: 2,
                frames: TWO_FRAMES,
            },
            Span {
                number: 3,
                frames: THREE_FRAMES,
            },
        ]
    );
    let whole = |seed, frames: u32, skipped: usize| {
        fill((frames as usize + skipped) * FRAME_BYTES, seed)[skipped * FRAME_BYTES..].to_vec()
    };
    let two = whole(TWO_SEED, TWO_FRAMES, 2);
    let three = whole(THREE_SEED, THREE_FRAMES, 0);
    let mut samples = ImageSamples::new(&disc);
    for (number, frames, wanted) in [
        (2, TWO_FRAMES, little(&two)),
        (3, THREE_FRAMES, big(&three)),
    ] {
        let mut out = vec![0; frames as usize * FRAME_SAMPLES + 10];
        let place = Place {
            track: number,
            frame: 0,
        };
        let read = samples.read(place, &mut out);
        assert_eq!(
            read,
            Some(frames as usize * FRAME_SAMPLES),
            "short at its end"
        );
        assert_eq!(out[..wanted.len()], wanted, "track {number}");
        let mut last = vec![0; FRAME_SAMPLES];
        let place = Place {
            track: number,
            frame: frames - 1,
        };
        assert_eq!(samples.read(place, &mut last), Some(FRAME_SAMPLES));
        assert_eq!(
            last,
            wanted[wanted.len() - FRAME_SAMPLES..],
            "track {number}'s end"
        );
    }
    let mut out = vec![0; FRAME_SAMPLES];
    let off = Place { track: 1, frame: 0 };
    assert_eq!(
        samples.read(off, &mut out),
        None,
        "the data track holds no sound"
    );
}

#[test]
fn a_store_file_that_cannot_be_measured_runs_for_no_frames_with_one_line() {
    let tracks = [Track {
        number: 4,
        audio: logical_disc::Audio::Encoded(logical_disc::Encoded {
            name: b"a file".to_vec(),
            format: logical_disc::Format::Ogg,
        }),
    }];
    let lines = debug_lines(|| {
        let found = spans(&Panics, &tracks, |_| None);
        assert_eq!(
            found,
            [Span {
                number: 4,
                frames: 0
            }]
        );
    });
    assert_eq!(lines, ["cd.track.unread a=4 by=cd-audio"]);
    let measured = spans(&Panics, &tracks, |_| Some(99));
    assert_eq!(
        measured,
        [Span {
            number: 4,
            frames: 99
        }]
    );
}

#[test]
fn the_door_is_exactly_44100_and_2_on_sound_mix_and_the_bytes_arrive_unchanged() {
    let disc = Disc::open(image(|bytes| bytes)).expect("the stand-in opens");
    let table = spans(&Panics, disc.tracks(), no_store_tracks);
    let tracks = Tracks(&table);
    let seat = Seat::default();
    let speaker = Speaker::default();
    let door = sound_mix::Front::new(&MIX, &seat, speaker.clone());
    let ms = Cell::new(0u32);
    let player = DoorPlayer::new(Panics, &door, ImageSamples::new(&disc), tracks, || ms.get());
    let front = PlayFront::new(&ROW, Panics, tracks, || ms.get(), &player);

    let file = fill(40 * FRAME_BYTES, TWO_SEED);
    let two = little(&file[2 * FRAME_BYTES..]);
    let lines = debug_lines(|| {
        let word = pack(2, 0);
        assert_eq!(front.send(ID, PLAY, 4, &mut record([0, word, 0, 0, 0])), 0);
    });
    assert!(
        lines
            .iter()
            .any(|line| line.starts_with("sound.door.open a=ac44,2 ")),
        "{lines:?}"
    );
    let heard = LEAD_FRAMES as usize * FRAME_SAMPLES;
    // 18 frames of 588 pairs at 44,100 a second is 240 ms.
    seat.0.store(240_000, Ordering::Relaxed);
    door.pump();
    {
        let speaker = speaker.heard.lock().expect("no test panics here");
        assert_eq!(speaker.len(), heard);
        assert_eq!(speaker[..], two[..heard], "the bytes, unchanged");
    }

    // A big-endian track, turned: the first door closed, what it held gone with it.
    speaker.heard.lock().expect("no test panics here").clear();
    let three = big(&fill(THREE_FRAMES as usize * FRAME_BYTES, THREE_SEED));
    assert_eq!(
        front.send(ID, PLAY, 4, &mut record([0, pack(3, 0), 0, 0, 0])),
        0
    );
    seat.0.store(480_000, Ordering::Relaxed);
    door.pump();
    let speaker = speaker.heard.lock().expect("no test panics here");
    assert_eq!(speaker[..], three[..heard], "the bytes, turned");
    assert!(speaker.len() == heard);
}

#[test]
fn over_the_null_back_every_track_plays_to_the_end_with_no_fault() {
    let disc = Disc::open(image(|bytes| bytes)).expect("the stand-in opens");
    let table = spans(&Panics, disc.tracks(), no_store_tracks);
    let tracks = Tracks(&table);
    let seat = Seat::default();
    let door = sound_mix::Front::new(&MIX, &seat, NullBack);
    let ms = Cell::new(0u32);
    let player = DoorPlayer::new(Panics, &door, ImageSamples::new(&disc), tracks, || ms.get());
    let front = PlayFront::new(&ROW, Panics, tracks, || ms.get(), &player);
    assert_eq!(
        front.send(ID, PLAY, 4, &mut record([0, pack(2, 0), 0, 0, 0])),
        0
    );
    for step in 1..50u32 {
        ms.set(step * 20);
        seat.0.store(u64::from(step) * 20_000, Ordering::Relaxed);
        player.pump();
        door.pump();
    }
    assert_ne!(mode_of(&front), MODE_PLAYING);
    assert_eq!(front.send(ID, CLOSE, 0, &mut Untouched), 0);
}

#[test]
fn a_read_that_fails_on_a_real_disc_is_one_line_and_silence() {
    let failing = Rc::new(Cell::new(false));
    let flag = Rc::clone(&failing);
    let disc = Disc::open(image(move |bytes| Flaky {
        bytes,
        failing: Rc::clone(&flag),
    }))
    .expect("the stand-in opens");
    let table = spans(&Panics, disc.tracks(), no_store_tracks);
    let tracks = Tracks(&table);
    let doors = Doors::default();
    let ms = Cell::new(0u32);
    let player = DoorPlayer::new(Panics, &doors, ImageSamples::new(&disc), tracks, || {
        ms.get()
    });
    let front = PlayFront::new(&ROW, Panics, tracks, || ms.get(), &player);
    failing.set(true);
    let lines = debug_lines(|| {
        assert_eq!(
            front.send(ID, PLAY, 4, &mut record([0, pack(2, 0), 0, 0, 0])),
            0
        );
        ms.set(100);
        player.pump();
    });
    let unread: Vec<_> = lines
        .iter()
        .filter(|line| line.starts_with("cd.track.unread"))
        .collect();
    assert_eq!(unread, ["cd.track.unread a=2 by=cd-audio"]);
    assert_eq!(doors.pushed.borrow().len(), 0);
    assert_eq!(mode_of(&front), MODE_PLAYING);
    // The file comes back, and the next track sounds at its own time.
    failing.set(false);
    // 38 frames of track 2 have passed by the clock, and the ms past rounding down.
    ms.set(TWO_FRAMES * 1000 / 75 + 1);
    player.pump();
    assert_eq!(
        doors.pushed.borrow().len(),
        (LEAD_FRAMES as usize) * FRAME_SAMPLES
    );
}

/// The seam: on another thread the player must move there. Fails to compile if it cannot.
#[allow(dead_code)]
fn the_player_can_move_to_another_thread() {
    fn moves<T: Send>() {}
    struct Shared;
    impl Host for Shared {
        fn now_us(&self) -> u64 {
            0
        }

        fn call_back(&self, _: u32, _: u32) {}

        fn read_file(&self, _: &str) -> Option<u32> {
            None
        }

        fn free_block(&self, _: u32) {}
    }
    moves::<
        DoorPlayer<
            'static,
            Panics,
            &'static sound_mix::Front<Shared, NullBack>,
            ImageSamples<Box<Disc<Vec<u8>>>>,
            fn() -> u32,
        >,
    >();
}
