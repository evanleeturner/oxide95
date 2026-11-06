// SPDX-License-Identifier: MIT
//! The calls a program makes to the Miles sound system, answered with no need of a
//! device and no audio thread the crate starts: voices move by the clock a [`Host`]
//! reads, the mix goes to an [`Output`], and a sample ends on time even if that output
//! refuses every frame.
//!
//! [`Front`] answers the 29 names in [`table::FRONTS`], each writing one
//! `imp.call` line where its answer is computed: `c` the call, `a` its numbers,
//! `v` its answer, `o` what it wrote through a pointer, `p` the file name it was
//! given, `by` that this crate answered. Each of the 15 in [`table::STOPS`] ends
//! the program through [`Front::named_stop`], one `sound.named.stop` line first,
//! never a quiet answer.
//! What no row covers stops the same way: a file not PCM, a double buffer's third
//! slot, a sample format mss.h does not name.
//!
//! The door, [`Front::stream_open`] and its two neighbours, is for a program's own
//! sound beside Miles': it opens the output itself, and `AIL_waveOutClose` leaves
//! it sounding.
//!
//! Chosen here, not Miles' own: every handle and driver number, which is a plain
//! number and not a pointer, so a program that read through one would break. Read
//! from mss.h and its documents, and unmeasured against Miles: the volume and pan
//! curves, the end-of-sample `AIL_end_sample` owes, and `AIL_set_sample_type`'s
//! values. A callback fires at the next call or [`Front::pump`] after a sample's
//! end, so a program with no call running needs a thread or a frame to pump.
//!
//! The crate names no game. One module is one target's: `waveout`, a back over winmm
//! and the thread that pumps it, compiled on Windows alone. The rest needs an allocator
//! and nothing of one target, so it builds for `wasm32-unknown-unknown` too. One more
//! back stands behind a feature, off by default: `page`, the ring a browser page's
//! script reads.
#![cfg_attr(not(test), no_std)]

extern crate alloc;

mod door;
mod lock;
pub mod output;
#[cfg(any(test, feature = "page"))]
pub mod page;
#[cfg(any(test, feature = "page"))]
mod ring;
pub mod table;
#[cfg(test)]
mod test_log;
#[cfg(test)]
mod tests;
mod voice;
pub mod wave;
#[cfg(any(windows, test))]
pub mod waveout;

use alloc::vec::Vec;
use core::fmt;

use door::Door;
use lock::Lock;
use oxide_trace::Args;
use output::Output;
use table::Row;
use voice::{MIX_HZ, USER_WORDS, Voice};
use wave::{FORMAT_PCM, Wave};

pub use output::NullBack;
#[cfg(any(test, feature = "page"))]
pub use ring::RING_FRAMES;
pub use voice::{SMP_DONE, SMP_FREE, SMP_PLAYING, SMP_STOPPED};
#[cfg(windows)]
pub use waveout::{WaveOutBack, start_pump_thread};

/// Where chosen handles start: never 0, which a program reads as a failure.
const HANDLE_BASE: u32 = 0x5300_0000;
const DRIVER_BASE: u32 = 0x4400_0000;
const DOOR_BASE: u32 = 0x4f00_0000;
const NUMBER_STEP: u32 = 16;

/// The most the mix renders at a time, so a long gap between calls costs time
/// and not memory.
const MIX_CHUNK: u64 = 1024;

/// mmsystem.h's `MMSYSERR_NODRIVER`, which the program takes as no MIDI and goes on.
const NO_DRIVER: u32 = 6;

/// A line's numbers, in order.
macro_rules! numbers {
    ($($value:expr),+) => {
        Args([$(oxide_trace::__shown!($value)),+])
    };
}

/// A call's name as a line writes it. A type and not `format_args!`, whose
/// temporary would not live to the end of the `log` statement.
struct Called<'a>(&'a str, &'a str);

impl fmt::Display for Called<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}!{}", self.0, self.1)
    }
}

