// SPDX-License-Identifier: MIT
//! The portable part, in a file's 32-bit numbers: the object, its buffers, and the
//! motion of a playing buffer by a clock. No thread: each front and [`Buffers::pump`]
//! hand a door the frames due by the clock plus [`LEAD_US`], so a mix that renders
//! between two calls finds them there.

use alloc::collections::BTreeMap;
use alloc::rc::Rc;
use alloc::vec::Vec;
use core::cell::{Cell, RefCell};
use core::fmt;

use oxide_trace::Args;

use crate::sdk::{
    DS_OK, DSBCAPS_PRIMARYBUFFER, DSBFREQUENCY_MAX, DSBFREQUENCY_MIN, DSBFREQUENCY_ORIGINAL,
    DSBLOCK_ENTIREBUFFER, DSBLOCK_FROMWRITECURSOR, DSBPAN_LEFT, DSBPAN_RIGHT, DSBPLAY_LOOPING,
    DSBSIZE_MAX, DSBSTATUS_LOOPING, DSBSTATUS_PLAYING, DSBVOLUME_MAX, DSBVOLUME_MIN,
    DSERR_BADFORMAT, DSERR_INVALIDCALL, DSERR_INVALIDPARAM, DSERR_NOAGGREGATION, DSERR_UNSUPPORTED,
    WAVE_FORMAT_PCM, WaveFormat,
};
use crate::table::Row;

/// How far ahead of the clock a playing buffer's frames reach a door: 2 of the
/// mixer's chunks of 1,024 frames at 44,100 Hz, which covers a program that pumps
/// at 30 frames a second. A door goes silent on an empty queue and never skips, so
/// this is what keeps a frame from arriving late.
pub const LEAD_US: u64 = 46_440;

/// The bytes of a `WAVEFORMATEX`, a `GetFormat` caller's least room.
pub(crate) const WAVE_BYTES: u32 = 18;
/// A primary's format until a program sets one: `DirectSound`'s own start.
const PRIMARY_RATE_HZ: u32 = 22_050;
const PRIMARY_CHANNELS: u16 = 2;
const PRIMARY_BITS: u16 = 8;
/// A door is always stereo: mono is doubled and pan applied here.
const DOOR_CHANNELS: u32 = 2;
const MICROSECONDS: u128 = 1_000_000;

const OBJECT: &str = "IDirectSound";
const BUFFER: &str = "IDirectSoundBuffer";

/// A line's numbers, in order.
macro_rules! numbers {
    ($($value:expr),+) => {
        Args([$(oxide_trace::__shown!($value)),+])
    };
}

/// A method's name as a line writes it.
struct Called<'a>(&'a str, &'a str);

impl fmt::Display for Called<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}::{}", self.0, self.1)
    }
}

/// What a target does for the stand-in: the clock and a mixer's three door calls.
pub trait Back {
    /// The clock in microseconds, the mix's own; it must never go back.
    fn now_us(&self) -> u64;
    /// Opens a door of `channels` at `rate_hz`.
    fn stream_open(&self, rate_hz: u32, channels: u32) -> u32;
    /// Queues interleaved samples on a door.
    fn stream_push(&self, door: u32, samples: &[i16]);
    /// Ends a door, what it still held unheard.
    fn stream_close(&self, door: u32);
    /// Writes one line around whatever the target must keep across it.
    fn quiet(&self, line: impl FnOnce()) {
        line();
    }
}

/// What a program asks of a new buffer.
#[derive(Clone, Copy, Debug)]
pub struct Shape {
    pub flags: u32,
    pub bytes: u32,
    pub format: Option<WaveFormat>,
}

/// What `Lock` answers: the ring's first address and the two spans in it, the
/// second always starting at the first address.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spans {
    pub base: u32,
    pub first_at: u32,
    pub first_bytes: u32,
    pub second_bytes: u32,
}

#[derive(Clone, Copy, Debug)]
struct Format {
    rate_hz: u32,
    channels: u16,
    bits: u16,
}

impl Format {
    const fn block(self) -> u32 {
        self.channels as u32 * self.bits as u32 / 8
    }

    /// The format `wave` asks, if it is one this stand-in plays.
    fn read(wave: &WaveFormat) -> Option<Self> {
        let (tag, rate_hz, channels, bits, block_align) = (
            wave.tag,
            wave.rate_hz,
            wave.channels,
            wave.bits,
            wave.block_align,
        );
        let format = Self {
            rate_hz,
            channels,
            bits,
        };
        let good = tag == WAVE_FORMAT_PCM
            && rate_hz != 0
            && matches!(channels, 1 | 2)
            && matches!(bits, 8 | 16)
            && u32::from(block_align) == format.block();
        good.then_some(format)
    }

