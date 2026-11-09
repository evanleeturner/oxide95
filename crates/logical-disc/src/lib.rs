// SPDX-License-Identifier: MIT
//! The player's own copy of a game's disc, read only and by range:
//! a cue sheet and its bin files, an iso alone, or a mounted disc's folder
//! as picked. The disc answers what a game reads from it,
//! never a CD drive rebuilt. Its one way to its bytes is [`ReadAt`], a
//! trait a target; a file's read takes its own sectors and no more, so an
//! image of 269M is never loaded whole. The data track's files answer as a
//! `kernel_files::Folder`, read only (`folder.rs`), each call one
//! `disc.call` line.
//!
//! Written from the formats' public descriptions, never from another
//! reader's code: the cue sheet's grammar (`cue.rs`); the raw sector of
//! ECMA-130, whose Mode 1 holds 12 sync bytes, a 4-byte header, then the
//! 2,048 user bytes from byte 16; ISO 9660 as ECMA-119 lays it out
//! (`iso.rs`). The crate names no game: each game's facts are its rows
//! crate's.
//!
//! [`ReadAt`] needs no allocator. The disc and its parsers do, so they are
//! the feature `disc`, which the swap never turns on.
#![cfg_attr(not(test), no_std)]

#[cfg(any(test, feature = "disc"))]
extern crate alloc;

#[cfg(any(test, feature = "disc"))]
mod check;
#[cfg(any(test, feature = "disc"))]
mod cue;
#[cfg(any(test, feature = "disc"))]
mod disc;
#[cfg(any(test, feature = "disc"))]
mod folder;
#[cfg(any(test, feature = "disc"))]
mod iso;
#[cfg(any(test, feature = "disc"))]
mod picked;
#[cfg(test)]
mod tests;
#[cfg(any(test, feature = "disc"))]
mod tracks;

#[cfg(any(test, feature = "disc"))]
pub use check::{Expected, Finding, Picked, check};
#[cfg(any(test, feature = "disc"))]
pub use disc::{Disc, OpenError, Source};
use kernel_files::Error;
#[cfg(any(test, feature = "disc"))]
pub use tracks::{Audio, Encoded, Pcm, Track};

/// A game's disc facts, as its rows crate keeps them: what its finder
/// looks for, what its music device asks. No game is named here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    /// The file at the disc's root the game's finder looks for.
    pub marker: &'static str,
    /// The audio tracks its disc holds.
    pub tracks: u8,
    /// The time the game sets on its music device.
    pub time: Time,
    /// A store folder's track files, if the game ships as one.
    pub store: Option<TrackFiles>,
}

/// A music device's time: TMSF counts within a track; MSF counts from the
/// disc's start, its 2-second lead-in with it (`mmsystem.h`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Time {
    Tmsf,
    Msf,
}

/// A store folder's naming rule for its track files:
/// `<stem><nn><extension>`, two digits, from `first` on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrackFiles {
    pub stem: &'static str,
    pub first: u8,
    pub extension: &'static str,
    pub format: Format,
}

/// An encoded track's form, decoded by a library, never by this crate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// Ogg Vorbis (RFC 3533, Xiph's Vorbis I).
    Ogg,
}

/// One source file's bytes by range: the disc's only way to them.
pub trait ReadAt {
    /// The file's length in bytes.
    fn size(&self) -> u64;
    /// `into.len()` bytes from `at`. A range past the end fails
    /// `BadArgument`; a target's own failure passes through as `Other`.
    fn read_at(&self, at: u64, into: &mut [u8]) -> Result<(), Error>;
}

/// The memory back: the tests' bytes, or a page's already in hand.
impl ReadAt for [u8] {
    fn size(&self) -> u64 {
        self.len() as u64
    }

    fn read_at(&self, at: u64, into: &mut [u8]) -> Result<(), Error> {
        let from = usize::try_from(at).map_err(|_| Error::BadArgument)?;
        let bytes = from
            .checked_add(into.len())
            .and_then(|end| self.get(from..end))
            .ok_or(Error::BadArgument)?;
        into.copy_from_slice(bytes);
        Ok(())
    }
}

#[cfg(any(test, feature = "disc"))]
impl ReadAt for alloc::vec::Vec<u8> {
    fn size(&self) -> u64 {
        self.as_slice().size()
    }

    fn read_at(&self, at: u64, into: &mut [u8]) -> Result<(), Error> {
        self.as_slice().read_at(at, into)
    }
}
