// SPDX-License-Identifier: MIT
//! A disc that plays: the calls [`crate::Front`] answers plus the ones that need a disc.
//!
//! The position a program reads comes from the clock handed in, never from whatever is
//! making the sound, so a [`Player`] that falls behind or fails moves nothing a program
//! can see. A disc with no audio tracks holds nothing to play, so every call goes to the
//! quiet front and answers as it does.
//!
//! The mode a program reads is only ever stopped, playing or paused. An open door is a
//! claim about a machine nothing here has, and asking for one is a stop.
use core::cell::Cell;

use oxide_trace::Args;

use crate::table::{Place, Row, Tracks};
use crate::tmsf::{FRAMES_A_SECOND, pack, unpack};
use crate::{
    Back, CLOSE, Front, ITEM, ITEM_MODE, ITEM_TRACKS, RETURN, Record, STATUS, STATUS_ITEM, SUCCESS,
    numbers,
};

const PLAY: u32 = 0x806;
const STOP: u32 = 0x808;
const PAUSE: u32 = 0x809;
const RESUME: u32 = 0x855;

/// `MCI_FROM`.
const FROM: u32 = 4;
/// `MCI_TO`.
const TO: u32 = 8;
/// `MCI_TRACK`.
const TRACK: u32 = 0x10;

/// `MCI_STATUS_POSITION`.
const ITEM_POSITION: u32 = 2;
/// `MCI_MODE_STOP`.
const MODE_STOPPED: u32 = 525;
/// `MCI_MODE_PLAY`.
const MODE_PLAYING: u32 = 526;
/// `MCI_MODE_PAUSE`.
const MODE_PAUSED: u32 = 529;

/// `MCIERR_OUTOFRANGE`: `MCIERR_BASE`, 256, plus 26.
const OUT_OF_RANGE: u32 = 282;

/// `MCI_STATUS_PARMS`' `dwTrack`.
const STATUS_TRACK: usize = 3;
/// `MCI_PLAY_PARMS`' `dwFrom`.
const PLAY_FROM: usize = 1;
/// `MCI_PLAY_PARMS`' `dwTo`.
const PLAY_TO: usize = 2;

/// `AUXCAPSA`'s 48 bytes, the one size a caller may hand.
const AUX_CAPS_SIZE: u32 = 0x30;
const AUX_CAPS_WORDS: usize = 12;
/// `AUXCAPSA`'s `wTechnology`, the low half of this word.
const AUX_TECHNOLOGY: usize = 10;
/// `AUXCAPSA`'s `dwSupport`.
const AUX_SUPPORT: usize = 11;
/// `AUXCAPS_CDAUDIO`.
const AUX_CD_AUDIO: u32 = 1;
/// `AUXCAPS_VOLUME` and `AUXCAPS_LRVOLUME`.
const AUX_BOTH_VOLUMES: u32 = 3;
const ONE_AUX_DEVICE: u32 = 1;

/// Where the position's time comes from, in milliseconds, wrapping as `timeGetTime` does.
/// A closure serves, so a caller can hand its own clock's call in.
pub trait Clock {
    fn now_ms(&self) -> u32;
}

impl<F: Fn() -> u32> Clock for F {
    fn now_ms(&self) -> u32 {
        self()
    }
}

/// What makes the sound, if anything does. It answers nothing, so it can move nothing a
/// program reads. A resume is a [`Player::play`] from where the pause held.
pub trait Player {
    /// Replaces what plays: one voice, from `from` up to `to`.
    fn play(&self, from: Place, to: Place);
    fn pause(&self);
    fn stop(&self);
    /// Each side, 0 to 0xffff.
    fn volume(&self, left: u16, right: u16);
}

impl<P: Player> Player for &P {
    fn play(&self, from: Place, to: Place) {
        (*self).play(from, to);
    }

    fn pause(&self) {
        (*self).pause();
    }

    fn stop(&self) {
        (*self).stop();
    }

    fn volume(&self, left: u16, right: u16) {
        (*self).volume(left, right);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Stopped,
    Playing { since_ms: u32, from: u32 },
    Paused,
}

/// One file's CD-audio calls over a disc.
pub struct PlayFront<'t, B, C, P> {
    quiet: Front<B>,
    tracks: Tracks<'t>,
    clock: C,
    player: P,
    mode: Cell<Mode>,
    /// Where a run that is not playing rests, as one count of the disc's frames.
    at: Cell<u32>,
    /// Where the current run ends.
    end: Cell<u32>,
}

impl<'t, B: Back, C: Clock, P: Player> PlayFront<'t, B, C, P> {
    pub fn new(file: &'static Row, back: B, tracks: Tracks<'t>, clock: C, player: P) -> Self {
        Self {
            quiet: Front::new(file, back),
            tracks,
            clock,
            player,
            mode: Cell::new(Mode::Stopped),
            at: Cell::new(0),
            end: Cell::new(0),
        }
    }