    #[expect(
        clippy::cast_possible_truncation,
        reason = "a block is at most 2 channels of 2 bytes"
    )]
    const fn wave(self) -> WaveFormat {
        WaveFormat {
            tag: WAVE_FORMAT_PCM,
            channels: self.channels,
            rate_hz: self.rate_hz,
            bytes_per_second: self.rate_hz * self.block(),
            block_align: self.block() as u16,
            bits: self.bits,
            extra: 0,
        }
    }
}

/// A playing buffer's course, reckoned from where and when it began.
#[derive(Clone, Copy, Debug)]
struct Play {
    door: u32,
    from_frame: u64,
    start_us: u64,
    /// Frames handed to the door since `start_us`, or passed over as late.
    pushed: u64,
    /// The frames since `start_us` at which it ends; `None` while it loops.
    limit: Option<u64>,
}

#[derive(Clone, Copy, Debug)]
enum Motion {
    Still,
    Playing(Play),
    /// A natural end: every frame is with the door, which keeps sounding them.
    Draining(u32),
    /// A primary told to play: it moves no samples, the mix being its own.
    Primary {
        looping: bool,
    },
}

struct Buffer {
    primary: bool,
    format: Format,
    rate_hz: u32,
    memory: Rc<[Cell<u8>]>,
    /// Where it stands while not playing.
    stop_frame: u64,
    volume: i32,
    pan: i32,
    motion: Motion,
}

/// The frames of `us` at `rate_hz`.
#[expect(
    clippy::cast_possible_truncation,
    reason = "a clock's microseconds at a rate this size fit 64 bits for centuries"
)]
fn frames_in(us: u64, rate_hz: u32) -> u64 {
    (u128::from(us) * u128::from(rate_hz) / MICROSECONDS) as u64
}

/// `10^(-2^bit / 2000)` in 30 fraction bits, one for each bit of a level in
/// hundredths of a decibel, so a gain needs no floating point.
const GAIN_STEPS: [u64; 14] = [
    0x3fed_25e8,
    0x3fda_515e,
    0x3fb4_b8ec,
    0x3f69_ca64,
    0x3ed4_f553,
    0x3daf_5fee,
    0x3b74_2f6f,
    0x373b_0a60,
    0x2fa9_b04c,
    0x237e_fcd9,
    0x13af_e084,
    0x060e_50a2,
    0x0092_b252,
    0x0001_5040,
];
const GAIN_FRACTION: u32 = 30;
/// A gain's fraction bits as a sample is scaled by it.
pub(crate) const SAMPLE_FRACTION: u32 = 16;

/// A level in hundredths of a decibel as a gain in [`SAMPLE_FRACTION`] bits:
/// `10^(level / 2000)`, 1 at 0 or above and 0 at the floor or below.
pub(crate) fn gain(level: i32) -> i64 {
    if level >= DSBVOLUME_MAX {
        return 1 << SAMPLE_FRACTION;
    }
    if level <= DSBVOLUME_MIN {
        return 0;
    }
    let down = level.unsigned_abs();
    let mut kept = 1u64 << GAIN_FRACTION;
    for (bit, step) in GAIN_STEPS.iter().enumerate() {
        if down >> bit & 1 == 1 {
            kept = (kept * step) >> GAIN_FRACTION;
        }
    }
    i64::from(u32::try_from(kept >> (GAIN_FRACTION - SAMPLE_FRACTION)).unwrap_or(0))
}

/// `sample` scaled by `by`, toward 0, so a floor gain leaves exactly 0.
#[expect(
    clippy::cast_possible_truncation,
    reason = "a gain is at most 1, so the answer stays a sample's size"
)]
fn scaled(sample: i16, by: i64) -> i16 {
    (i64::from(sample) * by / (1 << SAMPLE_FRACTION)) as i16
}

fn sample(memory: &[Cell<u8>], at: usize, bits: u16) -> i16 {
    if bits == 8 {
        (i16::from(memory[at].get()) - 128) << 8
    } else {
        i16::from_le_bytes([memory[at].get(), memory[at + 1].get()])
    }
}

impl Buffer {
    fn new(primary: bool, format: Format, memory: Rc<[Cell<u8>]>) -> Self {
        Self {
            primary,
            format,
            rate_hz: format.rate_hz,
            memory,
            stop_frame: 0,
            volume: DSBVOLUME_MAX,
            pan: 0,
            motion: Motion::Still,
        }
    }

