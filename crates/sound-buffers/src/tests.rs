// SPDX-License-Identifier: MIT
//! The stand-in on a door and a clock of the test's, and at the last on the real
//! mixer. The expected numbers were worked by hand before the code first ran: at
//! 10,000 Hz a frame is 100 microseconds, so the lead of 46,440 is 464 frames.
#![expect(
    unsafe_code,
    reason = "a program's raw calls, made by hand through the tables"
)]
#![expect(
    clippy::cast_possible_truncation,
    reason = "a test's buffers and an address's low word"
)]

use core::mem::{offset_of, size_of};
use std::cell::{Cell, RefCell};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr::{null, null_mut};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use sound_mix::output::Output;
use sound_mix::{Front, Host};

use crate::buffers::{Back, Buffers, SAMPLE_FRACTION, Shape, WAVE_BYTES, gain};
use crate::com::{BufferTable, Com, ObjectTable, This, sdk_offset};
use crate::mix_back::MixBack;
use crate::sdk::{
    DS_OK, DSBCAPS_PRIMARYBUFFER, DSBLOCK_ENTIREBUFFER, DSBLOCK_FROMWRITECURSOR, DSBPLAY_LOOPING,
    DSBSTATUS_LOOPING, DSBSTATUS_PLAYING, DSERR_BADFORMAT, DSERR_INVALIDCALL, DSERR_INVALIDPARAM,
    DSERR_NOAGGREGATION, DSERR_UNSUPPORTED, Desc, WAVE_FORMAT_PCM, WaveFormat,
};
use crate::table::{FRONTS, Row, STOPS};
use crate::test_log::debug_lines;

static ROW: Row = Row {
    file: "a program",
    levels: &[2],
};

const RATE: u32 = 10_000;
/// The lead at [`RATE`]: 46,440 microseconds, whole frames.
const LEAD_FRAMES: u32 = 464;
/// A mark in an out word, so one left unwritten shows.
const UNWRITTEN: u32 = 0xaaaa_aaaa;

/// What the fake door was told, and the clock.
#[derive(Default)]
struct Seat {
    now_us: Cell<u64>,
    opens: RefCell<Vec<(u32, u32)>>,
    pushes: RefCell<Vec<(u32, Vec<i16>)>>,
    closes: RefCell<Vec<u32>>,
}

impl Seat {
    fn at(&self, now_us: u64) {
        self.now_us.set(now_us);
    }

    /// Every sample a door was handed, in order.
    fn samples(&self, door: u32) -> Vec<i16> {
        self.pushes
            .borrow()
            .iter()
            .filter(|(to, _)| *to == door)
            .flat_map(|(_, samples)| samples.iter().copied())
            .collect()
    }
}

struct Fake(Rc<Seat>);

impl Back for Fake {
    fn now_us(&self) -> u64 {
        self.0.now_us.get()
    }

    fn stream_open(&self, rate_hz: u32, channels: u32) -> u32 {
        let mut opens = self.0.opens.borrow_mut();
        opens.push((rate_hz, channels));
        opens.len() as u32
    }

    fn stream_push(&self, door: u32, samples: &[i16]) {
        self.0.pushes.borrow_mut().push((door, samples.to_vec()));
    }

    fn stream_close(&self, door: u32) {
        self.0.closes.borrow_mut().push(door);
    }
}

type Buffer = This<Fake>;

struct Rig {
    com: &'static Com<Fake>,
    seat: Rc<Seat>,
    object: Buffer,
}

const fn wave(rate_hz: u32, channels: u16, bits: u16) -> WaveFormat {
    let block = channels * bits / 8;
    WaveFormat {
        tag: WAVE_FORMAT_PCM,
        channels,
        rate_hz,
        bytes_per_second: rate_hz * block as u32,
        block_align: block,
        bits,
        extra: 0,
    }
}

const fn desc(flags: u32, bytes: u32, format: *const WaveFormat) -> Desc {
    Desc {
        size: 20,
        flags,
        bytes,
        reserved: 0,
        format,
    }
}

fn object_slots(object: Buffer) -> &'static ObjectTable<Fake> {
    // SAFETY: an object's first word is its table, which lives as long as the program.
    unsafe { &*(*object).table.cast() }
}

fn slots(buffer: Buffer) -> &'static BufferTable<Fake> {
    // SAFETY: a buffer's first word is its table, which lives as long as the program.
    unsafe { &*(*buffer).table.cast() }
}

fn id_of(buffer: Buffer) -> u32 {
    // SAFETY: a test's buffers came from its `Com`, which it leaked.
    unsafe { (*buffer).id }
}

fn rig() -> Rig {
    let seat = Rc::new(Seat::default());
    let com = Box::leak(Box::new(Com::new(Buffers::new(
        &ROW,
        Fake(Rc::clone(&seat)),
    ))));
    let mut object = null_mut();
    // SAFETY: `object` is this function's word.
    let answer = unsafe { com.direct_sound_create(0, &raw mut object, 0) };
    assert_eq!(answer, DS_OK);
    Rig { com, seat, object }
}

impl Rig {
    fn create(&self, flags: u32, bytes: u32, format: *const WaveFormat) -> (u32, Buffer) {
        let asked = desc(flags, bytes, format);
        let mut made = null_mut();
        // SAFETY: the object is live, and the rest are this function's.
        let code = unsafe {
            (object_slots(self.object).create_sound_buffer)(
                self.object,
                &raw const asked,
                &raw mut made,
                0,
            )
        };
        (code, made)
    }

    fn buffer(&self, format: WaveFormat, bytes: u32) -> Buffer {
        let (code, made) = self.create(0, bytes, &raw const format);
        assert_eq!(code, DS_OK);
        made
    }

    fn primary(&self) -> Buffer {
        let (code, made) = self.create(DSBCAPS_PRIMARYBUFFER, 0, null());
        assert_eq!(code, DS_OK);
        made
    }

    /// A 16-bit mono buffer at [`RATE`] of `frames`, each `fill`'s value.
    fn mono(&self, frames: u32, fill: impl Fn(u32) -> i16) -> Buffer {
        let buffer = self.buffer(wave(RATE, 1, 16), frames * 2);
        for frame in 0..frames {
            self.write(buffer, frame * 2, &fill(frame).to_le_bytes());
        }
        buffer
    }

    /// Writes as a program writes through a word from `Lock`.
    fn write(&self, buffer: Buffer, at: u32, bytes: &[u8]) {
        self.com
            .buffers()
            .with_bytes(id_of(buffer), |cells| {
                for (cell, byte) in cells[at as usize..].iter().zip(bytes) {
                    cell.set(*byte);
                }
            })
            .expect("a test writes a live buffer");
    }

    /// The low word of a buffer's first byte, what `Lock` answers.
    fn base(&self, buffer: Buffer) -> u32 {
        self.com
            .buffers()
            .with_bytes(id_of(buffer), |cells| cells.as_ptr() as usize as u32)
            .expect("a test asks of a live buffer")
    }

    fn pump(&self) {
        self.com.buffers().pump();
    }

