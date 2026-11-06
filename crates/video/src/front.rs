// SPDX-License-Identifier: MIT
//! The 8 fronts and the clip table. Each writes one `imp.call` line naming the
//! library's own import, `by=video`, so a line reads against the same call
//! answered by the original library.
//!
//! The table has no fixed size. A clip's picture and cursor stay here; the
//! pixels reach a caller's buffer through the one pointer `smack_to_buffer`
//! was handed, at `y * pitch + x`, never past `pitch * height`.

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::fmt;
use core::ptr;

use video_smacker::{Clip, Picture, Refused};

use crate::{Choices, ClipBytes, Clock, Handle, SoundDoor, Track, VoidBlocks};

/// What each line says computed its answer.
const BY: &str = "video";

const OPEN: &str = "SMACKW32!SmackOpen";
const CLOSE: &str = "SMACKW32!SmackClose";
const TO_BUFFER: &str = "SMACKW32!SmackToBuffer";
const DO_FRAME: &str = "SMACKW32!SmackDoFrame";
const NEXT_FRAME: &str = "SMACKW32!SmackNextFrame";
const GOTO: &str = "SMACKW32!SmackGoto";
const WAIT: &str = "SMACKW32!SmackWait";
const SOUND_IN_TRACK: &str = "SMACKW32!SmackSoundInTrack";

/// A handle no clip in the table holds.
const UNKNOWN_HANDLE: &str = "unknown-handle";
/// A `smack_goto` number naming no frame of the clip.
const BAD_FRAME: &str = "bad-frame";
/// A `smack_to_buffer` given no buffer.
const NULL_BUFFER: &str = "null-buffer";
/// A frame painted before any buffer was named.
const NO_TARGET: &str = "no-target";
/// A frame the decoder refused; its own `smk.refused` line gives the reason.
const DECODER: &str = "decoder";

/// The library's bit for a clip's first track, in `smack_open`'s flags and
/// `smack_sound_in_track`'s mask. The 6 bits above it name tracks the decoder
/// refuses to open a clip with, so no answer turns on them.
const TRACK_1: u32 = 0x2000;

/// `smack_wait`'s answers: the frame is due, and it is not yet.
const DUE: u32 = 0;
const NOT_DUE: u32 = 1;

/// Numbers as the line shows them: hex, comma-joined, empty for none.
struct Numbers<'a>(&'a [u64]);

impl fmt::Display for Numbers<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (at, number) in self.0.iter().enumerate() {
            if at > 0 {
                f.write_str(",")?;
            }
            write!(f, "{number:x}")?;
        }
        Ok(())
    }
}

/// A name as the line shows it: a byte past printable ASCII as `\xNN`.
struct Text<'a>(&'a [u8]);

impl fmt::Display for Text<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for &byte in self.0 {
            if byte.is_ascii_graphic() || byte == b' ' {
                write!(f, "{}", char::from(byte))?;
            } else {
                write!(f, "\\x{byte:02x}")?;
            }
        }
        Ok(())
    }
}

/// The one line a front writes.
fn line(call: &str, arguments: &[u64], answer: Option<u64>) {
    match answer {
        Some(value) => log::debug!(
            c = call, a:% = Numbers(arguments), v:% = Numbers(&[value]), by = BY; "imp.call"
        ),
        None => log::debug!(c = call, a:% = Numbers(arguments), by = BY; "imp.call"),
    }
}

/// A front's named refusal.
fn refuse(call: &str, why: &str) {
    log::debug!(c = call, w = why; "video.refused");
}

/// A handle as the line shows it.
fn address(handle: *const Handle) -> u64 {
    handle.addr() as u64
}

/// Where in a caller's buffer a clip lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Place {
    /// Pixels from the buffer's left to the clip's.
    pub x: u32,
    /// Rows from the buffer's top to the clip's.
    pub y: u32,
    /// Bytes from one of the buffer's rows to the next.
    pub pitch: u32,
    /// The buffer's rows.
    pub height: u32,
}

/// A caller's buffer, named by `smack_to_buffer`.
struct Target {
    place: Place,
    buffer: *mut u8,
}