    fn frames(&self) -> u64 {
        self.memory.len() as u64 / u64::from(self.format.block().max(1))
    }

    /// The frame the clock has reached: where a listener is, the lead apart.
    fn heard_frame(&self, now_us: u64) -> u64 {
        match self.motion {
            Motion::Playing(play) => {
                let heard = frames_in(now_us.saturating_sub(play.start_us), self.rate_hz);
                (play.from_frame + heard) % self.frames()
            }
            _ => self.stop_frame,
        }
    }

    /// `count` frames from `first`, the ring wrapped, as a door's stereo samples.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "both are bounded by a buffer's bytes, which fit a usize"
    )]
    fn rendered(&self, first: u64, count: u64) -> Vec<i16> {
        let frames = self.frames();
        let block = u64::from(self.format.block());
        let width = usize::from(self.format.bits / 8);
        let bits = self.format.bits;
        let left_gain = gain(self.volume - self.pan.max(0));
        let right_gain = gain(self.volume + self.pan.min(0));
        let mut out = Vec::with_capacity(count as usize * 2);
        for step in 0..count {
            let at = ((first + step) % frames * block) as usize;
            let left = sample(&self.memory, at, bits);
            let right = if self.format.channels == 2 {
                sample(&self.memory, at + width, bits)
            } else {
                left
            };
            out.push(scaled(left, left_gain));
            out.push(scaled(right, right_gain));
        }
        out
    }

    /// Hands the door what the clock has made due. Frames the clock has already
    /// passed went silently by in the mix, so they are passed over and not queued:
    /// a late frame would lag for good, and a long stall would cost memory.
    fn advance(&mut self, id: u32, now_us: u64, back: &impl Back) {
        let Motion::Playing(mut play) = self.motion else {
            return;
        };
        let elapsed = now_us.saturating_sub(play.start_us);
        let bound = play.limit.unwrap_or(u64::MAX);
        let heard = frames_in(elapsed, self.rate_hz);
        let due = frames_in(elapsed + LEAD_US, self.rate_hz).min(bound);
        let first = play.pushed.max(heard.min(bound));
        let passed_over = first - play.pushed;
        let count = due.saturating_sub(first);
        if passed_over > 0 {
            back.quiet(|| {
                log::debug!(
                    a:% = numbers!(id, passed_over),
                    by = "sound-buffers"; "sound.buffers.passed.over"
                );
            });
        }
        if count > 0 {
            let samples = self.rendered(play.from_frame + first, count);
            back.stream_push(play.door, &samples);
            back.quiet(|| {
                log::debug!(
                    a:% = numbers!(id, first, count),
                    by = "sound-buffers"; "sound.buffers.pushed"
                );
            });
        }
        play.pushed = first + count;
        if heard >= bound {
            self.motion = Motion::Draining(play.door);
            self.stop_frame = 0;
            back.quiet(|| {
                log::debug!(a:% = numbers!(id), by = "sound-buffers"; "sound.buffers.ended");
            });
        } else {
            self.motion = Motion::Playing(play);
        }
    }

    /// Opens a door and sets out from `from_frame`, the lead handed over at once.
    fn set_out(&mut self, id: u32, from_frame: u64, looping: bool, now_us: u64, back: &impl Back) {
        let door = back.stream_open(self.rate_hz, DOOR_CHANNELS);
        self.motion = Motion::Playing(Play {
            door,
            from_frame,
            start_us: now_us,
            pushed: 0,
            limit: (!looping).then(|| self.frames() - from_frame),
        });
        self.advance(id, now_us, back);
    }

    /// Closes the door if there is one and stands still; the place is the caller's.
    fn halt(&mut self, back: &impl Back) {
        if let Motion::Playing(Play { door, .. }) | Motion::Draining(door) = self.motion {
            back.stream_close(door);
        }
        self.motion = Motion::Still;
    }

    /// Goes on from `frame`: playing, by a new door; still, by standing there.
    fn move_to(&mut self, id: u32, frame: u64, now_us: u64, back: &impl Back) {
        if let Motion::Playing(play) = self.motion {
            let looping = play.limit.is_none();
            self.halt(back);
            self.set_out(id, frame, looping, now_us, back);
        } else {
            self.stop_frame = frame;
        }
    }
}

#[derive(Default)]
struct State {
    object_live: bool,
    made: u32,
    buffers: BTreeMap<u32, Buffer>,
}

