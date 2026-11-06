// SPDX-License-Identifier: MIT
//! The calls a program makes to find its CD-audio disc: WINMM's
//! `mciSendCommandA` and `auxGetNumDevs`, answered from a row, no disc read and
//! no sound, on any target.
//!
//! What the answers hold lives in a [`table::Row`], kept in the game's rows
//! crate: how many devices, how many tracks, the device's name and number.
//! [`Front`] answers 8 calls: how many devices, a device's name, open it, count
//! in tracks, how many tracks, how many auxiliary devices, its mode, close it.
//! Each writes one `imp.call` line where its answer is computed: `c` the call's
//! name, `a` its numbers and the record's words it read, `v` its answer, `o`
//! what it wrote, `by` that this crate answered.
//!
//! No mode is ever answered. A mode is a claim about a disc nothing here has
//! read, so the caller gets `MCIERR_UNSUPPORTED_FUNCTION` and goes without.
//!
//! Not answered yet: every other message or flags, `MCI_PLAY` among them, a
//! device but the row's, a record holding what no row answers, and the names
//! `mciGetErrorStringA`, `auxGetDevCapsA`, `auxSetVolume` and `auxGetVolume`.
//! Each is a stop by name, never a guess: one `cd.stop` line, then [`Back::stop`].
//!
//! The crate names no game, nothing of any one target and no allocator, so it
//! builds for `wasm32-unknown-unknown` and runs where no heap exists yet.
#![cfg_attr(not(test), no_std)]

#[cfg(any(test, feature = "disc"))]
extern crate alloc;
#[cfg(all(any(test, feature = "ogg"), not(target_arch = "wasm32")))]
extern crate std;

#[cfg(any(test, feature = "disc"))]
pub mod disc;
#[cfg(any(test, feature = "door"))]
pub mod door;
#[cfg(test)]
mod door_tests;
#[cfg(all(any(test, feature = "ogg"), not(target_arch = "wasm32")))]
pub mod ogg;
#[cfg(all(test, not(target_arch = "wasm32")))]
mod ogg_tests;
pub mod page;
pub mod play;
#[cfg(test)]
mod play_tests;
pub mod table;
#[cfg(test)]
mod test_disc;
#[cfg(test)]
mod test_log;
#[cfg(test)]
mod tests;
pub mod tmsf;

use oxide_trace::Args;
use table::Row;

const OPEN: u32 = 0x803;
const CLOSE: u32 = 0x804;
const SET: u32 = 0x80d;
const SYSINFO: u32 = 0x810;
const STATUS: u32 = 0x814;

/// `MCI_SYSINFO_QUANTITY`: how many devices, written through the return pointer.
const SYSINFO_QUANTITY: u32 = 0x100;
/// `MCI_SYSINFO_NAME`: one device's name, written through the return pointer.
const SYSINFO_NAME: u32 = 0x400;
/// `MCI_OPEN_TYPE`: open by the device's name.
const OPEN_TYPE: u32 = 0x2000;
/// `MCI_SET_TIME_FORMAT`.
const SET_TIME_FORMAT: u32 = 0x400;
/// `MCI_STATUS_ITEM`.
const STATUS_ITEM: u32 = 0x100;

/// `MCI_DEVTYPE_CD_AUDIO`.
const CD_AUDIO: u32 = 0x204;
/// `MCI_FORMAT_TMSF`: time counted in tracks, minutes, seconds, frames.
const FORMAT_TMSF: u32 = 10;
/// `MCI_STATUS_NUMBER_OF_TRACKS`.
const ITEM_TRACKS: u32 = 3;
/// `MCI_STATUS_MODE`.
const ITEM_MODE: u32 = 4;

const SUCCESS: u32 = 0;
/// `MCIERR_UNSUPPORTED_FUNCTION`: `MCIERR_BASE`, 256, plus 18.
const UNSUPPORTED_FUNCTION: u32 = 274;
/// No auxiliary device, so no one asks to set its volume.
const NO_AUX_DEVICES: u32 = 0;

/// The return word of a `MCI_SYSINFO_PARMS` or `MCI_STATUS_PARMS`: in the first
/// an address to write through, in the second the answer itself.
const RETURN: usize = 1;
/// `MCI_SYSINFO_PARMS`' `dwRetSize`, in bytes.
const RETURN_SIZE: usize = 2;
/// `MCI_SYSINFO_PARMS`' `dwNumber`: which device, from 1.
const NUMBER: usize = 3;
/// `MCI_SYSINFO_PARMS`' `wDeviceType`.
const DEVICE_TYPE: usize = 4;
/// `MCI_OPEN_PARMS`' `wDeviceID`, written by the open.
const DEVICE_ID: usize = 1;
/// `MCI_SET_PARMS`' `dwTimeFormat`.
const TIME_FORMAT: usize = 1;
/// `MCI_STATUS_PARMS`' `dwItem`.
const ITEM: usize = 2;