    /// `mciSendCommandA`. Everything the quiet front answers goes to it, its stops too.
    pub fn send(&self, device: u32, message: u32, flags: u32, record: &mut impl Record) -> u32 {
        if self.tracks.is_empty() || device != self.quiet.file.device_id {
            return self.quiet.send(device, message, flags, record);
        }
        match (message, flags) {
            (STATUS, STATUS_ITEM) => self.status(record),
            (STATUS, f) if f == STATUS_ITEM | TRACK => self.track_position(record),
            (PLAY, FROM) => self.play(FROM, record.word(PLAY_FROM), None),
            (PLAY, f) if f == FROM | TO => {
                self.play(f, record.word(PLAY_FROM), Some(record.word(PLAY_TO)))
            }
            (PAUSE, 0) => self.pause(),
            (RESUME, 0) => self.resume(),
            (STOP, 0) => self.stop(),
            (CLOSE, 0) => self.close(),
            _ => self.quiet.send(device, message, flags, record),
        }
    }

    /// `auxGetNumDevs`: the disc's one device, or none with no disc to play.
    #[must_use]
    pub fn aux_devices(&self) -> u32 {
        if self.tracks.is_empty() {
            return self.quiet.aux_devices();
        }
        self.quiet.back.quiet(|| {
            log::debug!(
                c = self.quiet.file.calls.aux_devices,
                v:% = oxide_trace::__shown!(ONE_AUX_DEVICE),
                by = "cd-audio"; "imp.call"
            );
        });
        ONE_AUX_DEVICE
    }

    /// `auxGetDevCapsA`: what it is, on its one size of record, its name left empty.
    pub fn aux_caps(&self, device: u32, size: u32, record: &mut impl Record) -> u32 {
        if self.tracks.is_empty() || device != 0 || size != AUX_CAPS_SIZE {
            self.quiet.aux_caps(device, size);
        }
        for word in 0..AUX_CAPS_WORDS {
            record.set_word(word, 0);
        }
        record.set_word(AUX_TECHNOLOGY, AUX_CD_AUDIO);
        record.set_word(AUX_SUPPORT, AUX_BOTH_VOLUMES);
        self.quiet.back.quiet(|| {
            log::debug!(
                c = self.quiet.file.calls.aux_caps,
                a:% = numbers!(device, size),
                v:% = oxide_trace::__shown!(SUCCESS),
                o:% = oxide_trace::__shown!(AUX_CD_AUDIO),
                by = "cd-audio"; "imp.call"
            );
        });
        SUCCESS
    }

    /// `auxSetVolume`: the low half is the left side, the high half the right. It scales
    /// this crate's own sound and no other.
    pub fn aux_set_volume(&self, device: u32, volume: u32) -> u32 {
        if self.tracks.is_empty() || device != 0 {
            self.quiet.aux_set_volume(device, volume);
        }
        #[allow(clippy::cast_possible_truncation)]
        self.player.volume(volume as u16, (volume >> 16) as u16);
        self.quiet.back.quiet(|| {
            log::debug!(
                c = self.quiet.file.calls.aux_set_volume,
                a:% = numbers!(device, volume),
                v:% = oxide_trace::__shown!(SUCCESS),
                by = "cd-audio"; "imp.call"
            );
        });
        SUCCESS
    }

    /// `mciGetErrorStringA`: stops, as on the quiet front.
    pub fn error_string(&self, code: u32, size: u32) -> ! {
        self.quiet.error_string(code, size)
    }

    /// `auxGetVolume`: stops, as on the quiet front.
    pub fn aux_get_volume(&self, device: u32) -> ! {
        self.quiet.aux_get_volume(device)
    }

    /// Frames a run has covered by the clock; a run that has reached its end stops here,
    /// so the mode and the position can never disagree.
    fn settle(&self) {
        if let Mode::Playing { since_ms, from } = self.mode.get() {
            let at = from.saturating_add(frames_since(self.clock.now_ms(), since_ms));
            if at >= self.end.get() {
                self.at.set(self.end.get());
                self.mode.set(Mode::Stopped);
                self.player.stop();
            } else {
                self.at.set(at);
            }
        }
    }

    fn status(&self, record: &mut impl Record) -> u32 {
        let (id, item) = (self.quiet.file.device_id, record.word(ITEM));
        self.settle();
        let answer = match item {
            ITEM_MODE => match self.mode.get() {
                Mode::Stopped => MODE_STOPPED,
                Mode::Playing { .. } => MODE_PLAYING,
                Mode::Paused => MODE_PAUSED,
            },
            ITEM_TRACKS => self.tracks.count(),
            ITEM_POSITION => {
                let place = self.tracks.place(self.at.get());
                pack(place.track, place.frame)
            }
            _ => self.quiet.refuse(&numbers!(id, STATUS, STATUS_ITEM, item)),
        };
        record.set_word(RETURN, answer);
        self.line(
            &numbers!(id, STATUS, STATUS_ITEM, item),
            SUCCESS,
            Some(answer),
        );
        SUCCESS
    }