impl State {
    fn advance(&mut self, now_us: u64, back: &impl Back) {
        for (id, buffer) in &mut self.buffers {
            buffer.advance(*id, now_us, back);
        }
    }

    fn adopt(&mut self, buffer: Buffer) -> u32 {
        self.made += 1;
        self.buffers.insert(self.made, buffer);
        self.made
    }
}

/// One program's `DirectSound`: the object and its buffers. One thread's calls, as
/// `draw-surfaces`: it is not `Sync`.
pub struct Buffers<B> {
    row: &'static Row,
    back: B,
    state: RefCell<State>,
}

impl<B: Back> Buffers<B> {
    #[must_use]
    pub fn new(row: &'static Row, back: B) -> Self {
        Self {
            row,
            back,
            state: RefCell::new(State::default()),
        }
    }

    #[must_use]
    pub const fn back(&self) -> &B {
        &self.back
    }

    /// Every front's shape: the clock read, the buffers moved, the call, its line.
    fn front<R>(&self, call: impl FnOnce(&mut State, u64, &B) -> R, line: impl FnOnce(&R)) -> R {
        let now_us = self.back.now_us();
        let answer = {
            let mut state = self.state.borrow_mut();
            state.advance(now_us, &self.back);
            call(&mut state, now_us, &self.back)
        };
        self.back.quiet(|| line(&answer));
        answer
    }

    /// A buffer's call that answers a code alone; a buffer gone answers
    /// `DSERR_INVALIDCALL`, never a crash.
    fn on_buffer(
        &self,
        name: &str,
        id: u32,
        value: i64,
        call: impl FnOnce(&mut Buffer, u64, &B) -> u32,
    ) -> u32 {
        self.front(
            |state, now_us, back| {
                state
                    .buffers
                    .get_mut(&id)
                    .map_or(DSERR_INVALIDCALL, |buffer| call(buffer, now_us, back))
            },
            |answer| {
                let id = i64::from(id);
                log::debug!(
                    c:% = Called(BUFFER, name),
                    a:% = numbers!(id, value),
                    v:% = oxide_trace::__shown!(*answer),
                    by = "sound-buffers"; "imp.call"
                );
            },
        )
    }

    /// A buffer's call that reads one number back.
    fn read_buffer(
        &self,
        name: &str,
        id: u32,
        wanted: bool,
        read: impl FnOnce(&Buffer) -> i32,
    ) -> Result<i32, u32> {
        self.front(
            |state, _, _| {
                if !wanted {
                    return Err(DSERR_INVALIDPARAM);
                }
                state.buffers.get(&id).map(read).ok_or(DSERR_INVALIDCALL)
            },
            |answer| {
                let (code, value) = (answer.err().unwrap_or(DS_OK), answer.unwrap_or(0));
                log::debug!(
                    c:% = Called(BUFFER, name),
                    a:% = numbers!(id),
                    v:% = oxide_trace::__shown!(code),
                    o:% = oxide_trace::__shown!(value),
                    by = "sound-buffers"; "imp.call"
                );
            },
        )
    }

    /// Moves the playing buffers by the clock, for a program between its calls.
    pub fn pump(&self) {
        let now_us = self.back.now_us();
        self.state.borrow_mut().advance(now_us, &self.back);
    }

    /// The call's line, then the end of the program: `name` is one of
    /// [`crate::table::STOPS`], for which no answer here would be true.
    pub fn named_stop(&self, name: &str) -> ! {
        log::error!(
            c = name, file = self.row.file, by = "sound-buffers"; "sound.buffers.named.stop"
        );
        log::logger().flush();
        panic!("sound-buffers: {name}: this call is not answered");
    }

    /// The answer of a slot with no front.
    pub fn absent(&self, interface: &str, slot: u32) -> u32 {
        let i = oxide_trace::__shown!(slot);
        self.back.quiet(|| {
            log::debug!(c = interface, i:% = i, by = "sound-buffers"; "sound.buffers.absent");
        });
        DSERR_UNSUPPORTED
    }

    /// `DirectSoundCreate`: `guid` is shown and not read.
    pub fn direct_sound_create(&self, guid: u32, has_out: bool, outer: u32) -> u32 {
        self.front(
            |state, _, _| {
                if !has_out {
                    DSERR_INVALIDPARAM
                } else if outer != 0 {
                    DSERR_NOAGGREGATION
                } else {
                    state.object_live = true;
                    DS_OK
                }
            },
            |answer| {
                log::debug!(
                    c = "DSOUND!DirectSoundCreate",
                    a:% = numbers!(guid, outer),
                    v:% = oxide_trace::__shown!(*answer),
                    by = "sound-buffers"; "imp.call"
                );
            },
        )
    }