    fn pushes(&self) -> usize {
        self.seat.pushes.borrow().len()
    }
}

fn play(buffer: Buffer, flags: u32) -> u32 {
    // SAFETY: a test's buffers stay on their kept tables.
    unsafe { (slots(buffer).play)(buffer, 0, 0, flags) }
}

fn stop(buffer: Buffer) -> u32 {
    // SAFETY: as `play`.
    unsafe { (slots(buffer).stop)(buffer) }
}

fn release(buffer: Buffer) -> u32 {
    // SAFETY: as `play`.
    unsafe { (slots(buffer).release)(buffer) }
}

fn set_position(buffer: Buffer, byte: u32) -> u32 {
    // SAFETY: as `play`.
    unsafe { (slots(buffer).set_current_position)(buffer, byte) }
}

fn set_volume(buffer: Buffer, volume: i32) -> u32 {
    // SAFETY: as `play`.
    unsafe { (slots(buffer).set_volume)(buffer, volume) }
}

fn set_pan(buffer: Buffer, pan: i32) -> u32 {
    // SAFETY: as `play`.
    unsafe { (slots(buffer).set_pan)(buffer, pan) }
}

fn set_frequency(buffer: Buffer, rate_hz: u32) -> u32 {
    // SAFETY: as `play`.
    unsafe { (slots(buffer).set_frequency)(buffer, rate_hz) }
}

fn status(buffer: Buffer) -> u32 {
    let mut out = UNWRITTEN;
    // SAFETY: as `play`, and `out` is this function's.
    let code = unsafe { (slots(buffer).get_status)(buffer, &raw mut out) };
    assert_eq!(code, DS_OK);
    out
}

fn position(buffer: Buffer) -> (u32, u32) {
    let (mut play_byte, mut write_byte) = (UNWRITTEN, UNWRITTEN);
    // SAFETY: as `play`, and both words are this function's.
    let code = unsafe {
        (slots(buffer).get_current_position)(buffer, &raw mut play_byte, &raw mut write_byte)
    };
    assert_eq!(code, DS_OK);
    (play_byte, write_byte)
}

fn volume(buffer: Buffer) -> i32 {
    let mut out = 1;
    // SAFETY: as `play`, and `out` is this function's.
    let code = unsafe { (slots(buffer).get_volume)(buffer, &raw mut out) };
    assert_eq!(code, DS_OK);
    out
}

fn pan(buffer: Buffer) -> i32 {
    let mut out = 1;
    // SAFETY: as `play`, and `out` is this function's.
    let code = unsafe { (slots(buffer).get_pan)(buffer, &raw mut out) };
    assert_eq!(code, DS_OK);
    out
}

/// `Lock` with all 4 out words: the answer and the words as it left them.
fn lock(buffer: Buffer, offset: u32, bytes: u32, flags: u32) -> (u32, [u32; 4]) {
    let mut out = [UNWRITTEN; 4];
    let [first, first_bytes, second, second_bytes] = &mut out;
    // SAFETY: as `play`, and the 4 words are this function's.
    let code = unsafe {
        (slots(buffer).lock)(
            buffer,
            offset,
            bytes,
            first,
            first_bytes,
            second,
            second_bytes,
            flags,
        )
    };
    (code, out)
}

/// A frame's value in a ramp no sample's range cuts.
fn ramp(frame: u32) -> i16 {
    (frame % 30_000) as i16
}

/// Mono values as a stereo door hears them.
fn doubled(values: impl IntoIterator<Item = i16>) -> Vec<i16> {
    values
        .into_iter()
        .flat_map(|value| [value, value])
        .collect()
}

// Placed: the names, the numbers, the tables.

#[test]
fn the_names_are_the_censuss_as_widened_by_the_sweep() {
    assert_eq!(FRONTS.len(), 21);
    assert_eq!(STOPS.len(), 10);
    assert_eq!(FRONTS[0], "DSOUND!DirectSoundCreate");
    let of = |prefix: &str| STOPS.iter().filter(|name| name.starts_with(prefix)).count();
    assert_eq!(of("MSACM32!acmStream"), 5);
    assert_eq!(of("WINMM!waveIn"), 5);
}

/// By number, from `dsound.h`, not from the table they fence.
#[test]
fn each_front_sits_at_its_number_in_the_header() {
    let object = [
        ("Release", offset_of!(ObjectTable<()>, release), 0x08),
        (
            "CreateSoundBuffer",
            offset_of!(ObjectTable<()>, create_sound_buffer),
            0x0c,
        ),
        (
            "DuplicateSoundBuffer",
            offset_of!(ObjectTable<()>, duplicate_sound_buffer),
            0x14,
        ),
        (
            "SetCooperativeLevel",
            offset_of!(ObjectTable<()>, set_cooperative_level),
            0x18,
        ),
    ];
    let buffer = [
        ("Release", offset_of!(BufferTable<()>, release), 0x08),
        (
            "GetCurrentPosition",
            offset_of!(BufferTable<()>, get_current_position),
            0x10,
        ),
        ("GetFormat", offset_of!(BufferTable<()>, get_format), 0x14),
        ("GetVolume", offset_of!(BufferTable<()>, get_volume), 0x18),
        ("GetPan", offset_of!(BufferTable<()>, get_pan), 0x1c),
        ("GetStatus", offset_of!(BufferTable<()>, get_status), 0x24),
        ("Lock", offset_of!(BufferTable<()>, lock), 0x2c),
        ("Play", offset_of!(BufferTable<()>, play), 0x30),
        (
            "SetCurrentPosition",
            offset_of!(BufferTable<()>, set_current_position),
            0x34,
        ),
        ("SetFormat", offset_of!(BufferTable<()>, set_format), 0x38),
        ("SetVolume", offset_of!(BufferTable<()>, set_volume), 0x3c),
        ("SetPan", offset_of!(BufferTable<()>, set_pan), 0x40),
        (
            "SetFrequency",
            offset_of!(BufferTable<()>, set_frequency),
            0x44,
        ),
        ("Stop", offset_of!(BufferTable<()>, stop), 0x48),
        ("Unlock", offset_of!(BufferTable<()>, unlock), 0x4c),
        ("Restore", offset_of!(BufferTable<()>, restore), 0x50),
    ];
    let mut named = vec![FRONTS[0].to_owned()];
    for (name, at, wanted) in object {
        assert_eq!(sdk_offset(at), wanted, "{name}");
        named.push(format!("IDirectSound::{name}"));
    }
    for (name, at, wanted) in buffer {
        assert_eq!(sdk_offset(at), wanted, "{name}");
        named.push(format!("IDirectSoundBuffer::{name}"));
    }
    assert_eq!(named, FRONTS);
    assert_eq!(size_of::<ObjectTable<()>>() / size_of::<usize>(), 11);
    assert_eq!(size_of::<BufferTable<()>>() / size_of::<usize>(), 21);
}

