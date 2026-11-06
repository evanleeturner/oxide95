// SPDX-License-Identifier: MIT
//! The playing front over a made-up disc: the clock is a number the test moves and the
//! player a list of what it was told, so every answer here is the front's own.

use std::cell::{Cell, RefCell};
use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::play::{Clock, PlayFront, Player};
use crate::table::{Place, Row, Span, Tracks};
use crate::test_log::debug_lines;
use crate::tests::{CALLS, OTHERS, Panics, Untouched, record, stopped};
use crate::tmsf::{pack, unpack};
use crate::{Front, Record};

const ID: u32 = 7;
const OPEN: u32 = 0x803;
const CLOSE: u32 = 0x804;
const PLAY: u32 = 0x806;
const STOP: u32 = 0x808;
const PAUSE: u32 = 0x809;
const SET: u32 = 0x80d;
const STATUS: u32 = 0x814;
const RESUME: u32 = 0x855;

const STOPPED: u32 = 525;
const PLAYING: u32 = 526;
const PAUSED: u32 = 529;
const OUT_OF_RANGE: u32 = 282;

static ROW: Row = Row {
    file: "a program",
    calls: CALLS,
    letters: 1,
    tracks: 22,
    device_name: "cdaudio",
    device_id: ID,
};

/// A data track 1 and no track 4 around them: 10 s, 4 s and 60 s of audio.
const DISC: [Span; 3] = [
    Span {
        number: 2,
        frames: 750,
    },
    Span {
        number: 3,
        frames: 300,
    },
    Span {
        number: 5,
        frames: 4500,
    },
];
const DISC_END: Place = Place {
    track: 5,
    frame: 4500,
};

const fn place(track: u8, frame: u32) -> Place {
    Place { track, frame }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Order {
    Play(Place, Place),
    Pause,
    Stop,
    Volume(u16, u16),
}

/// A player that keeps what it was told.
#[derive(Default)]
struct Heard(RefCell<Vec<Order>>);

impl Heard {
    fn take(&self) -> Vec<Order> {
        self.0.borrow_mut().drain(..).collect()
    }

    /// The last thing it was told about what sounds.
    fn last_run(&self) -> Option<Order> {
        self.0
            .borrow()
            .iter()
            .rev()
            .find(|order| !matches!(order, Order::Volume(..)))
            .copied()
    }
}

impl Player for Heard {
    fn play(&self, from: Place, to: Place) {
        self.0.borrow_mut().push(Order::Play(from, to));
    }

    fn pause(&self) {
        self.0.borrow_mut().push(Order::Pause);
    }

    fn stop(&self) {
        self.0.borrow_mut().push(Order::Stop);
    }

    fn volume(&self, left: u16, right: u16) {
        self.0.borrow_mut().push(Order::Volume(left, right));
    }
}

#[derive(Default)]
struct Rig {
    ms: Cell<u32>,
    heard: Heard,
}

impl Rig {
    fn front(&self) -> PlayFront<'static, Panics, impl Fn() -> u32, &Heard> {
        PlayFront::new(&ROW, Panics, Tracks(&DISC), || self.ms.get(), &self.heard)
    }

    fn later(&self, ms: u32) {
        self.ms.set(self.ms.get().wrapping_add(ms));
    }
}

fn ask<C: Clock, P: Player>(front: &PlayFront<'_, Panics, C, P>, item: u32) -> u32 {
    let mut asked = record([0, 0xAA, item, 0, 0]);
    assert_eq!(front.send(ID, STATUS, 0x100, &mut asked), 0);
    asked.words[1]
}

fn mode<C: Clock, P: Player>(front: &PlayFront<'_, Panics, C, P>) -> u32 {
    ask(front, 4)
}

fn position<C: Clock, P: Player>(front: &PlayFront<'_, Panics, C, P>) -> u32 {
    ask(front, 2)
}

fn track_word<C: Clock, P: Player>(front: &PlayFront<'_, Panics, C, P>, track: u32) -> u32 {
    let mut asked = record([0, 0xAA, 2, track, 0]);
    assert_eq!(front.send(ID, STATUS, 0x110, &mut asked), 0);
    asked.words[1]
}

fn play_from<C: Clock, P: Player>(front: &PlayFront<'_, Panics, C, P>, from: u32) -> u32 {
    front.send(ID, PLAY, 4, &mut record([0, from, 0, 0, 0]))
}