    /// `IDirectSound::Release`: every door closes and every buffer goes.
    pub fn object_release(&self) -> u32 {
        self.front(
            |state, _, back| {
                if !state.object_live {
                    return DSERR_INVALIDCALL;
                }
                for buffer in state.buffers.values_mut() {
                    buffer.halt(back);
                }
                state.buffers.clear();
                state.object_live = false;
                DS_OK
            },
            |answer| {
                log::debug!(
                    c:% = Called(OBJECT, "Release"),
                    v:% = oxide_trace::__shown!(*answer),
                    by = "sound-buffers"; "imp.call"
                );
            },
        )
    }

    /// `IDirectSound::SetCooperativeLevel`: a level the row names, or refused.
    pub fn set_cooperative_level(&self, window: u32, level: u32) -> u32 {
        self.front(
            |state, _, _| {
                if !state.object_live {
                    DSERR_INVALIDCALL
                } else if self.row.levels.contains(&level) {
                    DS_OK
                } else {
                    DSERR_INVALIDPARAM
                }
            },
            |answer| {
                log::debug!(
                    c:% = Called(OBJECT, "SetCooperativeLevel"),
                    a:% = numbers!(window, level),
                    v:% = oxide_trace::__shown!(*answer),
                    by = "sound-buffers"; "imp.call"
                );
            },
        )
    }

    /// The new buffer `shape` asks, by the rules of `dsound.h`'s documentation.
    fn made_from(shape: Shape) -> Result<Buffer, u32> {
        if shape.flags & DSBCAPS_PRIMARYBUFFER != 0 {
            if shape.bytes != 0 || shape.format.is_some() {
                return Err(DSERR_INVALIDPARAM);
            }
            let format = Format {
                rate_hz: PRIMARY_RATE_HZ,
                channels: PRIMARY_CHANNELS,
                bits: PRIMARY_BITS,
            };
            return Ok(Buffer::new(true, format, Rc::from([])));
        }
        let wave = shape.format.as_ref().ok_or(DSERR_INVALIDPARAM)?;
        let format = Format::read(wave).ok_or(DSERR_BADFORMAT)?;
        if shape.bytes == 0 || shape.bytes > DSBSIZE_MAX || shape.bytes % format.block() != 0 {
            return Err(DSERR_INVALIDPARAM);
        }
        let memory: Rc<[Cell<u8>]> = core::iter::repeat_with(|| Cell::new(0))
            .take(shape.bytes as usize)
            .collect();
        Ok(Buffer::new(false, format, memory))
    }

    /// `IDirectSound::CreateSoundBuffer`: the new buffer's id. `shape` is `None` for
    /// a null description.
    pub fn create_sound_buffer(
        &self,
        shape: Option<Shape>,
        has_out: bool,
        outer: u32,
    ) -> Result<u32, u32> {
        self.front(
            |state, _, _| {
                if !state.object_live {
                    return Err(DSERR_INVALIDCALL);
                }
                let shape = shape.filter(|_| has_out).ok_or(DSERR_INVALIDPARAM)?;
                if outer != 0 {
                    return Err(DSERR_NOAGGREGATION);
                }
                Ok(state.adopt(Self::made_from(shape)?))
            },
            |answer| {
                let (flags, bytes) = shape.map_or((0, 0), |asked| (asked.flags, asked.bytes));
                let (code, made) = (answer.err().unwrap_or(DS_OK), answer.unwrap_or(0));
                log::debug!(
                    c:% = Called(OBJECT, "CreateSoundBuffer"),
                    a:% = numbers!(flags, bytes),
                    v:% = oxide_trace::__shown!(code),
                    o:% = oxide_trace::__shown!(made),
                    by = "sound-buffers"; "imp.call"
                );
            },
        )
    }