/// A line's numbers, in order.
macro_rules! numbers {
    ($($value:expr),+) => {
        Args([$(oxide_trace::__shown!($value)),+])
    };
}
pub(crate) use numbers;

/// What a target does for the front.
pub trait Back {
    /// Ends the program; the front has written the call's `cd.stop` line.
    fn stop(&self, name: &str) -> !;
    /// Writes one line around whatever the target must keep across it.
    fn quiet(&self, line: impl FnOnce()) {
        line();
    }
}

/// The record a call was handed, as its 32-bit words, whatever a pointer is on
/// this target. The front reads and writes only the words its message's record
/// holds, and a message that carries no record touches nothing.
pub trait Record {
    /// The word at `index`.
    fn word(&self, index: usize) -> u32;
    /// Writes the word at `index`.
    fn set_word(&mut self, index: usize, value: u32);
    /// Writes `chunks`, one after the other, at the address in the word at `index`.
    fn write_through(&mut self, index: usize, chunks: &[&[u8]]);
}

/// One file's CD-audio calls: its row, and its target.
pub struct Front<B> {
    file: &'static Row,
    back: B,
}

impl<B: Back> Front<B> {
    pub const fn new(file: &'static Row, back: B) -> Self {
        Self { file, back }
    }

    /// `mciSendCommandA`. A device number of 0 is the calls before the open's;
    /// every later call must name the row's. Returns 0, or
    /// `MCIERR_UNSUPPORTED_FUNCTION` for the mode; anything else stops.
    pub fn send(&self, device: u32, message: u32, flags: u32, record: &mut impl Record) -> u32 {
        let opened = device == self.file.device_id;
        match (message, flags) {
            (SYSINFO, SYSINFO_QUANTITY) if device == 0 => self.quantity(record),
            (SYSINFO, SYSINFO_NAME) if device == 0 => self.device_name(record),
            (OPEN, OPEN_TYPE) if device == 0 => self.open(record),
            (SET, SET_TIME_FORMAT) if opened => self.time_format(record),
            (STATUS, STATUS_ITEM) if opened => self.status(record),
            (CLOSE, 0) if opened => self.close(),
            _ => self.refuse(&numbers!(device, message, flags)),
        }
    }

    /// `MCI_SYSINFO_QUANTITY`: the row's count, through the return pointer.
    fn quantity(&self, record: &mut impl Record) -> u32 {
        let (kind, size) = (record.word(DEVICE_TYPE), record.word(RETURN_SIZE));
        if kind != CD_AUDIO || size < 4 {
            self.refuse(&numbers!(0u32, SYSINFO, SYSINFO_QUANTITY, kind, size));
        }
        let letters = self.file.letters;
        record.write_through(RETURN, &[&letters.to_le_bytes()]);
        self.back.quiet(|| {
            log::debug!(
                c = self.file.calls.send_command,
                a:% = numbers!(0u32, SYSINFO, SYSINFO_QUANTITY, kind, size),
                v:% = oxide_trace::__shown!(SUCCESS),
                o:% = oxide_trace::__shown!(letters),
                by = "cd-audio"; "imp.call"
            );
        });
        SUCCESS
    }

    /// `MCI_SYSINFO_NAME`: the row's name and its zero, through the return
    /// pointer, for a device the count allowed and a buffer that holds it.
    fn device_name(&self, record: &mut impl Record) -> u32 {
        let kind = record.word(DEVICE_TYPE);
        let (number, size) = (record.word(NUMBER), record.word(RETURN_SIZE));
        let name = self.file.device_name;
        let fits = name.len() < size as usize;
        if kind != CD_AUDIO || number == 0 || number > self.file.letters || !fits {
            self.refuse(&numbers!(0u32, SYSINFO, SYSINFO_NAME, kind, number, size));
        }
        record.write_through(RETURN, &[name.as_bytes(), &[0]]);
        self.back.quiet(|| {
            log::debug!(
                c = self.file.calls.send_command,
                a:% = numbers!(0u32, SYSINFO, SYSINFO_NAME, kind, number, size),
                v:% = oxide_trace::__shown!(SUCCESS),
                o = name,
                by = "cd-audio"; "imp.call"
            );
        });
        SUCCESS
    }