impl Target {
    /// Writes `bytes` at `column` of the clip's `row`, which the caller has
    /// checked lie inside the buffer.
    #[allow(unsafe_code, reason = "the one write through a caller's own pointer")]
    fn put(&self, row: usize, column: usize, bytes: &[u8]) {
        let Place { x, y, pitch, .. } = self.place;
        let at = (y as usize + row) * pitch as usize + x as usize + column;
        // SAFETY: `smack_to_buffer`'s contract: the buffer holds `pitch * height`
        // bytes while it is named here, and `write` cuts each row to fit.
        unsafe {
            ptr::copy_nonoverlapping(bytes.as_ptr(), self.buffer.add(at), bytes.len());
        }
    }
}

/// One open clip.
struct Playing {
    handle: Box<Handle>,
    clip: Clip,
    picture: Picture,
    /// The frame `smack_do_frame` paints next, from 0; the frame count past the last.
    cursor: u32,
    /// The frame last painted, unless a `smack_goto` came since.
    painted: Option<u32>,
    target: Option<Target>,
    /// The door's number, for a clip opened to play.
    stream: Option<u32>,
    /// The next paint writes the whole picture: what the target shows is unknown.
    whole_next: bool,
    /// A frame's length.
    period_us: u64,
    /// `anchor_frame` is due at `anchor_us`, and each later frame a period on.
    anchor_us: u64,
    anchor_frame: u32,
}

impl Playing {
    fn new(clip: Clip, now_us: u64) -> Self {
        let header = *clip.header();
        Self {
            handle: Box::new(Handle::new(header.width, header.height, header.frames)),
            clip,
            picture: Picture::new(),
            cursor: 0,
            painted: None,
            target: None,
            stream: None,
            whole_next: false,
            period_us: u64::from(header.frame_us),
            anchor_us: now_us,
            anchor_frame: 0,
        }
    }

    /// The instant the cursor's frame is due.
    fn due_us(&self) -> u64 {
        self.anchor_us + u64::from(self.cursor - self.anchor_frame) * self.period_us
    }

    /// The clip's track as a door takes it; none for a track of no rate, which no
    /// door could drain.
    fn track(&self) -> Option<Track> {
        let track = self.clip.header().track?;
        (track.hz != 0).then_some(Track {
            rate_hz: track.hz,
            channels: track.channels,
        })
    }

    /// Paints the cursor's frame and answers its samples, none for a clip with no
    /// stream; the cursor stays. Frames a seek steps over stay unheard.
    fn paint(&mut self, void_blocks: VoidBlocks) -> Result<Vec<i16>, Refused> {
        let frame = self.cursor;
        let follows = match self.painted {
            None => frame == 0,
            Some(last) => last + 1 == frame,
        };
        self.clip.seek(frame)?;
        let samples = if self.stream.is_some() {
            self.clip.frame_samples()?
        } else {
            Vec::new()
        };
        let painted = self.clip.decode_next_frame(&mut self.picture)?;
        let changed = painted.palette_changed || self.picture.palette != self.handle.palette;
        self.handle.palette_changed = u32::from(changed);
        self.handle.palette = self.picture.palette;
        let whole = !follows || self.whole_next || void_blocks == VoidBlocks::Written;
        self.whole_next = false;
        self.painted = Some(frame);
        self.write(whole);
        Ok(samples)
    }

    /// Writes the picture to the target, whole or by its painted blocks alone,
    /// cut at the buffer's edges.
    fn write(&self, whole: bool) {
        let Some(target) = &self.target else {
            return;
        };
        let (width, height) = (self.handle.width as usize, self.handle.height as usize);
        let Place { x, y, pitch, .. } = target.place;
        let rows = height.min(target.place.height.saturating_sub(y) as usize);
        let across = width.min(pitch.saturating_sub(x) as usize);
        let pixels = &self.picture.pixels;
        if whole {
            for row in 0..rows {
                target.put(row, 0, &pixels[row * width..row * width + across]);
            }
            return;
        }
        for (block, _) in self
            .clip
            .painted_blocks()
            .iter()
            .enumerate()
            .filter(|(_, painted)| **painted)
        {
            let left = block % (width / 4) * 4;
            let top = block / (width / 4) * 4;
            let end = across.min(left + 4);
            for row in top..rows.min(top + 4) {
                if left < end {
                    target.put(row, left, &pixels[row * width + left..row * width + end]);
                }
            }
        }
    }
}