#[test]
fn no_slot_is_null() {
    let rig = rig();
    let buffer = rig.mono(8, ramp);
    // SAFETY: each table is a whole table of function words, read as words.
    let (object_words, buffer_words) = unsafe {
        (
            std::slice::from_raw_parts((*rig.object).table.cast::<usize>(), 11),
            std::slice::from_raw_parts((*buffer).table.cast::<usize>(), 21),
        )
    };
    assert!(
        object_words
            .iter()
            .chain(buffer_words)
            .all(|word| *word != 0)
    );
}

/// One absent slot's call: its interface, its offset, and the call itself.
type AbsentCall = (&'static str, u32, Box<dyn Fn() -> u32>);

/// 7 of the object's and 5 of the buffer's: each called, each one line.
#[test]
fn a_slot_with_no_front_answers_unsupported_with_one_line() {
    let rig = rig();
    let object = rig.object;
    let buffer = rig.mono(8, ramp);
    let (o, b) = (object_slots(object), slots(buffer));
    // SAFETY: both interfaces are live, and a stub reads none of its arguments.
    let calls: [AbsentCall; 12] = unsafe {
        [
            (
                "IDirectSound",
                0x00,
                Box::new(move || (o.query_interface)(object, 0, 0)),
            ),
            ("IDirectSound", 0x04, Box::new(move || (o.add_ref)(object))),
            (
                "IDirectSound",
                0x10,
                Box::new(move || (o.get_caps)(object, 0)),
            ),
            ("IDirectSound", 0x1c, Box::new(move || (o.compact)(object))),
            (
                "IDirectSound",
                0x20,
                Box::new(move || (o.get_speaker_config)(object, 0)),
            ),
            (
                "IDirectSound",
                0x24,
                Box::new(move || (o.set_speaker_config)(object, 0)),
            ),
            (
                "IDirectSound",
                0x28,
                Box::new(move || (o.initialize)(object, 0)),
            ),
            (
                "IDirectSoundBuffer",
                0x00,
                Box::new(move || (b.query_interface)(buffer, 0, 0)),
            ),
            (
                "IDirectSoundBuffer",
                0x04,
                Box::new(move || (b.add_ref)(buffer)),
            ),
            (
                "IDirectSoundBuffer",
                0x0c,
                Box::new(move || (b.get_caps)(buffer, 0)),
            ),
            (
                "IDirectSoundBuffer",
                0x20,
                Box::new(move || (b.get_frequency)(buffer, 0)),
            ),
            (
                "IDirectSoundBuffer",
                0x28,
                Box::new(move || (b.initialize)(buffer, 0, 0)),
            ),
        ]
    };
    for (name, at, call) in calls {
        let mut answer = 0;
        let lines = debug_lines(|| answer = call());
        assert_eq!(answer, DSERR_UNSUPPORTED, "{name} {at:x}");
        assert_eq!(
            lines,
            [format!(
                "sound.buffers.absent c={name} i={at:x} by=sound-buffers"
            )]
        );
    }
}

/// The stop's line and the panic's text, for a name of [`STOPS`].
fn stopped_at(name: &str) -> (Vec<String>, String) {
    let rig = rig();
    let mut ended = None;
    let lines = debug_lines(|| {
        ended = Some(catch_unwind(AssertUnwindSafe(|| {
            rig.com.buffers().named_stop(name);
        })));
    });
    let payload = ended
        .expect("the call ran")
        .expect_err("a stop never answers");
    let text = payload
        .downcast_ref::<String>()
        .expect("a stop's panic is a formatted message")
        .clone();
    (lines, text)
}

#[test]
fn each_of_the_ten_stops_ends_the_program_by_name() {
    for name in STOPS {
        let (lines, text) = stopped_at(name);
        assert_eq!(
            lines,
            [format!(
                "sound.buffers.named.stop c={name} file=\"a program\" by=sound-buffers"
            )]
        );
        assert!(text.contains(name), "{text}");
    }
}

/// The trap: a size query is the first thing a program asks of conversion, and no
/// size here would be true, so nothing is answered at all.
#[test]
fn acm_stream_size_reaches_its_stop_and_answers_nothing() {
    let (lines, text) = stopped_at("MSACM32!acmStreamSize");
    assert_eq!(lines.len(), 1);
    assert!(lines[0].starts_with("sound.buffers.named.stop c=MSACM32!acmStreamSize"));
    assert!(text.contains("acmStreamSize"), "{text}");
}

// `Lock`, on a ring of 1,000 bytes: numbers by hand.

#[test]
fn lock_gives_the_two_spans_of_the_ring() {
    let rig = rig();
    let buffer = rig.mono(500, ramp);
    let base = rig.base(buffer);
    assert_eq!(lock(buffer, 0, 100, 0), (DS_OK, [base, 100, 0, 0]));
    assert_eq!(
        lock(buffer, 900, 300, 0),
        (DS_OK, [base + 900, 100, base, 200])
    );
    assert_eq!(
        lock(buffer, 7, 1, DSBLOCK_ENTIREBUFFER),
        (DS_OK, [base, 1000, 0, 0])
    );
}

#[test]
fn lock_refuses_and_writes_no_span() {
    let rig = rig();
    let buffer = rig.mono(500, ramp);
    for (offset, bytes, flags, code) in [
        (1000, 1, 0, DSERR_INVALIDPARAM),
        (1001, 1, 0, DSERR_INVALIDPARAM),
        (0, 0, 0, DSERR_INVALIDPARAM),
        (0, 1001, 0, DSERR_INVALIDPARAM),
        (0, 10, DSBLOCK_FROMWRITECURSOR, DSERR_INVALIDPARAM),
    ] {
        let mut answer = (0, [0; 4]);
        let lines = debug_lines(|| answer = lock(buffer, offset, bytes, flags));
        assert_eq!(answer, (code, [UNWRITTEN; 4]), "{offset} {bytes} {flags}");
        assert_eq!(lines.len(), 1, "{lines:?}");
    }
    let (mut word, mut length) = (UNWRITTEN, UNWRITTEN);
    // SAFETY: as `play`; a null pointer is what these calls test.
    let (no_first, no_length) = unsafe {
        let call = slots(buffer).lock;
        (
            call(
                buffer,
                0,
                10,
                null_mut(),
                &raw mut length,
                null_mut(),
                null_mut(),
                0,
            ),
            call(
                buffer,
                0,
                10,
                &raw mut word,
                null_mut(),
                null_mut(),
                null_mut(),
                0,
            ),
        )
    };
    assert_eq!(
        (no_first, no_length),
        (DSERR_INVALIDPARAM, DSERR_INVALIDPARAM)
    );
    assert_eq!((word, length), (UNWRITTEN, UNWRITTEN));

    let primary = rig.primary();
    assert_eq!(lock(primary, 0, 10, 0), (DSERR_INVALIDCALL, [UNWRITTEN; 4]));
}

/// Chosen here: with no second pointer a wrap gives the first span alone.
#[test]
fn a_wrap_with_a_null_second_pointer_gives_the_first_span_alone() {
    let rig = rig();
    let buffer = rig.mono(500, ramp);
    let base = rig.base(buffer);
    let (mut word, mut length) = (UNWRITTEN, UNWRITTEN);
    // SAFETY: as `play`, and both words are this test's.
    let code = unsafe {
        (slots(buffer).lock)(
            buffer,
            900,
            300,
            &raw mut word,
            &raw mut length,
            null_mut(),
            null_mut(),
            0,
        )
    };
    assert_eq!((code, word, length), (DS_OK, base + 900, 100));
}

#[test]
fn unlock_answers_0_with_its_numbers_on_the_line() {
    let rig = rig();
    let buffer = rig.mono(500, ramp);
    let mut answer = 1;
    let lines = debug_lines(|| {
        // SAFETY: as `play`; `Unlock` reads no pointer.
        answer = unsafe { (slots(buffer).unlock)(buffer, 0, 100, 0, 200) };
    });
    assert_eq!(answer, DS_OK);
    assert_eq!(
        lines,
        ["imp.call c=IDirectSoundBuffer::Unlock a=1,64,c8 v=0 by=sound-buffers"]
    );
}

// The door, by the clock: numbers by hand.

#[test]
fn play_pushes_the_lead_then_only_what_the_clock_adds() {
    let rig = rig();
    let buffer = rig.mono(4000, ramp);
    assert_eq!(play(buffer, 0), DS_OK);
    assert_eq!(*rig.seat.opens.borrow(), [(RATE, 2)]);
    assert_eq!(rig.seat.samples(1), doubled((0..LEAD_FRAMES).map(ramp)));

    rig.seat.at(1_000);
    rig.pump();
    assert_eq!(
        rig.seat.samples(1),
        doubled((0..LEAD_FRAMES + 10).map(ramp))
    );
    assert_eq!(rig.pushes(), 2);
    rig.pump();
    assert_eq!(rig.pushes(), 2, "the clock has not moved: no push");
    rig.seat.at(1_050);
    rig.pump();
    assert_eq!(rig.pushes(), 2, "half a frame is no frame");
    rig.seat.at(1_100);
    rig.pump();
    assert_eq!(rig.seat.samples(1).len(), (LEAD_FRAMES as usize + 11) * 2);
}

#[test]
fn the_door_hears_what_the_format_says() {
    // 16-bit stereo, unchanged at full volume and centre pan.
    let rig = rig();
    let stereo: Vec<i16> = vec![1, -2, 300, -400, 32_767, -32_768];
    let buffer = rig.buffer(wave(RATE, 2, 16), 12);
    let bytes: Vec<u8> = stereo
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect();
    rig.write(buffer, 0, &bytes);
    assert_eq!(play(buffer, 0), DS_OK);
    assert_eq!(rig.seat.samples(1), stereo);

    // 16-bit mono, doubled.
    let buffer = rig.mono(3, |frame| [5, -6, 7][frame as usize]);
    assert_eq!(play(buffer, 0), DS_OK);
    assert_eq!(rig.seat.samples(2), [5, 5, -6, -6, 7, 7]);

    // 8-bit mono: `(v - 128) << 8`.
    let buffer = rig.buffer(wave(RATE, 1, 8), 3);
    rig.write(buffer, 0, &[0, 128, 255]);
    assert_eq!(play(buffer, 0), DS_OK);
    assert_eq!(
        rig.seat.samples(3),
        [-32_768, -32_768, 0, 0, 32_512, 32_512]
    );

    // 8-bit stereo.
    let buffer = rig.buffer(wave(RATE, 2, 8), 2);
    rig.write(buffer, 0, &[129, 127]);
    assert_eq!(play(buffer, 0), DS_OK);
    assert_eq!(rig.seat.samples(4), [256, -256]);
}

/// 20 pumps a 30th of a second apart: 666,660 microseconds and the lead is 713,100,
/// 7,131 frames, across the seam of a ring of 1,000 seven times.
#[test]
fn a_looping_ring_has_no_gap_and_no_repeat_across_its_seam() {
    let rig = rig();
    let buffer = rig.mono(1000, ramp);
    assert_eq!(play(buffer, DSBPLAY_LOOPING), DS_OK);
    assert_eq!(status(buffer), DSBSTATUS_PLAYING | DSBSTATUS_LOOPING);
    for pump in 1..=20 {
        rig.seat.at(pump * 33_333);
        rig.pump();
    }
    assert_eq!(
        rig.seat.samples(1),
        doubled((0..7_131).map(|frame| ramp(frame % 1000)))
    );
    assert!(rig.seat.closes.borrow().is_empty());
}

/// 1,000 frames is 100,000 microseconds.
#[test]
fn a_ring_not_looping_ends_by_the_clock_and_leaves_its_door_to_drain() {
    let rig = rig();
    let buffer = rig.mono(1000, ramp);
    assert_eq!(play(buffer, 0), DS_OK);
    assert_eq!(status(buffer), DSBSTATUS_PLAYING);
    for now_us in [33_300, 66_600, 99_900] {
        rig.seat.at(now_us);
        rig.pump();
        assert_eq!(status(buffer), DSBSTATUS_PLAYING, "{now_us}");
    }
    assert_eq!(rig.seat.samples(1), doubled((0..1000).map(ramp)));

    rig.seat.at(100_000);
    let lines = debug_lines(|| rig.pump());
    assert_eq!(lines, ["sound.buffers.ended a=1 by=sound-buffers"]);
    assert_eq!(status(buffer), 0);
    assert_eq!(position(buffer), (0, 0));
    assert_eq!(rig.seat.samples(1).len(), 2000, "exactly its frames");
    assert!(rig.seat.closes.borrow().is_empty(), "left to drain");

    // Played again, the drained door goes and a new one starts from the top.
    assert_eq!(play(buffer, 0), DS_OK);
    assert_eq!(*rig.seat.closes.borrow(), [1]);
    assert_eq!(rig.seat.samples(2)[..4], [0, 0, 1, 1]);
}

#[test]
fn stop_closes_the_door_and_keeps_the_place_and_play_goes_on_from_it() {
    let rig = rig();
    let buffer = rig.mono(4000, ramp);
    assert_eq!(play(buffer, 0), DS_OK);

    rig.seat.at(1_000);
    assert_eq!(stop(buffer), DS_OK);
    assert_eq!(*rig.seat.closes.borrow(), [1]);
    assert_eq!(status(buffer), 0);
    assert_eq!(position(buffer), (20, 20));
    let pushes = rig.pushes();
    rig.seat.at(2_000);
    rig.pump();
    assert_eq!(rig.pushes(), pushes, "stopped: no push");

    // The clock moved on and the place did not: frame 10, as 1,000 microseconds said.
    assert_eq!(play(buffer, 0), DS_OK);
    assert_eq!(
        rig.seat.samples(2),
        doubled((10..10 + LEAD_FRAMES).map(ramp))
    );

    // Moved while still, it goes on from there.
    assert_eq!(stop(buffer), DS_OK);
    assert_eq!(set_position(buffer, 200), DS_OK);
    assert_eq!(position(buffer), (200, 200));
    assert_eq!(play(buffer, 0), DS_OK);
    assert_eq!(
        rig.seat.samples(3),
        doubled((100..100 + LEAD_FRAMES).map(ramp))
    );

    // Moved while playing, a new door from there, what the old one held gone.
    assert_eq!(set_position(buffer, 400), DS_OK);
    assert_eq!(*rig.seat.closes.borrow(), [1, 2, 3]);
    assert_eq!(
        rig.seat.samples(4),
        doubled((200..200 + LEAD_FRAMES).map(ramp))
    );
    assert_eq!(status(buffer), DSBSTATUS_PLAYING);

    assert_eq!(set_position(buffer, 8000), DSERR_INVALIDPARAM);
}

/// The door has the first 464 frames at time 0. A write to 400..500 changes what
/// is still to go, 464 on, and not what it already holds.
#[test]
fn a_write_past_the_pushed_point_is_heard_and_before_it_is_not() {
    let rig = rig();
    let buffer = rig.mono(4000, |_| 0);
    assert_eq!(play(buffer, 0), DS_OK);
    let sevens: Vec<u8> = (0..100).flat_map(|_| 7i16.to_le_bytes()).collect();
    rig.write(buffer, 800, &sevens);

    rig.seat.at(1_000);
    rig.pump();
    let heard = rig.seat.samples(1);
    assert_eq!(
        heard[..LEAD_FRAMES as usize * 2],
        vec![0; LEAD_FRAMES as usize * 2]
    );
    assert_eq!(heard[LEAD_FRAMES as usize * 2..], vec![7; 20]);
    assert_eq!(position(buffer), (20, (LEAD_FRAMES + 10) * 2));
}

fn duplicate(rig: &Rig, original: Buffer) -> Buffer {
    let mut twin = null_mut();
    // SAFETY: the object and the original are live, and `twin` is this function's.
    let code = unsafe {
        (object_slots(rig.object).duplicate_sound_buffer)(rig.object, original, &raw mut twin)
    };
    assert_eq!(code, DS_OK);
    twin
}

#[test]
fn a_duplicate_shares_the_bytes_and_keeps_its_own_course() {
    let rig = rig();
    let original = rig.mono(1000, |_| 0);
    let twin = duplicate(&rig, original);
    assert_ne!(id_of(twin), id_of(original));

    assert_eq!(set_volume(twin, -2000), DS_OK);
    assert_eq!((volume(twin), volume(original)), (-2000, 0));
    assert_eq!(set_pan(twin, 300), DS_OK);
    assert_eq!((pan(twin), pan(original)), (300, 0));

    assert_eq!(play(original, DSBPLAY_LOOPING), DS_OK);
    assert_eq!(play(twin, DSBPLAY_LOOPING), DS_OK);
    assert_eq!(rig.seat.opens.borrow().len(), 2, "a door each");

    // One write, through the original's bytes, heard on both: frame 500.
    rig.write(original, 1000, &7000i16.to_le_bytes());
    rig.seat.at(10_000);
    rig.pump();
    let frame_500 = 500 * 2;
    assert_eq!(rig.seat.samples(1)[frame_500], 7000);
    // 7,000 at -20 dB on the right, which a pan above 0 leaves alone: the fixed
    // point's own numbers, 0.1 being 6,553 of 65,536.
    assert_eq!(rig.seat.samples(2)[frame_500 + 1], 699);

    // Its own place, and alive past the original's release.
    assert_eq!(stop(original), DS_OK);
    assert_eq!(position(original).0, 200);
    rig.seat.at(20_000);
    assert_eq!(position(twin).0, 400);
    assert_eq!(release(original), DS_OK);
    assert_eq!(*rig.seat.closes.borrow(), [1]);
    assert_eq!(status(twin), DSBSTATUS_PLAYING | DSBSTATUS_LOOPING);
    let pushes = rig.pushes();
    rig.seat.at(30_000);
    rig.pump();
    assert_eq!(rig.pushes(), pushes + 1, "still pushing");
}

/// A choice: what the clock passed went silently by in the mix, and a late frame
/// lags for good, so a stall of 3 s passes those over and hands on the lead alone.
/// 3,000,000 microseconds at 44,100 Hz is frame 132,300; the lead is 2,048 frames;
/// 2,048 were already with the door, so 130,252 are passed over; on a ring of
/// 44,000 frames, 132,300 is frame 300.
#[test]
fn a_stall_of_3_seconds_goes_on_from_the_clocks_frame_with_the_lead_alone() {
    let rig = rig();
    let buffer = rig.buffer(wave(44_100, 1, 16), 88_000);
    for frame in 0..44_000 {
        rig.write(buffer, frame * 2, &ramp(frame).to_le_bytes());
    }
    assert_eq!(play(buffer, DSBPLAY_LOOPING), DS_OK);
    assert_eq!(rig.seat.samples(1).len(), 2048 * 2);

    rig.seat.at(3_000_000);
    let lines = debug_lines(|| rig.pump());
    assert_eq!(
        lines,
        [
            "sound.buffers.passed.over a=1,1fccc by=sound-buffers",
            "sound.buffers.pushed a=1,204cc,800 by=sound-buffers",
        ]
    );
    let last = rig
        .seat
        .pushes
        .borrow()
        .last()
        .expect("it pushed")
        .1
        .clone();
    assert_eq!(last, doubled((0..2048).map(|step| ramp(300 + step))));
}

// Through the real mixer.

static MIX_ROW: sound_mix::table::Row = sound_mix::table::Row {
    file: "a program",
    module: "MSS32",
};

/// The mix's clock, moved by the test.
#[derive(Clone, Default)]
struct MixSeat(Arc<AtomicU64>);

impl Host for MixSeat {
    fn now_us(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }

    fn call_back(&self, _callback: u32, _handle: u32) {}

    fn read_file(&self, _name: &str) -> Option<u32> {
        None
    }

    fn free_block(&self, _block: u32) {}
}

/// An output that keeps its frames.
#[derive(Clone, Default)]
struct Kept(Arc<Mutex<Vec<i16>>>);

impl Kept {
    fn frames(&self) -> Vec<i16> {
        self.0.lock().expect("no test panics holding this").clone()
    }
}

impl Output for Kept {
    fn open(&mut self, rate_hz: u32) -> u32 {
        rate_hz
    }

    fn room(&self) -> usize {
        usize::MAX
    }

    fn write(&mut self, frames: &[i16]) {
        self.0
            .lock()
            .expect("no test panics holding this")
            .extend_from_slice(frames);
    }

    fn close(&mut self) {}
}

/// The mix runs at 44,100 Hz, so at that rate a door gives one frame a sample, and
/// 10,000 microseconds is 441 frames: of 100 + 1,000 while both play, of 1,000 once
/// the first has stopped and what its door held has gone.
#[test]
fn two_buffers_sum_through_the_real_mixer_and_stop_drops_what_the_door_held() {
    let (seat, kept) = (MixSeat::default(), Kept::default());
    let mix = Front::new(&MIX_ROW, seat.clone(), kept.clone());
    let clock = seat.clone();
    let buffers = Buffers::new(&ROW, MixBack::new(&mix, move || clock.now_us()));
    assert_eq!(buffers.direct_sound_create(0, true, 0), DS_OK);
    let made = |value: i16| {
        let id = buffers
            .create_sound_buffer(
                Some(Shape {
                    flags: 0,
                    bytes: 88_200,
                    format: Some(wave(44_100, 1, 16)),
                }),
                true,
                0,
            )
            .expect("a good shape");
        buffers
            .with_bytes(id, |cells| {
                for (at, cell) in cells.iter().enumerate() {
                    cell.set(value.to_le_bytes()[at % 2]);
                }
            })
            .expect("it is live");
        id
    };
    let (first, second) = (made(100), made(1000));
    assert_eq!(buffers.play(first, 0), DS_OK);
    assert_eq!(buffers.play(second, 0), DS_OK);

    seat.0.store(10_000, Ordering::Relaxed);
    buffers.pump();
    mix.pump();
    assert_eq!(kept.frames(), vec![1100; 882]);

    assert_eq!(buffers.stop(first), DS_OK);
    seat.0.store(20_000, Ordering::Relaxed);
    buffers.pump();
    mix.pump();
    let frames = kept.frames();
    assert_eq!(frames.len(), 1764);
    assert_eq!(frames[882..], vec![1000; 882]);
}

// Volume and pan.

#[test]
fn volume_and_pan_read_back_and_keep_their_value_when_refused() {
    let rig = rig();
    let buffer = rig.mono(8, ramp);
    assert_eq!((volume(buffer), pan(buffer)), (0, 0));

    assert_eq!(set_volume(buffer, -1234), DS_OK);
    assert_eq!(volume(buffer), -1234);
    assert_eq!(set_volume(buffer, 1), DSERR_INVALIDPARAM);
    assert_eq!(set_volume(buffer, -10_001), DSERR_INVALIDPARAM);
    assert_eq!(volume(buffer), -1234);

    assert_eq!(set_pan(buffer, 4321), DS_OK);
    assert_eq!(pan(buffer), 4321);
    assert_eq!(set_pan(buffer, 10_001), DSERR_INVALIDPARAM);
    assert_eq!(set_pan(buffer, -10_001), DSERR_INVALIDPARAM);
    assert_eq!(pan(buffer), 4321);

    // SAFETY: as `play`; a null out is what these calls test.
    let (no_volume, no_pan, no_status) = unsafe {
        (
            (slots(buffer).get_volume)(buffer, null_mut()),
            (slots(buffer).get_pan)(buffer, null_mut()),
            (slots(buffer).get_status)(buffer, null_mut()),
        )
    };
    assert_eq!(
        (no_volume, no_pan, no_status),
        (DSERR_INVALIDPARAM, DSERR_INVALIDPARAM, DSERR_INVALIDPARAM)
    );
}

/// Every level. The tolerance is the fixed point's own: a gain is cut to
/// [`SAMPLE_FRACTION`] bits, one step, after up to 14 table entries, each rounded
/// and each product cut at 30 bits.
#[test]
fn the_gain_is_ten_to_the_level_over_2000() {
    let unit = f64::from(1u32 << SAMPLE_FRACTION);
    let inner = 28.0 / f64::from(1u32 << 30);
    let tolerance = 1.0 / unit + inner;
    for level in -9_999..=0 {
        let got = f64::from(u32::try_from(gain(level)).expect("a gain is never negative")) / unit;
        let wanted = 10f64.powf(f64::from(level) / 2000.0);
        assert!(
            (got - wanted).abs() <= tolerance,
            "{level}: {got} against {wanted}"
        );
    }
    assert_eq!(gain(-10_000), 0);
    assert_eq!(gain(-20_000), 0);
    assert_eq!(gain(1), 1 << SAMPLE_FRACTION);
}

/// 10,000 at -20 dB: 0.1 is 6,553 of 65,536 in the fixed point, giving 999.
#[test]
fn volume_scales_both_sides_and_pan_one_side_alone() {
    let rig = rig();
    let heard_with = |gain_of: &dyn Fn(Buffer)| {
        let buffer = rig.mono(1, |_| 10_000);
        gain_of(buffer);
        let door = rig.seat.opens.borrow().len() as u32 + 1;
        assert_eq!(play(buffer, 0), DS_OK);
        rig.seat.samples(door)
    };
    assert_eq!(heard_with(&|_| {}), [10_000, 10_000]);
    assert_eq!(
        heard_with(&|buffer| {
            set_volume(buffer, -2000);
        }),
        [999, 999]
    );
    assert_eq!(
        heard_with(&|buffer| {
            set_pan(buffer, 2000);
        }),
        [999, 10_000],
        "panned right, the left drops"
    );
    assert_eq!(
        heard_with(&|buffer| {
            set_pan(buffer, -2000);
        }),
        [10_000, 999],
        "panned left, the right drops"
    );
    assert_eq!(
        heard_with(&|buffer| {
            set_volume(buffer, -2000);
            set_pan(buffer, 2000);
        }),
        [99, 999],
        "the two add: -40 dB on the left is 655 of 65,536"
    );
}

#[test]
fn at_the_floor_a_full_scale_sample_is_zero() {
    let rig = rig();
    let buffer = rig.buffer(wave(RATE, 2, 16), 4);
    let bytes: Vec<u8> = [32_767i16, -32_768]
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect();
    rig.write(buffer, 0, &bytes);
    assert_eq!(set_volume(buffer, -10_000), DS_OK);
    assert_eq!(play(buffer, 0), DS_OK);
    assert_eq!(rig.seat.samples(1), [0, 0]);

    assert_eq!(stop(buffer), DS_OK);
    assert_eq!(set_volume(buffer, 0), DS_OK);
    assert_eq!(set_pan(buffer, 10_000), DS_OK);
    assert_eq!(play(buffer, 0), DS_OK);
    assert_eq!(rig.seat.samples(2), [0, -32_768], "panned hard right");

    assert_eq!(stop(buffer), DS_OK);
    assert_eq!(set_pan(buffer, -10_000), DS_OK);
    assert_eq!(play(buffer, 0), DS_OK);
    assert_eq!(rig.seat.samples(3), [32_767, 0], "panned hard left");
}

// The object.

#[test]
fn direct_sound_create_refuses_a_null_out_and_an_outer() {
    let rig = rig();
    let mut object = null_mut();
    // SAFETY: `object` is this test's, and null is what the first call tests.
    let (no_out, outer) = unsafe {
        (
            rig.com.direct_sound_create(0, null_mut(), 0),
            rig.com.direct_sound_create(0, &raw mut object, 1),
        )
    };
    assert_eq!((no_out, outer), (DSERR_INVALIDPARAM, DSERR_NOAGGREGATION));
    assert!(object.is_null(), "a refusal writes no object");
}

#[test]
fn set_cooperative_level_takes_a_rows_level_alone() {
    let rig = rig();
    let call = object_slots(rig.object).set_cooperative_level;
    // SAFETY: the object is live.
    let (good, other) = unsafe { (call(rig.object, 0, 2), call(rig.object, 0, 3)) };
    assert_eq!((good, other), (DS_OK, DSERR_INVALIDPARAM));
}

#[test]
fn create_sound_buffer_judges_what_it_is_asked() {
    let rig = rig();
    let good = wave(RATE, 1, 16);

    let (code, made) = rig.create(DSBCAPS_PRIMARYBUFFER, 0, null());
    assert_eq!(code, DS_OK);
    assert!(!made.is_null());
    assert_eq!(
        rig.create(DSBCAPS_PRIMARYBUFFER, 2, null()).0,
        DSERR_INVALIDPARAM,
        "a primary holds no bytes of its own"
    );
    assert_eq!(
        rig.create(DSBCAPS_PRIMARYBUFFER, 0, &raw const good).0,
        DSERR_INVALIDPARAM,
        "a primary is told its format later"
    );

    for bad in [
        WaveFormat { tag: 2, ..good },
        WaveFormat { rate_hz: 0, ..good },
        WaveFormat {
            channels: 0,
            ..good
        },
        WaveFormat {
            channels: 3,
            ..good
        },
        WaveFormat { bits: 12, ..good },
        WaveFormat {
            block_align: 3,
            ..good
        },
    ] {
        let (code, made) = rig.create(0, 16, &raw const bad);
        assert_eq!(code, DSERR_BADFORMAT, "{bad:?}");
        assert!(made.is_null(), "a refusal writes no buffer");
    }

    assert_eq!(rig.create(0, 0, &raw const good).0, DSERR_INVALIDPARAM);
    assert_eq!(
        rig.create(0, 3, &raw const good).0,
        DSERR_INVALIDPARAM,
        "half a frame"
    );
    assert_eq!(rig.create(0, 16, null()).0, DSERR_INVALIDPARAM, "no format");

    let asked = desc(0, 16, &raw const good);
    let mut out = null_mut();
    let call = object_slots(rig.object).create_sound_buffer;
    // SAFETY: the object is live, and the rest are this test's or the null tested.
    let (no_desc, no_out, outer) = unsafe {
        (
            call(rig.object, null(), &raw mut out, 0),
            call(rig.object, &raw const asked, null_mut(), 0),
            call(rig.object, &raw const asked, &raw mut out, 1),
        )
    };
    assert_eq!(
        (no_desc, no_out, outer),
        (DSERR_INVALIDPARAM, DSERR_INVALIDPARAM, DSERR_NOAGGREGATION)
    );
    assert!(out.is_null());
}

/// The format read back, by a caller with `room`.
fn format_of(buffer: Buffer, room: u32) -> (u32, WaveFormat, u32) {
    let mut out = wave(1, 1, 8);
    let mut written = UNWRITTEN;
    // SAFETY: as `play`, and both are this function's.
    let code = unsafe { (slots(buffer).get_format)(buffer, &raw mut out, room, &raw mut written) };
    (code, out, written)
}

#[test]
fn set_format_is_a_primarys_alone_and_get_format_reads_it() {
    let rig = rig();
    let primary = rig.primary();
    assert_eq!(
        format_of(primary, WAVE_BYTES),
        (DS_OK, wave(22_050, 2, 8), WAVE_BYTES)
    );

    let asked = wave(44_100, 2, 16);
    // SAFETY: as `play`, and `asked` is this test's.
    let good = unsafe { (slots(primary).set_format)(primary, &raw const asked) };
    assert_eq!(good, DS_OK);
    assert_eq!(format_of(primary, WAVE_BYTES).1, asked);

    let secondary = rig.mono(8, ramp);
    // SAFETY: as above.
    let refused = unsafe { (slots(secondary).set_format)(secondary, &raw const asked) };
    assert_eq!(refused, DSERR_INVALIDCALL);
    assert_eq!(format_of(secondary, WAVE_BYTES).1, wave(RATE, 1, 16));

    // SAFETY: as `play`; a null format is what this call tests.
    let no_format = unsafe { (slots(primary).set_format)(primary, null()) };
    assert_eq!(no_format, DSERR_INVALIDPARAM);
}

#[test]
fn get_format_asks_room_only_of_a_caller_wanting_the_format() {
    let rig = rig();
    let buffer = rig.mono(8, ramp);
    let call = slots(buffer).get_format;
    let mut written = UNWRITTEN;
    // SAFETY: as `play`, and the word is this test's: the null is what's tested.
    let size_query = unsafe { call(buffer, null_mut(), 0, &raw mut written) };
    assert_eq!((size_query, written), (DS_OK, WAVE_BYTES));

    let kept = wave(1, 1, 8);
    let (code, out, untouched) = format_of(buffer, WAVE_BYTES - 1);
    assert_eq!(
        (code, out, untouched),
        (DSERR_INVALIDPARAM, kept, UNWRITTEN)
    );

    // SAFETY: as above.
    let neither = unsafe { call(buffer, null_mut(), 0, null_mut()) };
    assert_eq!(neither, DSERR_INVALIDPARAM);
}

#[test]
fn get_current_position_takes_either_out_null() {
    let rig = rig();
    let buffer = rig.mono(4000, ramp);
    assert_eq!(play(buffer, 0), DS_OK);
    rig.seat.at(1_000);
    let call = slots(buffer).get_current_position;
    let (mut play_byte, mut write_byte) = (UNWRITTEN, UNWRITTEN);
    // SAFETY: as `play`; each is this test's word or the null tested.
    let (first, second) = unsafe {
        (
            call(buffer, &raw mut play_byte, null_mut()),
            call(buffer, null_mut(), &raw mut write_byte),
        )
    };
    assert_eq!((first, second), (DS_OK, DS_OK));
    assert_eq!((play_byte, write_byte), (20, (LEAD_FRAMES + 10) * 2));
}

#[test]
fn set_frequency_goes_on_from_the_clocks_frame_by_a_new_door() {
    let rig = rig();
    let buffer = rig.mono(4000, ramp);
    assert_eq!(set_frequency(buffer, 99), DSERR_INVALIDPARAM);
    assert_eq!(set_frequency(buffer, 100_001), DSERR_INVALIDPARAM);
    assert_eq!(play(buffer, 0), DS_OK);

    // At 1,000 microseconds it stands at frame 10, and at 20,000 Hz a frame is 50
    // microseconds, so the lead is 928 frames.
    rig.seat.at(1_000);
    assert_eq!(set_frequency(buffer, 20_000), DS_OK);
    assert_eq!(*rig.seat.opens.borrow(), [(RATE, 2), (20_000, 2)]);
    assert_eq!(*rig.seat.closes.borrow(), [1]);
    assert_eq!(rig.seat.samples(2), doubled((10..10 + 928).map(ramp)));

    // 0 is the format's own.
    assert_eq!(set_frequency(buffer, 0), DS_OK);
    assert_eq!(rig.seat.opens.borrow()[2], (RATE, 2));
    assert_eq!(rig.seat.samples(3)[..2], [10, 10]);

    let primary = rig.primary();
    assert_eq!(set_frequency(primary, RATE), DSERR_INVALIDCALL);
}

#[test]
fn play_while_playing_changes_its_looping_alone() {
    let rig = rig();
    let buffer = rig.mono(1000, ramp);
    assert_eq!(play(buffer, DSBPLAY_LOOPING), DS_OK);
    assert_eq!(play(buffer, 0), DS_OK);
    assert_eq!(rig.seat.opens.borrow().len(), 1, "no new door");
    assert_eq!(status(buffer), DSBSTATUS_PLAYING);
    // Told to end, it ends at the ring's end, pumped often enough to be no stall.
    for now_us in [33_300, 66_600, 99_900, 100_000] {
        rig.seat.at(now_us);
        rig.pump();
    }
    assert_eq!(status(buffer), 0);
    assert_eq!(rig.seat.samples(1), doubled((0..1000).map(ramp)));
}

#[test]
fn a_primary_says_it_plays_and_moves_no_samples() {
    let rig = rig();
    let primary = rig.primary();
    assert_eq!(play(primary, DSBPLAY_LOOPING), DS_OK);
    assert_eq!(status(primary), DSBSTATUS_PLAYING | DSBSTATUS_LOOPING);
    assert_eq!(position(primary), (0, 0));
    assert_eq!(rig.seat.opens.borrow().len(), 0);
    assert_eq!(set_position(primary, 0), DSERR_INVALIDCALL);
    assert_eq!(stop(primary), DS_OK);
    assert_eq!(status(primary), 0);
}

#[test]
fn releasing_a_playing_buffer_closes_its_door_and_its_kept_table_still_answers() {
    let rig = rig();
    let buffer = rig.mono(4000, ramp);
    let other = rig.mono(8, ramp);
    assert_eq!(play(buffer, DSBPLAY_LOOPING), DS_OK);
    assert_eq!(release(buffer), DS_OK);
    assert_eq!(*rig.seat.closes.borrow(), [1]);

    assert_eq!(release(buffer), DSERR_INVALIDCALL);
    assert_eq!(play(buffer, 0), DSERR_INVALIDCALL);
    assert_eq!(stop(buffer), DSERR_INVALIDCALL);
    assert_eq!(set_volume(buffer, 0), DSERR_INVALIDCALL);
    assert_eq!(lock(buffer, 0, 1, 0), (DSERR_INVALIDCALL, [UNWRITTEN; 4]));
    let mut out = UNWRITTEN;
    // SAFETY: the kept interface is what this tests; the word is this test's.
    let (read, unlock, restore, frequency) = unsafe {
        let table = slots(buffer);
        (
            (table.get_status)(buffer, &raw mut out),
            (table.unlock)(buffer, 0, 0, 0, 0),
            (table.restore)(buffer),
            (table.set_frequency)(buffer, 0),
        )
    };
    assert_eq!(
        (read, unlock, restore, frequency),
        (
            DSERR_INVALIDCALL,
            DSERR_INVALIDCALL,
            DSERR_INVALIDCALL,
            DSERR_INVALIDCALL
        )
    );
    assert_eq!(out, UNWRITTEN);
    assert_eq!(status(other), 0, "another buffer stands");
}

#[test]
fn releasing_the_object_closes_every_door_and_its_table_still_answers() {
    let rig = rig();
    let (first, second) = (rig.mono(4000, ramp), rig.mono(4000, ramp));
    assert_eq!(play(first, DSBPLAY_LOOPING), DS_OK);
    assert_eq!(play(second, 0), DS_OK);
    let table = object_slots(rig.object);
    // SAFETY: the object is live.
    let released = unsafe { (table.release)(rig.object) };
    assert_eq!(released, DS_OK);
    assert_eq!(*rig.seat.closes.borrow(), [1, 2]);
    assert_eq!(play(first, 0), DSERR_INVALIDCALL);

    // SAFETY: the kept interface is what this tests.
    let again = unsafe { (table.release)(rig.object) };
    assert_eq!(again, DSERR_INVALIDCALL);
    assert_eq!(rig.create(0, 16, null()).0, DSERR_INVALIDCALL);
}

#[test]
fn restore_answers_0_as_nothing_here_is_lost() {
    let rig = rig();
    let buffer = rig.mono(8, ramp);
    // SAFETY: as `play`.
    assert_eq!(unsafe { (slots(buffer).restore)(buffer) }, DS_OK);
}

/// All 21 by their tables: one `imp.call` line each, in `c` the name [`FRONTS`] has.
#[test]
fn every_front_writes_one_line_under_its_name() {
    let rig = rig();
    let object = rig.object;
    let primary = rig.primary();
    let buffer = rig.mono(8, ramp);
    let lines = debug_lines(|| {
        let format = wave(RATE, 2, 16);
        let asked = desc(0, 16, &raw const format);
        let mut out = null_mut();
        let (mut word, mut other, mut signed) = (0u32, 0u32, 0i32);
        let mut read = wave(1, 1, 8);
        let (o, b) = (object_slots(object), slots(buffer));
        // SAFETY: each interface is live and each pointer this closure's own.
        unsafe {
            rig.com.direct_sound_create(0, &raw mut out, 0);
            (o.create_sound_buffer)(object, &raw const asked, &raw mut out, 0);
            (o.duplicate_sound_buffer)(object, buffer, &raw mut out);
            (o.set_cooperative_level)(object, 0, 2);
            (b.get_current_position)(buffer, &raw mut word, &raw mut other);
            (b.get_format)(buffer, &raw mut read, WAVE_BYTES, &raw mut word);
            (b.get_volume)(buffer, &raw mut signed);
            (b.get_pan)(buffer, &raw mut signed);
            (b.get_status)(buffer, &raw mut word);
            (b.lock)(
                buffer,
                0,
                2,
                &raw mut word,
                &raw mut other,
                null_mut(),
                null_mut(),
                0,
            );
            (b.unlock)(buffer, 0, 2, 0, 0);
            (b.play)(buffer, 0, 0, 0);
            (b.set_current_position)(buffer, 0);
            (b.set_format)(primary, &raw const format);
            (b.set_volume)(buffer, 0);
            (b.set_pan)(buffer, 0);
            (b.set_frequency)(buffer, 0);
            (b.stop)(buffer);
            (b.restore)(buffer);
            (b.release)(buffer);
            (o.release)(object);
        }
    });
    let mut names: Vec<&str> = lines
        .iter()
        .filter(|line| line.starts_with("imp.call "))
        .map(|line| {
            line["imp.call c=".len()..]
                .split(' ')
                .next()
                .expect("a line has a name")
        })
        .collect();
    names.sort_unstable();
    let mut wanted = FRONTS.to_vec();
    wanted.sort_unstable();
    assert_eq!(names, wanted);
    for line in &lines {
        assert!(line.ends_with(" by=sound-buffers"), "{line}");
    }
}