/// What a target does for the front.
pub trait Host {
    /// The mix's clock, in microseconds; it must never go back.
    fn now_us(&self) -> u64;
    /// Calls an end-of-sample callback. The front's lock is free, so the callback
    /// may call the front.
    fn call_back(&self, callback: u32, handle: u32);
    /// Reads a named file into a block of the target's; `None` if it could not.
    fn read_file(&self, name: &str) -> Option<u32>;
    /// Frees a block [`Host::read_file`] gave.
    fn free_block(&self, block: u32);
    /// Writes one line around whatever the target must keep across it.
    fn quiet(&self, line: impl FnOnce()) {
        line();
    }
}

/// What a call leaves to do after the lock is free.
struct Pass {
    owed: Vec<(u32, u32)>,
    dropped: u64,
    refused: Option<u32>,
}

impl Pass {
    const fn new() -> Self {
        Self { owed: Vec::new(), dropped: 0, refused: None }
    }
}

struct State<O> {
    output: O,
    /// The back has answered an open and not been closed.
    opened: bool,
    /// It took the mix's own rate, so frames may be written to it.
    writes: bool,
    driver: u32,
    drivers_opened: u32,
    voices: Vec<Voice>,
    doors: Vec<Door>,
    doors_opened: u32,
    blocks: Vec<u32>,
    preferences: Vec<(u32, u32)>,
    clock_from: Option<u64>,
    mixed: u64,
    scratch: Vec<i32>,
    written: Vec<i16>,
    pass: Pass,
}

impl<O> State<O> {
    const fn new(output: O) -> Self {
        Self {
            output,
            opened: false,
            writes: false,
            driver: 0,
            drivers_opened: 0,
            voices: Vec::new(),
            doors: Vec::new(),
            doors_opened: 0,
            blocks: Vec::new(),
            preferences: Vec::new(),
            clock_from: None,
            mixed: 0,
            scratch: Vec::new(),
            written: Vec::new(),
            pass: Pass::new(),
        }
    }

    fn slot_of(&self, handle: u32) -> Option<usize> {
        let past_base = handle.checked_sub(HANDLE_BASE)?;
        if past_base % NUMBER_STEP != 0 {
            return None;
        }
        let slot = (past_base / NUMBER_STEP) as usize;
        self.voices.get(slot).filter(|voice| voice.used).map(|_| slot)
    }

    /// A free slot's handle, so numbers come back as they did in the logged run;
    /// 0 on a driver not open, which the program guards by a test.
    #[allow(clippy::cast_possible_truncation)]
    fn allocate(&mut self, driver: u32) -> u32 {
        if driver == 0 || driver != self.driver {
            return 0;
        }
        if let Some(voice) = self.voices.iter_mut().find(|voice| !voice.used) {
            voice.used = true;
            voice.init();
            return voice.handle;
        }
        let handle = HANDLE_BASE + NUMBER_STEP * self.voices.len() as u32;
        self.voices.push(Voice::new(handle));
        handle
    }

    fn sounding(&self) -> bool {
        self.voices.iter().any(|voice| voice.used && voice.status == SMP_PLAYING)
            || self.doors.iter().any(Door::sounding)
    }
}

impl<O: Output> State<O> {
    /// Brings every voice to `now_us`. The mix runs first, so a voice's last frames
    /// sound before the clock ends it.
    fn advance(&mut self, now_us: u64) {
        let from = *self.clock_from.get_or_insert(now_us);
        let target = now_us.saturating_sub(from) * MIX_HZ / 1_000_000;
        let frames = target.saturating_sub(self.mixed);
        self.mixed = self.mixed.max(target);
        if frames > 0 && self.sounding() {
            self.mix(frames);
        }
        for voice in &mut self.voices {
            if voice.used && voice.catch_up(now_us) && voice.callback != 0 {
                self.pass.owed.push((voice.callback, voice.handle));
            }
        }
    }

