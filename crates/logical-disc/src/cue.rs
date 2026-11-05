// SPDX-License-Identifier: MIT
//! The cue sheet, read from its public grammar: `FILE "<name>" <type>`
//! opens a file, `TRACK <nn> <mode>` a track in it, `INDEX <nn>
//! <mm:ss:ff>` a place in the track, counted from its file's start at 75
//! frames a second. Taken: files `BINARY` (little-endian samples) and
//! `MOTOROLA` (big-endian); tracks `MODE1/2352` and `AUDIO`; index 00, the
//! pregap's start, and 01, the track's; every track needs its 01. Every
//! other command (`REM`, `TITLE`, `PERFORMER`, `FLAGS`, `PREGAP` and the
//! rest) and index is passed over; any other file type or track mode is
//! refused, its line named.
use alloc::vec::Vec;

use crate::disc::OpenError;

/// Frames a second, the sheet's `ff`.
pub(crate) const FRAMES: u32 = 75;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    Mode1,
    Audio,
}

/// A track: its number, its kind, and its places in frames from its
/// file's start.
pub(crate) struct Track {
    pub(crate) number: u8,
    pub(crate) mode: Mode,
    pub(crate) pregap: Option<u32>,
    pub(crate) start: u32,
}

/// A file the sheet names, its samples' order, and its tracks.
pub(crate) struct File {
    pub(crate) name: Vec<u8>,
    pub(crate) big_endian: bool,
    pub(crate) tracks: Vec<Track>,
}

/// The sheet's files, in its order.
pub(crate) fn parse(text: &[u8]) -> Result<Vec<File>, OpenError> {
    let text = text.strip_prefix(b"\xef\xbb\xbf").unwrap_or(text);
    let mut files: Vec<File> = Vec::new();
    // The line of the last track while it has no index 01.
    let mut unstarted = None;
    for (at, line) in text.split(|&byte| byte == b'\n').enumerate() {
        let refused = OpenError::Cue(at + 1);
        let (command, rest) = word(line);
        match command {
            b"FILE" => {
                if let Some(line) = unstarted {
                    return Err(OpenError::Cue(line));
                }
                let (name, kind) = quoted(rest).ok_or(refused)?;
                let big_endian = match word(kind).0 {
                    b"BINARY" => false,
                    b"MOTOROLA" => true,
                    _ => return Err(refused),
                };
                files.push(File {
                    name: name.to_vec(),
                    big_endian,
                    tracks: Vec::new(),
                });
            }
            b"TRACK" => {
                if let Some(line) = unstarted {
                    return Err(OpenError::Cue(line));
                }
                let file = files.last_mut().ok_or(refused)?;
                let (number, mode) = word(rest);
                let number = number_of(number)
                    .and_then(|number| u8::try_from(number).ok())
                    .ok_or(refused)?;
                let mode = match word(mode).0 {
                    b"MODE1/2352" => Mode::Mode1,
                    b"AUDIO" => Mode::Audio,
                    _ => return Err(refused),
                };
                file.tracks.push(Track {
                    number,
                    mode,
                    pregap: None,
                    start: 0,
                });
                unstarted = Some(at + 1);
            }
            b"INDEX" => {
                let track = files
                    .last_mut()
                    .and_then(|file| file.tracks.last_mut())
                    .ok_or(refused)?;
                let (index, time) = word(rest);
                let frames = frames_of(word(time).0).ok_or(refused)?;
                match number_of(index).ok_or(refused)? {
                    0 => track.pregap = Some(frames),
                    1 => {
                        track.start = frames;
                        unstarted = None;
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    match unstarted {
        Some(line) => Err(OpenError::Cue(line)),
        None => Ok(files),
    }
}

/// A line's first word, and the rest after it.
fn word(line: &[u8]) -> (&[u8], &[u8]) {
    let line = line.trim_ascii();
    let end = line
        .iter()
        .position(u8::is_ascii_whitespace)
        .unwrap_or(line.len());
    (&line[..end], line[end..].trim_ascii_start())
}

/// A file's name, in quotes or a bare word, and the rest after it.
fn quoted(rest: &[u8]) -> Option<(&[u8], &[u8])> {
    if let Some(inside) = rest.strip_prefix(b"\"") {
        let end = inside.iter().position(|&byte| byte == b'"')?;
        Some((&inside[..end], &inside[end + 1..]))
    } else {
        let (name, kind) = word(rest);
        (!name.is_empty()).then_some((name, kind))
    }
}

/// A run of decimal digits.
fn number_of(digits: &[u8]) -> Option<u32> {
    if digits.is_empty() {
        return None;
    }
    digits.iter().try_fold(0u32, |value, &digit| {
        let digit = char::from(digit).to_digit(10)?;
        value.checked_mul(10)?.checked_add(digit)
    })
}

/// `mm:ss:ff` in frames.
fn frames_of(time: &[u8]) -> Option<u32> {
    let mut parts = time.split(|&byte| byte == b':').map(number_of);
    let (minutes, seconds, frames) = (parts.next()??, parts.next()??, parts.next()??);
    if parts.next().is_some() || seconds >= 60 || frames >= FRAMES {
        return None;
    }
    minutes
        .checked_mul(60)?
        .checked_add(seconds)?
        .checked_mul(FRAMES)?
        .checked_add(frames)
}