/// The 8 fronts over one clock, one source of bytes and one sound door.
pub struct Videos<C, B, S> {
    clock: C,
    bytes: B,
    sound: S,
    choices: Choices,
    clips: Vec<Playing>,
}

impl<C: Clock, B: ClipBytes, S: SoundDoor> Videos<C, B, S> {
    /// No clip open yet.
    pub const fn new(clock: C, bytes: B, sound: S, choices: Choices) -> Self {
        Self {
            clock,
            bytes,
            sound,
            choices,
            clips: Vec::new(),
        }
    }

    /// The clock, for its owner to move.
    pub fn clock_mut(&mut self) -> &mut C {
        &mut self.clock
    }

    /// The sound door.
    pub fn sound(&self) -> &S {
        &self.sound
    }

    /// The clips now open.
    #[must_use]
    pub fn open_clips(&self) -> usize {
        self.clips.len()
    }

    fn find(&mut self, handle: *const Handle) -> Option<&mut Playing> {
        self.clips
            .iter_mut()
            .find(|one| ptr::eq(&raw const *one.handle, handle))
    }

    /// `_SmackOpen`: reads the file whole and answers its handle; null for a file
    /// the back lacks or a clip the decoder refuses, and no row kept. `flags` naming
    /// the clip's track open its stream, if the door plays; the other bits and
    /// `extra` are kept in the line and change nothing.
    pub fn smack_open(&mut self, name: &[u8], flags: u32, extra: u32) -> *mut Handle {
        let bytes = self.bytes.read(name);
        log::debug!(p:% = Text(name), n = bytes.as_ref().map_or(0, Vec::len); "video.open");
        let clip = bytes.and_then(|bytes| Clip::parse(&bytes).ok());
        let handle = match clip {
            Some(clip) => {
                let mut one = Playing::new(clip, self.clock.now_us());
                if flags & TRACK_1 != 0 && self.sound.plays() {
                    one.stream = one.track().map(|track| {
                        let stream = self.sound.stream_open(track);
                        log::debug!(
                            s = stream, r = track.rate_hz, c = track.channels; "video.stream.open"
                        );
                        stream
                    });
                }
                let handle = ptr::from_mut(&mut *one.handle);
                self.clips.push(one);
                handle
            }
            None => ptr::null_mut(),
        };
        line(
            OPEN,
            &[u64::from(flags), u64::from(extra)],
            Some(address(handle)),
        );
        handle
    }

    /// `_SmackClose`: the handle is gone, its pointer with it.
    pub fn smack_close(&mut self, handle: *const Handle) {
        let found = self
            .clips
            .iter()
            .position(|one| ptr::eq(&raw const *one.handle, handle));
        match found {
            Some(at) => {
                if let Some(stream) = self.clips.swap_remove(at).stream {
                    self.sound.stream_close(stream);
                    log::debug!(s = stream; "video.stream.close");
                }
            }
            None => refuse(CLOSE, UNKNOWN_HANDLE),
        }
        line(CLOSE, &[address(handle)], None);
    }

    /// `_SmackToBuffer`: names where the clip's frames land.
    ///
    /// # Safety
    ///
    /// `buffer` holds `place.pitch * place.height` writable bytes for as long as
    /// the clip holds it: until this clip is closed or names another buffer.
    #[allow(unsafe_code, reason = "the caller vouches for its buffer here")]
    pub unsafe fn smack_to_buffer(
        &mut self,
        handle: *const Handle,
        place: Place,
        buffer: *mut u8,
        flags: u32,
    ) {
        match self.find(handle) {
            None => refuse(TO_BUFFER, UNKNOWN_HANDLE),
            Some(_) if buffer.is_null() => refuse(TO_BUFFER, NULL_BUFFER),
            Some(one) => {
                one.whole_next |= one.painted.is_some();
                one.target = Some(Target { place, buffer });
            }
        }
        line(
            TO_BUFFER,
            &[
                address(handle),
                u64::from(place.x),
                u64::from(place.y),
                u64::from(place.pitch),
                u64::from(place.height),
                buffer.addr() as u64,
                u64::from(flags),
            ],
            None,
        );
    }