    /// Renders `frames` and hands the back what it has room for. Frames past its
    /// room are still rendered, so the cursors stay with the clock.
    #[allow(clippy::cast_possible_truncation)]
    fn mix(&mut self, mut frames: u64) {
        let mut room = if self.writes { self.output.room() as u64 } else { 0 };
        while frames > 0 {
            let chunk = frames.min(MIX_CHUNK);
            frames -= chunk;
            self.scratch.clear();
            self.scratch.resize(chunk as usize * 2, 0);
            for voice in self.voices.iter_mut().filter(|voice| voice.used) {
                voice.render(&mut self.scratch);
            }
            for door in &mut self.doors {
                door.render(&mut self.scratch);
            }
            if !self.writes {
                continue;
            }
            let taken = chunk.min(room);
            room -= taken;
            self.pass.dropped += chunk - taken;
            if taken > 0 {
                self.written.clear();
                self.written.extend(
                    self.scratch[..taken as usize * 2]
                        .iter()
                        .map(|&sum| sum.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16),
                );
                self.output.write(&self.written);
            }
        }
    }

    /// A back refusing, or taking another rate, leaves the mix unwritten and the
    /// program going on: nothing here depends on a device.
    #[allow(clippy::cast_possible_truncation)]
    fn open_output(&mut self) {
        if self.opened {
            return;
        }
        let took = self.output.open(MIX_HZ as u32);
        self.opened = took != 0;
        self.writes = u64::from(took) == MIX_HZ;
        if !self.writes {
            self.pass.refused = Some(took);
        }
    }

    fn close_output(&mut self) {
        if self.opened {
            self.output.close();
        }
        self.opened = false;
        self.writes = false;
    }
}

/// One file's Miles calls: its row, its target, and the mix's output.
pub struct Front<H, O> {
    row: &'static Row,
    host: H,
    state: Lock<State<O>>,
}

impl<H: Host, O: Output> Front<H, O> {
    pub const fn new(row: &'static Row, host: H, output: O) -> Self {
        Self { row, host, state: Lock::new(State::new(output)) }
    }

    /// The one path every call takes: the clock moves every voice, the call runs,
    /// and with the lock free its line is written and any callback fires. A callback
    /// run under the lock would hang the moment it called back in.
    fn front<R>(
        &self,
        call: impl FnOnce(&mut State<O>, u64) -> R,
        line: impl FnOnce(&R),
    ) -> R {
        let (answer, pass) = self.state.with(|state| {
            let now_us = self.host.now_us();
            state.advance(now_us);
            let answer = call(state, now_us);
            (answer, core::mem::replace(&mut state.pass, Pass::new()))
        });
        self.host.quiet(|| line(&answer));
        if let Some(took) = pass.refused {
            self.host.quiet(|| {
                log::warn!(took:% = oxide_trace::__shown!(took), by = "sound-mix"; "sound.output.refused");
            });
        }
        if pass.dropped > 0 {
            self.host.quiet(|| {
                log::debug!(
                    frames:% = oxide_trace::__shown!(pass.dropped),
                    by = "sound-mix"; "sound.frames.dropped"
                );
            });
        }
        for (callback, handle) in pass.owed {
            self.host.call_back(callback, handle);
        }
        answer
    }

    fn called<'a>(&'a self, name: &'a str) -> Called<'a> {
        Called(self.row.module, name)
    }

    /// The call's line, then the end of the program.
    fn stop_because(&self, name: &str, why: &str) -> ! {
        log::error!(c:% = self.called(name), why = why, by = "sound-mix"; "sound.named.stop");
        log::logger().flush();
        panic!("sound-mix: {name}: {why}");
    }

    fn not_live(&self, name: &str, number: u32) -> ! {
        log::error!(c:% = self.called(name), a:% = numbers!(number), by = "sound-mix"; "sound.not.live");
        log::logger().flush();
        panic!("sound-mix: {name}: {number:x} is not live");
    }

