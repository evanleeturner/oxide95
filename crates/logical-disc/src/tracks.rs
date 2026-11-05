// SPDX-License-Identifier: MIT
//! The audio side: a disc's tracks as a game's music device
//! plays them, never decoded here. An image's `AUDIO` tracks are
//! [`Pcm`]: raw frames of IEC 60908, 2,352 bytes each, 588 stereo samples
//! of 16 bits at 44,100 Hz, read through [`Disc::read_track`], one
//! `disc.call` line a read. A store folder's are [`Encoded`]: whole files
//! a row's [`TrackFiles`] rule finds through a `kernel_files::Folder`,
//! their bytes the folder's and their length their decoder's.
use alloc::ffi::CString;
use alloc::format;
use alloc::vec::Vec;

use kernel_files::{Create, Error, Folder, Mode};

use crate::cue::{self, Mode as TrackMode};
use crate::disc::{Disc, RAW};
use crate::folder::line;
use crate::{Format, ReadAt, TrackFiles};

/// One CD audio track.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Track {
    /// Its number on the disc, the one a game asks.
    pub number: u8,
    pub audio: Audio,
}

/// Where a track's sound is, and in what form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Audio {
    Pcm(Pcm),
    Encoded(Encoded),
}

/// An image's track: raw frames in one of its source files.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pcm {
    /// Its first frame, `INDEX 01`, counted from the disc's first sector.
    pub start: u32,
    /// Its frames, to the next track's `INDEX 00` or its file's end; the
    /// pregap is never in them.
    pub frames: u32,
    /// The samples big-endian (a `MOTOROLA` file), else little-endian.
    pub big_endian: bool,
    /// The source file, by its place in the sheet.
    pub(crate) file: usize,
    /// The first frame's byte in it.
    pub(crate) offset: u64,
}

/// A store folder's track: one file, its form, decoded by a library.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Encoded {
    /// The file's name, as the rule asked the folder for it.
    pub name: Vec<u8>,
    pub format: Format,
}

/// The sheet's `AUDIO` tracks, by their place on the disc. Each file's
/// frames follow the last's; a track runs to the next one's `INDEX 00`
/// (its `INDEX 01` with none) in its file, or to the file's end.
pub(crate) fn of_sheet<R: ReadAt>(sheet: &[cue::File], files: &[R]) -> Vec<Track> {
    let mut tracks = Vec::new();
    let mut base = 0u64;
    for (at, (file, bytes)) in sheet.iter().zip(files).enumerate() {
        let length = bytes.size() / RAW;
        for (place, track) in file.tracks.iter().enumerate() {
            if track.mode != TrackMode::Audio {
                continue;
            }
            let end = file
                .tracks
                .get(place + 1)
                .map_or(length, |next| u64::from(next.pregap.unwrap_or(next.start)));
            let frames = end.saturating_sub(u64::from(track.start));
            tracks.push(Track {
                number: track.number,
                audio: Audio::Pcm(Pcm {
                    start: u32::try_from(base + u64::from(track.start)).unwrap_or(u32::MAX),
                    frames: u32::try_from(frames).unwrap_or(u32::MAX),
                    big_endian: file.big_endian,
                    file: at,
                    offset: u64::from(track.start) * RAW,
                }),
            });
        }
        base += length;
    }
    tracks
}

impl<R: ReadAt> Disc<R> {
    /// The audio tracks, by CD number; none for an iso alone.
    #[must_use]
    pub fn tracks(&self) -> &[Track] {
        &self.tracks
    }

    /// Track `number`'s bytes from its byte `at` into `into`, one range of
    /// its source file; the count moved, short at its end. A number with
    /// no image track answers `NotFound`.
    pub fn read_track(&self, number: u8, at: u64, into: &mut [u8]) -> Result<usize, Error> {
        let answer = self.pcm(number).and_then(|pcm| {
            let left = (u64::from(pcm.frames) * RAW).saturating_sub(at);
            let count = usize::try_from(left).map_or(into.len(), |left| left.min(into.len()));
            self.files[pcm.file].read_at(pcm.offset + at, &mut into[..count])?;
            Ok(count)
        });
        let moved = answer.map_or(0, |count| u32::try_from(count).unwrap_or(u32::MAX));
        line(
            "read_track",
            u32::from(number),
            b"",
            answer.map(|_| moved),
            moved,
        );
        answer
    }

    fn pcm(&self, number: u8) -> Result<&Pcm, Error> {
        self.tracks
            .iter()
            .find_map(|track| match &track.audio {
                Audio::Pcm(pcm) if track.number == number => Some(pcm),
                _ => None,
            })
            .ok_or(Error::NotFound)
    }
}

impl TrackFiles {
    /// The tracks in `folder` by this rule: `<stem><nn><extension>` from
    /// `first` on, two digits, each asked by an open and a close; the
    /// first number with no file ends the run.
    pub fn find(&self, folder: &impl Folder) -> Result<Vec<Track>, Error> {
        let mode = Mode {
            read: true,
            write: false,
            create: Create::Existing,
        };
        let mut tracks = Vec::new();
        for number in self.first..=99 {
            let name = format!("{}{number:02}{}", self.stem, self.extension);
            let asked = CString::new(name).map_err(|_| Error::BadArgument)?;
            match folder.open(&asked, mode) {
                Ok(file) => folder.close(file)?,
                Err(Error::NotFound) => break,
                Err(error) => return Err(error),
            }
            tracks.push(Track {
                number,
                audio: Audio::Encoded(Encoded {
                    name: asked.into_bytes(),
                    format: self.format,
                }),
            });
        }
        Ok(tracks)
    }
}