    /// `_SmackDoFrame`: paints the cursor's frame and pushes its samples, and does
    /// not move the cursor, so twice in a row paints and pushes once. Answers 0.
    pub fn smack_do_frame(&mut self, handle: *const Handle) -> u32 {
        let void_blocks = self.choices.void_blocks;
        let mut heard = None;
        match self.find(handle) {
            None => refuse(DO_FRAME, UNKNOWN_HANDLE),
            Some(one) if one.painted == Some(one.cursor) => {}
            Some(one) => match one.paint(void_blocks) {
                Err(_) => refuse(DO_FRAME, DECODER),
                Ok(samples) => {
                    if one.target.is_none() {
                        refuse(DO_FRAME, NO_TARGET);
                    }
                    heard = one.stream.zip(Some(samples));
                }
            },
        }
        if let Some((stream, samples)) = heard.filter(|(_, samples)| !samples.is_empty()) {
            self.sound.stream_push(stream, &samples);
        }
        line(DO_FRAME, &[address(handle)], Some(0));
        0
    }

    /// `_SmackNextFrame`: moves the cursor on, stopping one past the last frame,
    /// where `smack_do_frame` refuses by name until a `smack_goto`.
    pub fn smack_next_frame(&mut self, handle: *const Handle) {
        match self.find(handle) {
            None => refuse(NEXT_FRAME, UNKNOWN_HANDLE),
            Some(one) => one.cursor = (one.cursor + 1).min(one.handle.frames),
        }
        line(NEXT_FRAME, &[address(handle)], None);
    }

    /// `_SmackGoto`: moves the cursor and re-anchors the clock there. What the
    /// number counts from is [`Choices::goto_base`]. What the door still holds plays
    /// out; the next sound is the frame landed on.
    pub fn smack_goto(&mut self, handle: *const Handle, frame: u32) {
        let (now_us, base) = (self.clock.now_us(), self.choices.goto_base);
        match self.find(handle) {
            None => refuse(GOTO, UNKNOWN_HANDLE),
            Some(one) => match base.index(frame).filter(|at| *at < one.handle.frames) {
                None => refuse(GOTO, BAD_FRAME),
                Some(at) => {
                    one.cursor = at;
                    one.painted = None;
                    one.whole_next = true;
                    one.anchor_us = now_us;
                    one.anchor_frame = at;
                }
            },
        }
        line(GOTO, &[address(handle), u64::from(frame)], None);
    }

    /// `_SmackWait`, which never waits: 0 when the cursor's frame is due, else 1.
    /// A frame late by more than one period re-anchors there, so a stalled clock
    /// cannot burst frames.
    pub fn smack_wait(&mut self, handle: *const Handle) -> u32 {
        let now_us = self.clock.now_us();
        let answer = match self.find(handle) {
            None => {
                refuse(WAIT, UNKNOWN_HANDLE);
                DUE
            }
            Some(one) => {
                let due_us = one.due_us();
                log::debug!(i = one.cursor, t = now_us, d = due_us; "video.wait");
                if now_us < due_us {
                    NOT_DUE
                } else {
                    if now_us - due_us > one.period_us {
                        one.anchor_us = now_us;
                        one.anchor_frame = one.cursor;
                    }
                    DUE
                }
            }
        };
        line(WAIT, &[address(handle)], Some(u64::from(answer)));
        answer
    }

    /// `_SmackSoundInTrack`: 1 if the mask names a track the clip holds and the door
    /// plays, else 0, whichever flags the clip opened with. Only 0 or not was read
    /// from the one caller, so the 1 is this front's own.
    pub fn smack_sound_in_track(&mut self, handle: *const Handle, mask: u32) -> u32 {
        let plays = self.sound.plays();
        let answer = match self.find(handle) {
            None => {
                refuse(SOUND_IN_TRACK, UNKNOWN_HANDLE);
                0
            }
            Some(one) => u32::from(plays && mask & TRACK_1 != 0 && one.track().is_some()),
        };
        line(
            SOUND_IN_TRACK,
            &[address(handle), u64::from(mask)],
            Some(u64::from(answer)),
        );
        answer
    }
}