fn play_to<C: Clock, P: Player>(front: &PlayFront<'_, Panics, C, P>, from: u32, to: u32) -> u32 {
    front.send(ID, PLAY, 0xc, &mut record([0, from, to, 0, 0]))
}

fn bare<C: Clock, P: Player>(front: &PlayFront<'_, Panics, C, P>, message: u32) -> u32 {
    front.send(ID, message, 0, &mut Untouched)
}

#[test]
fn a_tracks_word_plays_from_its_first_frame() {
    let rig = Rig::default();
    let front = rig.front();
    for span in DISC {
        let word = track_word(&front, u32::from(span.number));
        assert_eq!(play_from(&front, word), 0);
        assert_eq!(
            rig.heard.take(),
            [Order::Play(place(span.number, 0), DISC_END)]
        );
        assert_eq!(position(&front), word);
    }
}

#[test]
fn a_place_off_the_disc_answers_282_writes_nothing_and_what_plays_goes_on() {
    let rig = Rig::default();
    let front = rig.front();
    assert_eq!(play_from(&front, pack(2, 0)), 0);
    rig.heard.take();
    rig.later(1000);
    let before = position(&front);
    for track in [0, 1, 4, 6, 255, 256, u32::MAX] {
        let words = [0, 0xAA, 2, track, 0];
        let mut asked = record(words);
        assert_eq!(front.send(ID, STATUS, 0x110, &mut asked), OUT_OF_RANGE);
        assert_eq!(asked.words, words, "track {track}");
    }
    // A track's own end is somewhere a run may stop, not somewhere it may start.
    let off = [
        pack(1, 0),
        pack(4, 0),
        pack(6, 0),
        pack(2, 750),
        pack(2, 751),
        pack(5, 4500),
    ];
    for word in off {
        let words = [0, word, 0, 0, 0];
        let mut asked = record(words);
        assert_eq!(front.send(ID, PLAY, 4, &mut asked), OUT_OF_RANGE);
        assert_eq!(asked.words, words, "from {word:#x}");
    }
    for word in [pack(1, 0), pack(4, 0), pack(6, 0), pack(3, 301)] {
        assert_eq!(play_to(&front, pack(2, 0), word), OUT_OF_RANGE);
    }
    assert_eq!(mode(&front), PLAYING);
    assert_eq!(position(&front), before);
    assert_eq!(rig.heard.take(), []);
}

/// Each of a walk's steps, the clock's among them.
#[derive(Clone, Copy, Debug)]
enum Step {
    PlayFrom,
    PlayTo,
    PlayOff,
    AskTrack,
    Pause,
    Resume,
    Stop,
    Close,
    Open,
    Format,
    Later,
    PastTheEnd,
}

const STEPS: [Step; 12] = [
    Step::PlayFrom,
    Step::PlayTo,
    Step::PlayOff,
    Step::AskTrack,
    Step::Pause,
    Step::Resume,
    Step::Stop,
    Step::Close,
    Step::Open,
    Step::Format,
    Step::Later,
    Step::PastTheEnd,
];

/// Two steps reach every state, so 4 meet every state with every step and look after it.
const WALK: u32 = 4;

fn take_step<C: Clock, P: Player>(rig: &Rig, front: &PlayFront<'_, Panics, C, P>, step: Step) {
    let answer = match step {
        Step::PlayFrom => play_from(front, pack(2, 0)),
        Step::PlayTo => play_to(front, pack(3, 0), pack(5, 0)),
        Step::PlayOff => play_from(front, pack(4, 0)) - OUT_OF_RANGE,
        Step::AskTrack => track_word(front, 5) - pack(5, 0),
        Step::Pause => bare(front, PAUSE),
        Step::Resume => bare(front, RESUME),
        Step::Stop => bare(front, STOP),
        Step::Close => bare(front, CLOSE),
        Step::Open => front.send(0, OPEN, 0x2000, &mut record([0; 5])),
        Step::Format => front.send(ID, SET, 0x400, &mut record([0, 10, 0, 0, 0])),
        Step::Later => {
            rig.later(1000);
            0
        }
        Step::PastTheEnd => {
            rig.later(100_000);
            0
        }
    };
    assert_eq!(answer, 0, "{step:?}");
}