    fn voice<'s>(&self, state: &'s mut State<O>, name: &str, handle: u32) -> &'s mut Voice {
        match state.slot_of(handle) {
            Some(slot) => &mut state.voices[slot],
            None => self.not_live(name, handle),
        }
    }

    /// The sound in a file; anything this mix could not play stops by name.
    fn sound_of<'f>(&self, name: &str, file: &'f [u8]) -> Wave<'f> {
        match wave::parse(file) {
            Ok(sound)
                if sound.format == FORMAT_PCM
                    && matches!(sound.channels, 1 | 2)
                    && matches!(sound.bits, 8 | 16)
                    && sound.rate_hz != 0 =>
            {
                sound
            }
            _ => self.stop_because(name, "not-pcm-wave"),
        }
    }

    /// For a thread or a frame: the same path, with no line of its own, so a sample
    /// ends and its callback fires though the program calls nothing.
    pub fn pump(&self) {
        self.front(|_, _| (), |()| {});
    }

    /// One of [`table::STOPS`]: the program went where no one has read it going.
    pub fn named_stop(&self, name: &str) -> ! {
        self.stop_because(name, "unreached")
    }

    pub fn startup(&self) {
        self.front(
            |_, _| (),
            |()| log::debug!(c:% = self.called("AIL_startup"), by = "sound-mix"; "imp.call"),
        );
    }

    /// Ends every voice, door and driver. A handle kept past this is not live.
    pub fn shutdown(&self) {
        self.front(
            |state, _| {
                state.driver = 0;
                state.voices.clear();
                state.doors.clear();
                state.close_output();
            },
            |()| log::debug!(c:% = self.called("AIL_shutdown"), by = "sound-mix"; "imp.call"),
        );
    }

    /// Answers 0 and a new driver number; the target writes the number through the
    /// call's first pointer and 0 through its second. The mix keeps its own rate
    /// whatever the program asks.
    pub fn wave_out_open(&self, device: u32, rate_hz: u32, channels: u32, bits: u32) -> (u32, u32) {
        self.front(
            |state, _| {
                state.drivers_opened += 1;
                state.driver = DRIVER_BASE + NUMBER_STEP * state.drivers_opened;
                state.open_output();
                (0, state.driver)
            },
            |&(answer, driver)| {
                log::debug!(
                    c:% = self.called("AIL_waveOutOpen"),
                    a:% = numbers!(device, rate_hz, channels, bits),
                    v:% = oxide_trace::__shown!(answer),
                    o:% = oxide_trace::__shown!(driver),
                    by = "sound-mix"; "imp.call"
                );
            },
        )
    }

    /// Every Miles voice ends, no callback fired, its handle still valid: the bytes
    /// let a handle outlive the close. A door keeps the output open.
    pub fn wave_out_close(&self, driver: u32) {
        self.front(
            |state, _| {
                if driver == 0 || driver != state.driver {
                    self.not_live("AIL_waveOutClose", driver);
                }
                state.driver = 0;
                for voice in state.voices.iter_mut().filter(|voice| voice.used) {
                    voice.end();
                }
                if state.doors.is_empty() {
                    state.close_output();
                }
            },
            |()| {
                log::debug!(
                    c:% = self.called("AIL_waveOutClose"),
                    a:% = numbers!(driver),
                    by = "sound-mix"; "imp.call"
                );
            },
        );
    }

    pub fn serve(&self) {
        self.front(
            |_, _| (),
            |()| log::debug!(c:% = self.called("AIL_serve"), by = "sound-mix"; "imp.call"),
        );
    }

    /// Answers the value it replaces, 0 at first. Nothing here reads a preference.
    pub fn set_preference(&self, number: u32, value: u32) -> u32 {
        self.front(
            |state, _| {
                if let Some(held) = state.preferences.iter_mut().find(|held| held.0 == number) {
                    return core::mem::replace(&mut held.1, value);
                }
                state.preferences.push((number, value));
                0
            },
            |answer| {
                log::debug!(
                    c:% = self.called("AIL_set_preference"),
                    a:% = numbers!(number, value),
                    v:% = oxide_trace::__shown!(*answer),
                    by = "sound-mix"; "imp.call"
                );
            },
        )
    }

    /// A chosen number, or 0 on a driver not open.
    pub fn allocate_sample_handle(&self, driver: u32) -> u32 {
        self.front(
            |state, _| state.allocate(driver),
            |answer| {
                log::debug!(
                    c:% = self.called("AIL_allocate_sample_handle"),
                    a:% = numbers!(driver),
                    v:% = oxide_trace::__shown!(*answer),
                    by = "sound-mix"; "imp.call"
                );
            },
        )
    }

    pub fn init_sample(&self, handle: u32) {
        self.front(
            |state, _| self.voice(state, "AIL_init_sample", handle).init(),
            |()| {
                log::debug!(
                    c:% = self.called("AIL_init_sample"),
                    a:% = numbers!(handle),
                    by = "sound-mix"; "imp.call"
                );
            },
        );
    }

    /// Frees the slot for the next allocate; a voice still sounding goes silent and
    /// owes no callback.
    pub fn release_sample_handle(&self, handle: u32) {
        self.front(
            |state, _| {
                let voice = self.voice(state, "AIL_release_sample_handle", handle);
                voice.init();
                voice.status = SMP_FREE;
                voice.used = false;
            },
            |()| {
                log::debug!(
                    c:% = self.called("AIL_release_sample_handle"),
                    a:% = numbers!(handle),
                    by = "sound-mix"; "imp.call"
                );
            },
        );
    }

    pub fn set_sample_volume(&self, handle: u32, volume: i32) {
        self.front(
            |state, _| self.voice(state, "AIL_set_sample_volume", handle).volume = volume,
            |()| {
                log::debug!(
                    c:% = self.called("AIL_set_sample_volume"),
                    a:% = numbers!(handle, volume),
                    by = "sound-mix"; "imp.call"
                );
            },
        );
    }

    /// 0 is forever.
    pub fn set_sample_loop_count(&self, handle: u32, loops: u32) {
        self.front(
            |state, _| self.voice(state, "AIL_set_sample_loop_count", handle).loops = loops,
            |()| {
                log::debug!(
                    c:% = self.called("AIL_set_sample_loop_count"),
                    a:% = numbers!(handle, loops),
                    by = "sound-mix"; "imp.call"
                );
            },
        );
    }

    pub fn start_sample(&self, handle: u32) {
        self.front(
            |state, now_us| self.voice(state, "AIL_start_sample", handle).start(now_us),
            |()| {
                log::debug!(
                    c:% = self.called("AIL_start_sample"),
                    a:% = numbers!(handle),
                    by = "sound-mix"; "imp.call"
                );
            },
        );
    }

    pub fn stop_sample(&self, handle: u32) {
        self.front(
            |state, now_us| self.voice(state, "AIL_stop_sample", handle).stop(now_us),
            |()| {
                log::debug!(
                    c:% = self.called("AIL_stop_sample"),
                    a:% = numbers!(handle),
                    by = "sound-mix"; "imp.call"
                );
            },
        );
    }

    pub fn resume_sample(&self, handle: u32) {
        self.front(
            |state, now_us| self.voice(state, "AIL_resume_sample", handle).resume(now_us),
            |()| {
                log::debug!(
                    c:% = self.called("AIL_resume_sample"),
                    a:% = numbers!(handle),
                    by = "sound-mix"; "imp.call"
                );
            },
        );
    }

    /// Ends it now. A voice playing or stopped owes its end-of-sample, by mss's
    /// documents, unmeasured.
    pub fn end_sample(&self, handle: u32) {
        self.front(
            |state, _| {
                let voice = self.voice(state, "AIL_end_sample", handle);
                let (owed, callback) = (voice.end(), voice.callback);
                if owed && callback != 0 {
                    state.pass.owed.push((callback, handle));
                }
            },
            |()| {
                log::debug!(
                    c:% = self.called("AIL_end_sample"),
                    a:% = numbers!(handle),
                    by = "sound-mix"; "imp.call"
                );
            },
        );
    }

    /// Takes a copy of `file`'s sound, the bytes at `address` read by the target;
    /// `block` is only logged. Answers 1.
    pub fn set_sample_file(&self, handle: u32, address: u32, file: &[u8], block: u32) -> u32 {
        let sound = self.sound_of("AIL_set_sample_file", file);
        self.front(
            |state, _| {
                let voice = self.voice(state, "AIL_set_sample_file", handle);
                voice.set_file(
                    u32::from(sound.channels),
                    u32::from(sound.bits),
                    sound.rate_hz,
                    sound.data,
                );
                1
            },
            |answer| {
                log::debug!(
                    c:% = self.called("AIL_set_sample_file"),
                    a:% = numbers!(handle, address, block),
                    v:% = oxide_trace::__shown!(*answer),
                    by = "sound-mix"; "imp.call"
                );
            },
        )
    }

    /// Allocate, init and set file in one call; 0 on a driver not open.
    pub fn allocate_file_sample(&self, driver: u32, address: u32, file: &[u8], block: u32) -> u32 {
        let sound = self.sound_of("AIL_allocate_file_sample", file);
        self.front(
            |state, _| {
                let handle = state.allocate(driver);
                if handle != 0 {
                    self.voice(state, "AIL_allocate_file_sample", handle).set_file(
                        u32::from(sound.channels),
                        u32::from(sound.bits),
                        sound.rate_hz,
                        sound.data,
                    );
                }
                handle
            },
            |answer| {
                log::debug!(
                    c:% = self.called("AIL_allocate_file_sample"),
                    a:% = numbers!(driver, address, block),
                    v:% = oxide_trace::__shown!(*answer),
                    by = "sound-mix"; "imp.call"
                );
            },
        )
    }

    pub fn sample_status(&self, handle: u32) -> u32 {
        self.front(
            |state, _| self.voice(state, "AIL_sample_status", handle).status,
            |answer| {
                log::debug!(
                    c:% = self.called("AIL_sample_status"),
                    a:% = numbers!(handle),
                    v:% = oxide_trace::__shown!(*answer),
                    by = "sound-mix"; "imp.call"
                );
            },
        )
    }

    /// `format` 0 to 3: mono 8, mono 16, stereo 8, stereo 16, by mss.h, unmeasured;
    /// the program passes 0 and 0. Anything else stops, since these bits change how
    /// every byte is read.
    pub fn set_sample_type(&self, handle: u32, format: u32, flags: u32) {
        let (channels, bits) = match (format, flags) {
            (0, 0) => (1, 8),
            (1, 0) => (1, 16),
            (2, 0) => (2, 8),
            (3, 0) => (2, 16),
            _ => self.stop_because("AIL_set_sample_type", "format-unnamed"),
        };
        self.front(
            |state, _| {
                let voice = self.voice(state, "AIL_set_sample_type", handle);
                voice.channels = channels;
                voice.bits = bits;
            },
            |()| {
                log::debug!(
                    c:% = self.called("AIL_set_sample_type"),
                    a:% = numbers!(handle, format, flags),
                    by = "sound-mix"; "imp.call"
                );
            },
        );
    }

    pub fn set_sample_playback_rate(&self, handle: u32, rate_hz: u32) {
        self.front(
            |state, now_us| {
                self.voice(state, "AIL_set_sample_playback_rate", handle).set_rate(now_us, rate_hz);
            },
            |()| {
                log::debug!(
                    c:% = self.called("AIL_set_sample_playback_rate"),
                    a:% = numbers!(handle, rate_hz),
                    by = "sound-mix"; "imp.call"
                );
            },
        );
    }

    pub fn set_sample_pan(&self, handle: u32, pan: i32) {
        self.front(
            |state, _| self.voice(state, "AIL_set_sample_pan", handle).pan = pan,
            |()| {
                log::debug!(
                    c:% = self.called("AIL_set_sample_pan"),
                    a:% = numbers!(handle, pan),
                    by = "sound-mix"; "imp.call"
                );
            },
        );
    }

    /// One of a voice's 8 user words; a ninth stops.
    pub fn set_sample_user_data(&self, handle: u32, index: u32, value: u32) {
        let word = self.user_word("AIL_set_sample_user_data", index);
        self.front(
            |state, _| self.voice(state, "AIL_set_sample_user_data", handle).user[word] = value,
            |()| {
                log::debug!(
                    c:% = self.called("AIL_set_sample_user_data"),
                    a:% = numbers!(handle, index, value),
                    by = "sound-mix"; "imp.call"
                );
            },
        );
    }

    pub fn sample_user_data(&self, handle: u32, index: u32) -> u32 {
        let word = self.user_word("AIL_sample_user_data", index);
        self.front(
            |state, _| self.voice(state, "AIL_sample_user_data", handle).user[word],
            |answer| {
                log::debug!(
                    c:% = self.called("AIL_sample_user_data"),
                    a:% = numbers!(handle, index),
                    v:% = oxide_trace::__shown!(*answer),
                    by = "sound-mix"; "imp.call"
                );
            },
        )
    }

    fn user_word(&self, name: &str, index: u32) -> usize {
        let word = index as usize;
        if word >= USER_WORDS {
            self.stop_because(name, "user-word-past-8");
        }
        word
    }

    /// Answers the callback it replaces, 0 at first.
    pub fn register_eos_callback(&self, handle: u32, callback: u32) -> u32 {
        self.front(
            |state, _| {
                let voice = self.voice(state, "AIL_register_EOS_callback", handle);
                core::mem::replace(&mut voice.callback, callback)
            },
            |answer| {
                log::debug!(
                    c:% = self.called("AIL_register_EOS_callback"),
                    a:% = numbers!(handle, callback),
                    v:% = oxide_trace::__shown!(*answer),
                    by = "sound-mix"; "imp.call"
                );
            },
        )
    }

    /// Fills a double buffer's slot with a copy of `data`, so the caller may change
    /// its own buffer at once.
    pub fn load_sample_buffer(&self, handle: u32, slot: u32, address: u32, data: &[u8]) {
        if slot > 1 {
            self.stop_because("AIL_load_sample_buffer", "slot-past-2");
        }
        let bytes = data.len();
        self.front(
            |state, _| {
                self.voice(state, "AIL_load_sample_buffer", handle).load(slot as usize, data);
            },
            |()| {
                log::debug!(
                    c:% = self.called("AIL_load_sample_buffer"),
                    a:% = numbers!(handle, slot, address, bytes),
                    by = "sound-mix"; "imp.call"
                );
            },
        );
    }

    /// The slot to fill next, -1 if both hold sound.
    pub fn sample_buffer_ready(&self, handle: u32) -> i32 {
        self.front(
            |state, _| self.voice(state, "AIL_sample_buffer_ready", handle).ready(),
            |answer| {
                log::debug!(
                    c:% = self.called("AIL_sample_buffer_ready"),
                    a:% = numbers!(handle),
                    v:% = oxide_trace::__shown!(*answer),
                    by = "sound-mix"; "imp.call"
                );
            },
        )
    }

    /// Answers the block [`Host::read_file`] gave, 0 if it gave none. A destination
    /// of the caller's own stops: every site read passes 0. `address` is where the
    /// program held the name, only logged.
    pub fn file_read(&self, address: u32, name: &str, destination: u32) -> u32 {
        if destination != 0 {
            self.stop_because("FILE_read", "destination-not-0");
        }
        let block = self.host.read_file(name);
        self.front(
            |state, _| {
                if let Some(block) = block {
                    state.blocks.push(block);
                }
                block.unwrap_or(0)
            },
            |answer| {
                log::debug!(
                    c:% = self.called("FILE_read"),
                    a:% = numbers!(address, destination),
                    v:% = oxide_trace::__shown!(*answer),
                    p = name,
                    by = "sound-mix"; "imp.call"
                );
            },
        )
    }

    /// Frees what [`Front::file_read`] handed out, and nothing else. 0 does nothing.
    /// Any other number stops the program: a free of what this pair did not hand
    /// out, or a second free, is a bug that a quiet answer would bury.
    pub fn mem_free_lock(&self, block: u32) {
        let known = self.front(
            |state, _| {
                if block == 0 {
                    return true;
                }
                let held = state.blocks.iter().position(|&held| held == block);
                held.map(|at| state.blocks.swap_remove(at)).is_some()
            },
            |&known| {
                if known {
                    log::debug!(
                        c:% = self.called("MEM_free_lock"),
                        a:% = numbers!(block),
                        by = "sound-mix"; "imp.call"
                    );
                } else {
                    log::error!(
                        c:% = self.called("MEM_free_lock"),
                        a:% = numbers!(block),
                        by = "sound-mix"; "sound.free.unknown"
                    );
                }
            },
        );
        assert!(known, "sound-mix: MEM_free_lock: {block:x} was not handed out by FILE_read");
        if block != 0 {
            self.host.free_block(block);
        }
    }

    /// Answers `MMSYSERR_NODRIVER`, the target writing 0 through the call's pointer:
    /// the program tries twice and goes on with no MIDI.
    pub fn midi_out_open(&self, device: u32) -> u32 {
        self.front(
            |_, _| NO_DRIVER,
            |answer| {
                log::debug!(
                    c:% = self.called("AIL_midiOutOpen"),
                    a:% = numbers!(device),
                    v:% = oxide_trace::__shown!(*answer),
                    by = "sound-mix"; "imp.call"
                );
            },
        )
    }

    /// Opens a door: `channels` is 1 or 2, `rate_hz` not 0. It opens the output if
    /// no driver has. Answers a chosen number.
    pub fn stream_open(&self, rate_hz: u32, channels: u32) -> u32 {
        if rate_hz == 0 || !matches!(channels, 1 | 2) {
            self.stop_because("stream_open", "door-shape");
        }
        self.front(
            |state, _| {
                state.doors_opened += 1;
                let id = DOOR_BASE + NUMBER_STEP * state.doors_opened;
                state.doors.push(Door::new(id, rate_hz, channels));
                state.open_output();
                id
            },
            |answer| {
                log::debug!(
                    a:% = numbers!(rate_hz, channels),
                    v:% = oxide_trace::__shown!(*answer),
                    by = "sound-mix"; "sound.door.open"
                );
            },
        )
    }

    /// Queues interleaved samples on a door.
    pub fn stream_push(&self, id: u32, samples: &[i16]) {
        let count = samples.len();
        self.front(
            |state, _| match state.doors.iter_mut().find(|door| door.id == id) {
                Some(door) => door.push(samples),
                None => self.not_live("stream_push", id),
            },
            |()| {
                log::debug!(
                    a:% = numbers!(id, count),
                    by = "sound-mix"; "sound.door.push"
                );
            },
        );
    }

    /// Ends a door, what it still held unheard. The output closes with the last
    /// door if the driver has gone too.
    pub fn stream_close(&self, id: u32) {
        self.front(
            |state, _| {
                let Some(at) = state.doors.iter().position(|door| door.id == id) else {
                    self.not_live("stream_close", id);
                };
                state.doors.swap_remove(at);
                if state.driver == 0 && state.doors.is_empty() {
                    state.close_output();
                }
            },
            |()| log::debug!(a:% = numbers!(id), by = "sound-mix"; "sound.door.close"),
        );
    }
}