    /// `IDirectSound::DuplicateSoundBuffer`: it shares the original's memory and
    /// starts with its format, volume, pan and rate, still, at its first frame.
    pub fn duplicate(&self, original: Option<u32>, has_out: bool) -> Result<u32, u32> {
        self.front(
            |state, _, _| {
                if !state.object_live {
                    return Err(DSERR_INVALIDCALL);
                }
                let id = original.filter(|_| has_out).ok_or(DSERR_INVALIDPARAM)?;
                let kept = state.buffers.get(&id).ok_or(DSERR_INVALIDCALL)?;
                if kept.primary {
                    return Err(DSERR_INVALIDCALL);
                }
                let mut twin = Buffer::new(false, kept.format, Rc::clone(&kept.memory));
                twin.rate_hz = kept.rate_hz;
                twin.volume = kept.volume;
                twin.pan = kept.pan;
                Ok(state.adopt(twin))
            },
            |answer| {
                let asked = original.unwrap_or(0);
                let (code, made) = (answer.err().unwrap_or(DS_OK), answer.unwrap_or(0));
                log::debug!(
                    c:% = Called(OBJECT, "DuplicateSoundBuffer"),
                    a:% = numbers!(asked),
                    v:% = oxide_trace::__shown!(code),
                    o:% = oxide_trace::__shown!(made),
                    by = "sound-buffers"; "imp.call"
                );
            },
        )
    }

    /// `IDirectSoundBuffer::Release`: its door closes; its memory lives while a
    /// duplicate holds it.
    pub fn buffer_release(&self, id: u32) -> u32 {
        self.front(
            |state, _, back| match state.buffers.remove(&id) {
                Some(mut buffer) => {
                    buffer.halt(back);
                    DS_OK
                }
                None => DSERR_INVALIDCALL,
            },
            |answer| {
                log::debug!(
                    c:% = Called(BUFFER, "Release"),
                    a:% = numbers!(id),
                    v:% = oxide_trace::__shown!(*answer),
                    by = "sound-buffers"; "imp.call"
                );
            },
        )
    }