#[test]
fn no_walk_reads_any_mode_but_stopped_playing_or_paused() {
    let many = STEPS.len();
    for walk in 0..many.pow(WALK) {
        let rig = Rig::default();
        let front = rig.front();
        let mut left = walk;
        for _ in 0..WALK {
            let step = STEPS[left % many];
            left /= many;
            take_step(&rig, &front, step);
            let read = mode(&front);
            let told = rig.heard.last_run();
            match read {
                PLAYING => assert!(matches!(told, Some(Order::Play(..))), "walk {walk}"),
                PAUSED => assert_eq!(told, Some(Order::Pause), "walk {walk}"),
                STOPPED => assert!(matches!(told, None | Some(Order::Stop)), "walk {walk}"),
                other => panic!("walk {walk}: mode {other} after {step:?}"),
            }
        }
    }
}

#[test]
fn the_position_runs_on_the_clock_alone() {
    let rig = Rig::default();
    // The run crosses the clock's wrap.
    rig.ms.set(0xffff_ff00);
    let front = rig.front();
    assert_eq!(play_from(&front, pack(2, 0)), 0);
    assert_eq!(position(&front), pack(2, 0));
    rig.later(2000);
    assert_eq!(position(&front), pack(2, 150));
    assert_eq!(mode(&front), PLAYING);

    assert_eq!(bare(&front, PAUSE), 0);
    rig.later(5000);
    assert_eq!(position(&front), pack(2, 150));
    assert_eq!(mode(&front), PAUSED);

    assert_eq!(bare(&front, RESUME), 0);
    assert_eq!(position(&front), pack(2, 150));
    rig.later(1000);
    assert_eq!(position(&front), pack(2, 225));
    assert_eq!(mode(&front), PLAYING);

    assert_eq!(bare(&front, STOP), 0);
    rig.later(9000);
    assert_eq!(position(&front), pack(2, 225));
    assert_eq!(mode(&front), STOPPED);

    // Not playing, not paused: 0, and nothing moves.
    assert_eq!(bare(&front, PAUSE), 0);
    assert_eq!(bare(&front, RESUME), 0);
    assert_eq!(bare(&front, STOP), 0);
    assert_eq!(position(&front), pack(2, 225));
    assert_eq!(mode(&front), STOPPED);

    assert_eq!(
        rig.heard.take(),
        [
            Order::Play(place(2, 0), DISC_END),
            Order::Pause,
            Order::Play(place(2, 150), DISC_END),
            Order::Stop,
        ]
    );
}

#[test]
fn the_pack_splits_by_bytes_and_carries_at_75_and_60() {
    let minute = 75 * 60;
    let cases = [
        (3, 0, [3, 0, 0, 0]),
        (3, 74, [3, 0, 0, 74]),
        (3, 75, [3, 0, 1, 0]),
        (3, minute - 1, [3, 0, 59, 74]),
        (3, minute, [3, 1, 0, 0]),
        (99, 74 * minute + 33 * 75 + 74, [99, 74, 33, 74]),
    ];
    for (track, frame, bytes) in cases {
        assert_eq!(pack(track, frame).to_le_bytes(), bytes);
        assert_eq!(unpack(pack(track, frame)), (track, frame));
    }
}

#[test]
fn a_play_with_an_end_ends_there_and_rests() {
    let rig = Rig::default();
    let front = rig.front();
    // One second into 2 to one second into 3: 750 frames, 10 s.
    assert_eq!(play_to(&front, pack(2, 75), pack(3, 75)), 0);
    rig.later(9999);
    assert_eq!(position(&front), pack(3, 74));
    assert_eq!(mode(&front), PLAYING);
    rig.later(1);
    assert_eq!(mode(&front), STOPPED);
    assert_eq!(position(&front), pack(3, 75));
    rig.later(999_000);
    assert_eq!(mode(&front), STOPPED);
    assert_eq!(position(&front), pack(3, 75));
    assert_eq!(
        rig.heard.take(),
        [Order::Play(place(2, 75), place(3, 75)), Order::Stop]
    );
}

#[test]
fn a_track_plays_to_the_start_of_the_next() {
    let rig = Rig::default();
    let front = rig.front();
    assert_eq!(play_to(&front, pack(3, 0), pack(5, 0)), 0);
    rig.later(4000);
    assert_eq!(mode(&front), STOPPED);
    assert_eq!(position(&front), pack(5, 0));
}