    /// `MCI_OPEN`: hands out the row's number. The type's text is not read: the
    /// one name a caller can hold is the one [`Self::device_name`] gave it.
    fn open(&self, record: &mut impl Record) -> u32 {
        let id = self.file.device_id;
        record.set_word(DEVICE_ID, id);
        self.back.quiet(|| {
            log::debug!(
                c = self.file.calls.send_command,
                a:% = numbers!(0u32, OPEN, OPEN_TYPE),
                v:% = oxide_trace::__shown!(SUCCESS),
                o:% = oxide_trace::__shown!(id),
                by = "cd-audio"; "imp.call"
            );
        });
        SUCCESS
    }

    /// `MCI_SET`: track time alone; nothing here counts it, as nothing plays.
    fn time_format(&self, record: &impl Record) -> u32 {
        let (id, format) = (self.file.device_id, record.word(TIME_FORMAT));
        if format != FORMAT_TMSF {
            self.refuse(&numbers!(id, SET, SET_TIME_FORMAT, format));
        }
        self.back.quiet(|| {
            log::debug!(
                c = self.file.calls.send_command,
                a:% = numbers!(id, SET, SET_TIME_FORMAT, format),
                v:% = oxide_trace::__shown!(SUCCESS),
                by = "cd-audio"; "imp.call"
            );
        });
        SUCCESS
    }

    /// `MCI_STATUS`: the row's track count in the record's own return word; the
    /// mode refused, its return word left as it was.
    fn status(&self, record: &mut impl Record) -> u32 {
        let (id, item) = (self.file.device_id, record.word(ITEM));
        match item {
            ITEM_TRACKS => {
                let tracks = self.file.tracks;
                record.set_word(RETURN, tracks);
                self.back.quiet(|| {
                    log::debug!(
                        c = self.file.calls.send_command,
                        a:% = numbers!(id, STATUS, STATUS_ITEM, item),
                        v:% = oxide_trace::__shown!(SUCCESS),
                        o:% = oxide_trace::__shown!(tracks),
                        by = "cd-audio"; "imp.call"
                    );
                });
                SUCCESS
            }
            ITEM_MODE => {
                self.back.quiet(|| {
                    log::debug!(
                        c = self.file.calls.send_command,
                        a:% = numbers!(id, STATUS, STATUS_ITEM, item),
                        v:% = oxide_trace::__shown!(UNSUPPORTED_FUNCTION),
                        by = "cd-audio"; "imp.call"
                    );
                });
                UNSUPPORTED_FUNCTION
            }
            _ => self.refuse(&numbers!(id, STATUS, STATUS_ITEM, item)),
        }
    }

    /// `MCI_CLOSE`: nothing is held. The front keeps no state, so a second close
    /// answers as the first did.
    fn close(&self) -> u32 {
        let id = self.file.device_id;
        self.back.quiet(|| {
            log::debug!(
                c = self.file.calls.send_command,
                a:% = numbers!(id, CLOSE, 0u32),
                v:% = oxide_trace::__shown!(SUCCESS),
                by = "cd-audio"; "imp.call"
            );
        });
        SUCCESS
    }

    /// `auxGetNumDevs`: none, on any target.
    #[must_use]
    pub fn aux_devices(&self) -> u32 {
        self.back.quiet(|| {
            log::debug!(
                c = self.file.calls.aux_devices,
                v:% = oxide_trace::__shown!(NO_AUX_DEVICES),
                by = "cd-audio"; "imp.call"
            );
        });
        NO_AUX_DEVICES
    }

    /// `mciGetErrorStringA`: stops, its numbers alone logged.
    pub fn error_string(&self, code: u32, size: u32) -> ! {
        self.stop(self.file.calls.error_string, &numbers!(code, size))
    }

    /// `auxGetDevCapsA`: stops, its numbers alone logged.
    pub fn aux_caps(&self, device: u32, size: u32) -> ! {
        self.stop(self.file.calls.aux_caps, &numbers!(device, size))
    }

    /// `auxSetVolume`: stops.
    pub fn aux_set_volume(&self, device: u32, volume: u32) -> ! {
        self.stop(self.file.calls.aux_set_volume, &numbers!(device, volume))
    }

    /// `auxGetVolume`: stops, its number alone logged.
    pub fn aux_get_volume(&self, device: u32) -> ! {
        self.stop(self.file.calls.aux_get_volume, &numbers!(device))
    }

    /// A `mciSendCommandA` no row answers.
    fn refuse<const N: usize>(&self, arguments: &Args<'_, N>) -> ! {
        self.stop(self.file.calls.send_command, arguments)
    }

    /// The line, then the end of the program.
    fn stop<const N: usize>(&self, name: &str, arguments: &Args<'_, N>) -> ! {
        log::error!(c = name, a:% = arguments, by = "cd-audio"; "cd.stop");
        self.back.stop(name)
    }
}