    /// `GetCurrentPosition`: the clock's byte and the first byte a door has not
    /// taken, which is what a writer may still change and have heard.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a frame's byte is inside a buffer, whose size fits 32 bits"
    )]
    pub fn get_current_position(&self, id: u32) -> Result<(u32, u32), u32> {
        self.front(
            |state, now_us, _| {
                let buffer = state.buffers.get(&id).ok_or(DSERR_INVALIDCALL)?;
                let block = u64::from(buffer.format.block());
                let play_byte = (buffer.heard_frame(now_us) * block) as u32;
                let write_byte = match buffer.motion {
                    Motion::Playing(play) => {
                        ((play.from_frame + play.pushed) % buffer.frames() * block) as u32
                    }
                    _ => play_byte,
                };
                Ok((play_byte, write_byte))
            },
            |answer: &Result<(u32, u32), u32>| {
                let (play_byte, write_byte) = answer.unwrap_or((0, 0));
                let code = answer.err().unwrap_or(DS_OK);
                log::debug!(
                    c:% = Called(BUFFER, "GetCurrentPosition"),
                    a:% = numbers!(id),
                    v:% = oxide_trace::__shown!(code),
                    o:% = numbers!(play_byte, write_byte),
                    by = "sound-buffers"; "imp.call"
                );
            },
        )
    }

    /// `GetFormat`: the format, for a caller with room, and always its size.
    pub fn get_format(
        &self,
        id: u32,
        has_format: bool,
        room: u32,
        has_written: bool,
    ) -> Result<WaveFormat, u32> {
        self.front(
            |state, _, _| {
                let buffer = state.buffers.get(&id).ok_or(DSERR_INVALIDCALL)?;
                if has_format && room < WAVE_BYTES || !has_format && !has_written {
                    return Err(DSERR_INVALIDPARAM);
                }
                Ok(buffer.format.wave())
            },
            |answer| {
                let code = answer.err().unwrap_or(DS_OK);
                log::debug!(
                    c:% = Called(BUFFER, "GetFormat"),
                    a:% = numbers!(id, room),
                    v:% = oxide_trace::__shown!(code),
                    o:% = oxide_trace::__shown!(WAVE_BYTES),
                    by = "sound-buffers"; "imp.call"
                );
            },
        )
    }

    pub fn get_volume(&self, id: u32, has_out: bool) -> Result<i32, u32> {
        self.read_buffer("GetVolume", id, has_out, |buffer| buffer.volume)
    }

    pub fn get_pan(&self, id: u32, has_out: bool) -> Result<i32, u32> {
        self.read_buffer("GetPan", id, has_out, |buffer| buffer.pan)
    }

    /// `GetStatus`: playing, and looping, as the clock has it. A buffer is never lost.
    pub fn get_status(&self, id: u32, has_out: bool) -> Result<u32, u32> {
        self.front(
            |state, _, _| {
                if !has_out {
                    return Err(DSERR_INVALIDPARAM);
                }
                let buffer = state.buffers.get(&id).ok_or(DSERR_INVALIDCALL)?;
                Ok(match buffer.motion {
                    Motion::Playing(Play { limit: None, .. })
                    | Motion::Primary { looping: true } => DSBSTATUS_PLAYING | DSBSTATUS_LOOPING,
                    Motion::Playing(_) | Motion::Primary { .. } => DSBSTATUS_PLAYING,
                    Motion::Still | Motion::Draining(_) => 0,
                })
            },
            |answer| {
                let (code, status) = (answer.err().unwrap_or(DS_OK), answer.unwrap_or(0));
                log::debug!(
                    c:% = Called(BUFFER, "GetStatus"),
                    a:% = numbers!(id),
                    v:% = oxide_trace::__shown!(code),
                    o:% = oxide_trace::__shown!(status),
                    by = "sound-buffers"; "imp.call"
                );
            },
        )
    }

    /// `Lock`: the spans of the ring a program may write. `can_wrap` is false for a
    /// null second pointer, which leaves the first span alone.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a 32-bit program's address fits; on a wider host the word is only a number"
    )]
    pub fn lock(
        &self,
        id: u32,
        offset: u32,
        bytes: u32,
        flags: u32,
        has_first: bool,
        can_wrap: bool,
    ) -> Result<Spans, u32> {
        self.front(
            |state, _, _| {
                let buffer = state.buffers.get(&id).ok_or(DSERR_INVALIDCALL)?;
                if buffer.primary {
                    return Err(DSERR_INVALIDCALL);
                }
                let known = DSBLOCK_ENTIREBUFFER | DSBLOCK_FROMWRITECURSOR;
                if !has_first || flags & DSBLOCK_FROMWRITECURSOR != 0 || flags & !known != 0 {
                    return Err(DSERR_INVALIDPARAM);
                }
                let size = buffer.memory.len() as u32;
                let base = buffer.memory.as_ptr() as usize as u32;
                if flags & DSBLOCK_ENTIREBUFFER != 0 {
                    return Ok(Spans {
                        base,
                        first_at: 0,
                        first_bytes: size,
                        second_bytes: 0,
                    });
                }
                if offset >= size || bytes == 0 || bytes > size {
                    return Err(DSERR_INVALIDPARAM);
                }
                let first_bytes = bytes.min(size - offset);
                Ok(Spans {
                    base,
                    first_at: offset,
                    first_bytes,
                    second_bytes: if can_wrap { bytes - first_bytes } else { 0 },
                })
            },
            |answer: &Result<Spans, u32>| {
                let spans = answer.map_or((0, 0, 0), |at| {
                    (at.first_at, at.first_bytes, at.second_bytes)
                });
                let code = answer.err().unwrap_or(DS_OK);
                log::debug!(
                    c:% = Called(BUFFER, "Lock"),
                    a:% = numbers!(id, offset, bytes, flags),
                    v:% = oxide_trace::__shown!(code),
                    o:% = numbers!(spans.0, spans.1, spans.2),
                    by = "sound-buffers"; "imp.call"
                );
            },
        )
    }

    /// `Unlock`: the bytes are already the ring's, so nothing moves.
    pub fn unlock(&self, id: u32, first_bytes: u32, second_bytes: u32) -> u32 {
        self.front(
            |state, _, _| {
                if state.buffers.contains_key(&id) {
                    DS_OK
                } else {
                    DSERR_INVALIDCALL
                }
            },
            |answer| {
                log::debug!(
                    c:% = Called(BUFFER, "Unlock"),
                    a:% = numbers!(id, first_bytes, second_bytes),
                    v:% = oxide_trace::__shown!(*answer),
                    by = "sound-buffers"; "imp.call"
                );
            },
        )
    }

    /// `Play`: a door opens and the buffer sets out from where it stands. Already
    /// playing, it goes on, only its looping changed.
    pub fn play(&self, id: u32, flags: u32) -> u32 {
        let looping = flags & DSBPLAY_LOOPING != 0;
        self.on_buffer("Play", id, i64::from(flags), |buffer, now_us, back| {
            if buffer.primary {
                buffer.motion = Motion::Primary { looping };
                return DS_OK;
            }
            let frames = buffer.frames();
            if let Motion::Playing(play) = &mut buffer.motion {
                let next = (play.from_frame + play.pushed) % frames;
                play.limit = match (looping, play.limit) {
                    (true, _) => None,
                    (false, Some(limit)) => Some(limit),
                    (false, None) => Some(play.pushed + frames - next),
                };
                return DS_OK;
            }
            buffer.halt(back);
            buffer.set_out(id, buffer.stop_frame, looping, now_us, back);
            DS_OK
        })
    }

    /// `Stop`: the door closes, what it held going with it, and the place is kept.
    pub fn stop(&self, id: u32) -> u32 {
        self.on_buffer("Stop", id, 0, |buffer, now_us, back| {
            buffer.stop_frame = buffer.heard_frame(now_us);
            buffer.halt(back);
            DS_OK
        })
    }

    /// `SetCurrentPosition`: the byte's frame; playing, it goes on from there.
    pub fn set_current_position(&self, id: u32, byte: u32) -> u32 {
        self.on_buffer(
            "SetCurrentPosition",
            id,
            i64::from(byte),
            |buffer, now_us, back| {
                if buffer.primary {
                    return DSERR_INVALIDCALL;
                }
                if byte as usize >= buffer.memory.len() {
                    return DSERR_INVALIDPARAM;
                }
                let frame = u64::from(byte / buffer.format.block());
                buffer.move_to(id, frame, now_us, back);
                DS_OK
            },
        )
    }

    /// `SetFormat`: a primary's alone, a secondary's format being its for life.
    pub fn set_format(&self, id: u32, wave: Option<WaveFormat>) -> u32 {
        let rate = wave.map_or(0, |asked| asked.rate_hz);
        self.on_buffer("SetFormat", id, i64::from(rate), |buffer, _, _| {
            if !buffer.primary {
                return DSERR_INVALIDCALL;
            }
            let Some(asked) = wave else {
                return DSERR_INVALIDPARAM;
            };
            let Some(format) = Format::read(&asked) else {
                return DSERR_BADFORMAT;
            };
            buffer.format = format;
            buffer.rate_hz = format.rate_hz;
            DS_OK
        })
    }

    /// `SetVolume`, in hundredths of a decibel; out of range, the value is kept.
    pub fn set_volume(&self, id: u32, volume: i32) -> u32 {
        self.on_buffer("SetVolume", id, i64::from(volume), |buffer, _, _| {
            if (DSBVOLUME_MIN..=DSBVOLUME_MAX).contains(&volume) {
                buffer.volume = volume;
                DS_OK
            } else {
                DSERR_INVALIDPARAM
            }
        })
    }

    /// `SetPan`, in hundredths of a decibel; out of range, the value is kept.
    pub fn set_pan(&self, id: u32, pan: i32) -> u32 {
        self.on_buffer("SetPan", id, i64::from(pan), |buffer, _, _| {
            if (DSBPAN_LEFT..=DSBPAN_RIGHT).contains(&pan) {
                buffer.pan = pan;
                DS_OK
            } else {
                DSERR_INVALIDPARAM
            }
        })
    }

    /// `SetFrequency`: 0 is the format's own. Playing, it goes on from the clock's
    /// frame by a new door, a door's rate being fixed; what the old one held goes.
    pub fn set_frequency(&self, id: u32, rate_hz: u32) -> u32 {
        self.on_buffer(
            "SetFrequency",
            id,
            i64::from(rate_hz),
            |buffer, now_us, back| {
                if buffer.primary {
                    return DSERR_INVALIDCALL;
                }
                let new_rate = if rate_hz == DSBFREQUENCY_ORIGINAL {
                    buffer.format.rate_hz
                } else if (DSBFREQUENCY_MIN..=DSBFREQUENCY_MAX).contains(&rate_hz) {
                    rate_hz
                } else {
                    return DSERR_INVALIDPARAM;
                };
                let frame = buffer.heard_frame(now_us);
                buffer.rate_hz = new_rate;
                if matches!(buffer.motion, Motion::Playing(_)) {
                    buffer.move_to(id, frame, now_us, back);
                }
                DS_OK
            },
        )
    }

    /// `Restore`: nothing here is ever lost, so nothing needs it.
    pub fn restore(&self, id: u32) -> u32 {
        self.on_buffer("Restore", id, 0, |_, _, _| DS_OK)
    }

    /// Runs `write` on a buffer's bytes, a host's way in: on a host wider than 32
    /// bits the word `Lock` answers is not a pointer. `None` for a buffer gone.
    pub fn with_bytes<R>(&self, id: u32, write: impl FnOnce(&[Cell<u8>]) -> R) -> Option<R> {
        let memory = Rc::clone(&self.state.borrow().buffers.get(&id)?.memory);
        Some(write(&memory))
    }
}