#[test]
fn a_play_with_no_end_crosses_tracks_to_the_discs_end() {
    let rig = Rig::default();
    let front = rig.front();
    assert_eq!(play_from(&front, pack(3, 290)), 0);
    rig.later(1000);
    assert_eq!(position(&front), pack(5, 65));
    assert_eq!(mode(&front), PLAYING);
    // 4,510 frames from there to the end: frame 4,509 at 60,133 ms, the end at 60,134.
    rig.ms.set(60_133);
    assert_eq!(position(&front), pack(5, 4499));
    assert_eq!(mode(&front), PLAYING);
    rig.ms.set(60_134);
    assert_eq!(mode(&front), STOPPED);
    assert_eq!(position(&front), pack(5, 4500));
    assert_eq!(
        rig.heard.take(),
        [Order::Play(place(3, 290), DISC_END), Order::Stop]
    );
}

#[test]
fn a_new_play_replaces_what_plays_and_a_second_end_is_not_told_twice() {
    let rig = Rig::default();
    let front = rig.front();
    assert_eq!(play_from(&front, pack(2, 0)), 0);
    rig.later(1000);
    assert_eq!(play_from(&front, pack(5, 0)), 0);
    assert_eq!(position(&front), pack(5, 0));
    rig.later(1000);
    assert_eq!(position(&front), pack(5, 75));
    assert_eq!(bare(&front, CLOSE), 0);
    assert_eq!(mode(&front), STOPPED);
    assert_eq!(
        position(&front),
        pack(2, 0),
        "a close goes back to the start"
    );
    assert_eq!(bare(&front, CLOSE), 0);
    assert_eq!(
        rig.heard.take(),
        [
            Order::Play(place(2, 0), DISC_END),
            Order::Play(place(5, 0), DISC_END),
            Order::Stop,
        ]
    );
}

#[test]
fn a_stop_while_a_run_ended_unread_is_not_told_twice() {
    let rig = Rig::default();
    let front = rig.front();
    assert_eq!(play_to(&front, pack(3, 0), pack(5, 0)), 0);
    rig.later(50_000);
    assert_eq!(bare(&front, PAUSE), 0);
    assert_eq!(mode(&front), STOPPED);
    assert_eq!(bare(&front, STOP), 0);
    assert_eq!(bare(&front, CLOSE), 0);
    assert_eq!(
        rig.heard.take(),
        [Order::Play(place(3, 0), place(5, 0)), Order::Stop]
    );
}

#[test]
fn a_bare_message_reads_no_record_and_other_flags_stop_by_name() {
    let rig = Rig::default();
    let front = rig.front();
    for message in [PAUSE, RESUME, STOP, CLOSE] {
        assert_eq!(bare(&front, message), 0, "{message:#x}");
    }
    let others = [
        (PAUSE, 1),
        (PAUSE, 2),
        (RESUME, 1),
        (RESUME, 4),
        (STOP, 1),
        (STOP, 2),
        (CLOSE, 1),
        (CLOSE, 2),
        (PLAY, 0),
        (PLAY, 1),
        (PLAY, 5),
        (PLAY, 6),
        (PLAY, 8),
        (PLAY, 0xd),
        (PLAY, 0x14),
        (STATUS, 0x101),
        (STATUS, 0x111),
        (STATUS, 0),
    ];
    for (message, flags) in others {
        let (name, lines) = stopped(|| {
            let _ = front.send(ID, message, flags, &mut record([0; 5]));
        });
        assert_eq!(
            name, "stopped: WINMM!mciSendCommandA",
            "{message:#x} {flags:#x}"
        );
        assert_eq!(lines.len(), 1, "{message:#x} {flags:#x}: {lines:?}");
        assert!(lines[0].starts_with("cd.stop "), "{}", lines[0]);
    }
    assert_eq!(rig.heard.take(), []);
}

#[test]
fn an_item_or_a_run_no_row_answers_stops_by_name_before_it_writes() {
    let rig = Rig::default();
    let front = rig.front();
    assert_eq!(play_from(&front, pack(3, 0)), 0);
    rig.heard.take();
    let asks: [(u32, u32, [u32; 5]); 4] = [
        (STATUS, 0x100, [0, 0xAA, 1, 0, 0]),
        (STATUS, 0x110, [0, 0xAA, 4, 5, 0]),
        (PLAY, 0xc, [0, pack(5, 0), pack(3, 0), 0, 0]),
        (PLAY, 0xc, [0, pack(3, 9), pack(3, 9), 0, 0]),
    ];
    for (message, flags, words) in asks {
        let mut asked = record(words);
        let (name, lines) = stopped(|| {
            let _ = front.send(ID, message, flags, &mut asked);
        });
        assert_eq!(name, "stopped: WINMM!mciSendCommandA");
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert_eq!(asked.words, words);
    }
    assert_eq!(rig.heard.take(), []);
}