    /// A track's first place, which a program keeps and hands back to a play.
    fn track_position(&self, record: &mut impl Record) -> u32 {
        let id = self.quiet.file.device_id;
        let (item, track) = (record.word(ITEM), record.word(STATUS_TRACK));
        let flags = STATUS_ITEM | TRACK;
        if item != ITEM_POSITION {
            self.quiet.refuse(&numbers!(id, STATUS, flags, item, track));
        }
        let number = u8::try_from(track).ok().filter(|number| {
            self.tracks
                .start_of(Place {
                    track: *number,
                    frame: 0,
                })
                .is_some()
        });
        let Some(number) = number else {
            self.line(
                &numbers!(id, STATUS, flags, item, track),
                OUT_OF_RANGE,
                None,
            );
            return OUT_OF_RANGE;
        };
        let word = pack(number, 0);
        record.set_word(RETURN, word);
        self.line(
            &numbers!(id, STATUS, flags, item, track),
            SUCCESS,
            Some(word),
        );
        SUCCESS
    }

    fn play(&self, flags: u32, from_word: u32, to_word: Option<u32>) -> u32 {
        let id = self.quiet.file.device_id;
        let shown = to_word.unwrap_or(0);
        let from = self.tracks.start_of(place_of(from_word));
        let to = match to_word {
            Some(word) => self.tracks.end_of(place_of(word)),
            None => Some(self.tracks.end()),
        };
        let (Some(from), Some(to)) = (from, to) else {
            self.line(
                &numbers!(id, PLAY, flags, from_word, shown),
                OUT_OF_RANGE,
                None,
            );
            return OUT_OF_RANGE;
        };
        if to <= from {
            self.quiet
                .refuse(&numbers!(id, PLAY, flags, from_word, shown));
        }
        self.start(from, to);
        self.line(&numbers!(id, PLAY, flags, from_word, shown), SUCCESS, None);
        SUCCESS
    }

    fn start(&self, from: u32, to: u32) {
        self.at.set(from);
        self.end.set(to);
        self.mode.set(Mode::Playing {
            since_ms: self.clock.now_ms(),
            from,
        });
        self.player
            .play(self.tracks.place(from), self.tracks.place(to));
    }

    fn pause(&self) -> u32 {
        self.settle();
        if matches!(self.mode.get(), Mode::Playing { .. }) {
            self.mode.set(Mode::Paused);
            self.player.pause();
        }
        self.bare(PAUSE)
    }

    fn resume(&self) -> u32 {
        if self.mode.get() == Mode::Paused {
            self.start(self.at.get(), self.end.get());
        }
        self.bare(RESUME)
    }

    fn stop(&self) -> u32 {
        self.settle();
        if self.mode.get() != Mode::Stopped {
            self.mode.set(Mode::Stopped);
            self.player.stop();
        }
        self.bare(STOP)
    }

    /// Ends what plays and goes back to the disc's start, for the next open.
    fn close(&self) -> u32 {
        if self.mode.get() != Mode::Stopped {
            self.mode.set(Mode::Stopped);
            self.player.stop();
        }
        self.at.set(0);
        self.quiet.close()
    }

    /// A message with no record and no flags, answered.
    fn bare(&self, message: u32) -> u32 {
        let id = self.quiet.file.device_id;
        self.line(&numbers!(id, message, 0u32), SUCCESS, None);
        SUCCESS
    }

    fn line<const N: usize>(&self, arguments: &Args<'_, N>, answer: u32, wrote: Option<u32>) {
        let name = self.quiet.file.calls.send_command;
        self.quiet.back.quiet(|| match wrote {
            Some(wrote) => log::debug!(
                c = name,
                a:% = arguments,
                v:% = oxide_trace::__shown!(answer),
                o:% = oxide_trace::__shown!(wrote),
                by = "cd-audio"; "imp.call"
            ),
            None => log::debug!(
                c = name,
                a:% = arguments,
                v:% = oxide_trace::__shown!(answer),
                by = "cd-audio"; "imp.call"
            ),
        });
    }
}

fn place_of(word: u32) -> Place {
    let (track, frame) = unpack(word);
    Place { track, frame }
}

/// The disc's frames between two readings of the clock, which may have wrapped between.
pub(crate) fn frames_since(now_ms: u32, since_ms: u32) -> u32 {
    let elapsed = u64::from(now_ms.wrapping_sub(since_ms));
    u32::try_from(elapsed * u64::from(FRAMES_A_SECOND) / 1000).unwrap_or(u32::MAX)
}