#[test]
fn the_door_is_never_asked_for() {
    let rig = Rig::default();
    let front = rig.front();
    let (name, lines) = stopped(|| {
        let _ = front.send(ID, SET, 0x100, &mut record([0; 5]));
    });
    assert_eq!(name, "stopped: WINMM!mciSendCommandA");
    assert_eq!(
        lines,
        ["cd.stop c=WINMM!mciSendCommandA a=7,80d,100 by=cd-audio"]
    );
    assert_eq!(front.send(ID, SET, 0x400, &mut record([0, 10, 0, 0, 0])), 0);
}

/// `AUXCAPSA`'s 12 words.
struct Caps([u32; 12]);

impl Record for Caps {
    fn word(&self, index: usize) -> u32 {
        self.0[index]
    }

    fn set_word(&mut self, index: usize, value: u32) {
        self.0[index] = value;
    }

    fn write_through(&mut self, _: usize, _: &[&[u8]]) {
        panic!("a record of caps holds no address")
    }
}

#[test]
fn the_aux_device_is_one_cd_audio_device_of_one_size() {
    let rig = Rig::default();
    let front = rig.front();
    assert_eq!(front.aux_devices(), 1);
    let mut caps = Caps([0xAAAA_AAAA; 12]);
    assert_eq!(front.aux_caps(0, 0x30, &mut caps), 0);
    assert_eq!(caps.0, [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 3]);
    for (device, size) in [(0, 0x2f), (0, 0x31), (0, 0), (1, 0x30)] {
        let mut caps = Caps([0xAAAA_AAAA; 12]);
        let (name, lines) = stopped(|| {
            let _ = front.aux_caps(device, size, &mut caps);
        });
        assert_eq!(name, "stopped: WINMM!auxGetDevCapsA");
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert_eq!(caps.0, [0xAAAA_AAAA; 12]);
    }
}

#[test]
fn a_volume_reaches_the_player_each_half_its_own_side() {
    let rig = Rig::default();
    let front = rig.front();
    assert_eq!(front.aux_set_volume(0, 0x8000_ffff), 0);
    assert_eq!(rig.heard.take(), [Order::Volume(0xffff, 0x8000)]);
    let (name, lines) = stopped(|| {
        let _ = front.aux_set_volume(1, 0);
    });
    assert_eq!(name, "stopped: WINMM!auxSetVolume");
    assert_eq!(lines, ["cd.stop c=WINMM!auxSetVolume a=1,0 by=cd-audio"]);
    assert_eq!(rig.heard.take(), []);
}

#[test]
fn each_answer_is_one_line_saying_who() {
    let rig = Rig::default();
    let front = rig.front();
    let lines = debug_lines(|| {
        assert_eq!(play_from(&front, pack(2, 75)), 0);
        assert_eq!(mode(&front), PLAYING);
        assert_eq!(position(&front), pack(2, 75));
        assert_eq!(track_word(&front, 3), pack(3, 0));
        assert_eq!(play_to(&front, pack(2, 0), pack(3, 0)), 0);
        assert_eq!(ask(&front, 3), 5);
        assert_eq!(bare(&front, PAUSE), 0);
        assert_eq!(bare(&front, RESUME), 0);
        assert_eq!(bare(&front, STOP), 0);
        assert_eq!(play_from(&front, pack(4, 0)), OUT_OF_RANGE);
        assert_eq!(bare(&front, CLOSE), 0);
        assert_eq!(front.aux_devices(), 1);
        assert_eq!(front.aux_caps(0, 0x30, &mut Caps([0; 12])), 0);
        assert_eq!(front.aux_set_volume(0, 0xffff), 0);
    });
    let send = "imp.call c=WINMM!mciSendCommandA a=7,";
    assert_eq!(
        lines,
        [
            format!("{send}806,4,10002,0 v=0 by=cd-audio"),
            format!("{send}814,100,4 v=0 o=20e by=cd-audio"),
            format!("{send}814,100,2 v=0 o=10002 by=cd-audio"),
            format!("{send}814,110,2,3 v=0 o=3 by=cd-audio"),
            format!("{send}806,c,2,3 v=0 by=cd-audio"),
            format!("{send}814,100,3 v=0 o=5 by=cd-audio"),
            format!("{send}809,0 v=0 by=cd-audio"),
            format!("{send}855,0 v=0 by=cd-audio"),
            format!("{send}808,0 v=0 by=cd-audio"),
            format!("{send}806,4,4,0 v=11a by=cd-audio"),
            format!("{send}804,0 v=0 by=cd-audio"),
            "imp.call c=WINMM!auxGetNumDevs v=1 by=cd-audio".to_owned(),
            "imp.call c=WINMM!auxGetDevCapsA a=0,30 v=0 o=1 by=cd-audio".to_owned(),
            "imp.call c=WINMM!auxSetVolume a=0,ffff v=0 by=cd-audio".to_owned(),
        ]
    );
}

/// All one call did, for comparing two fronts.
#[derive(Debug, PartialEq, Eq)]
struct Outcome {
    answer: Result<u32, String>,
    words: [u32; 5],
    through: Vec<u8>,
    lines: Vec<String>,
}

fn outcome(words: [u32; 5], call: impl FnOnce(&mut crate::tests::Words) -> u32) -> Outcome {
    let mut asked = record(words);
    let mut answer = Err(String::new());
    let lines = debug_lines(|| {
        answer = catch_unwind(AssertUnwindSafe(|| call(&mut asked))).map_err(|panic| {
            panic
                .downcast_ref::<String>()
                .cloned()
                .unwrap_or_else(|| "not a stop by name".to_owned())
        });
    });
    Outcome {
        answer,
        words: asked.words,
        through: asked.through,
        lines,
    }
}

/// Would fail the comparison if anything told it anything.
struct NothingToPlay;

impl Player for NothingToPlay {
    fn play(&self, _: Place, _: Place) {
        panic!("played over no tracks")
    }

    fn pause(&self) {
        panic!("paused over no tracks")
    }

    fn stop(&self) {
        panic!("stopped over no tracks")
    }

    fn volume(&self, _: u16, _: u16) {
        panic!("a volume over no tracks")
    }
}

#[test]
fn over_no_tracks_it_answers_as_the_quiet_front_call_for_call() {
    let quiet = Front::new(&ROW, Panics);
    let clock = || -> u32 { panic!("read the clock over no tracks") };
    let played = PlayFront::new(&ROW, Panics, Tracks(&[]), clock, NothingToPlay);
    let answered: [(u32, u32, u32, [u32; 5]); 7] = [
        (0, 0x810, 0x100, [0, 0xAA, 4, 0, 0x204]),
        (0, 0x810, 0x400, [0, 0xBB, 0x20, 1, 0x204]),
        (0, OPEN, 0x2000, [0; 5]),
        (ID, SET, 0x400, [0, 10, 0, 0, 0]),
        (ID, STATUS, 0x100, [0, 0, 3, 0, 0]),
        (ID, STATUS, 0x100, [0, 0x55, 4, 0, 0]),
        (ID, CLOSE, 0, [0; 5]),
    ];
    for (device, message, flags, words) in answered.into_iter().chain(OTHERS) {
        let one = outcome(words, |asked| quiet.send(device, message, flags, asked));
        let two = outcome(words, |asked| played.send(device, message, flags, asked));
        assert_eq!(one, two, "{device} {message:#x} {flags:#x}");
    }
    let one = outcome([0; 5], |_| quiet.aux_devices());
    let two = outcome([0; 5], |_| played.aux_devices());
    assert_eq!(one, two);
    assert_eq!(two.answer, Ok(0));

    let one = outcome([0; 5], |_| quiet.aux_caps(0, 0x30));
    let two = outcome([0; 5], |asked| played.aux_caps(0, 0x30, asked));
    assert_eq!(one, two);
    let one = outcome([0; 5], |_| quiet.aux_set_volume(0, 0xffff));
    let two = outcome([0; 5], |_| played.aux_set_volume(0, 0xffff));
    assert_eq!(one, two);
    let one = outcome([0; 5], |_| quiet.aux_get_volume(0));
    let two = outcome([0; 5], |_| played.aux_get_volume(0));
    assert_eq!(one, two);
    let one = outcome([0; 5], |_| quiet.error_string(274, 0x20));
    let two = outcome([0; 5], |_| played.error_string(274, 0x20));
    assert_eq!(one, two);
}
