// SPDX-License-Identifier: MIT
//! A program's file calls: open, read, write, seek, the
//! file's type, close, delete, move and find, written once. Each file the
//! seam serves is a [`table::Row`], kept in its game's rows crate; where
//! its bytes live is a backend behind [`Folder`]; [`Files`] reads the
//! call's numbers through the row,
//! asks the folder in plain terms, answers in the shell's numbers and
//! writes one `imp.call` line a call: `c` the row's name, `a` the call's
//! numbers, `v` its answer, `p` and `to` the names given as text, `n` the
//! bytes moved, `found` the name found, `e` the error left (0 on success).
//!
//! The level is the import: the calling program's own C library makes
//! every call, so the front answers only what that library reads back:
//! the handle, or the failure's; the bytes and
//! their count; the position; the file's type; the find data's name; the
//! last error after a failure. What it never reads is never carried: the
//! security and template an open passes, a seek's high half (its one
//! caller, the C library's `__lseek_lk`, passes null), a read's overlap, and
//! every find-data field but the name, which the front clears.
//!
//! The crate names nothing of any one target, so it builds for
//! `wasm32-unknown-unknown`; the Windows folder is a game's own backend.
//! The in-memory folder, [`memory`], is the host tests' and a
//! page's: its given half as given, its written half its own. Names
//! compare by one rule, [`names`], every folder's.
#![cfg_attr(not(test), no_std)]

#[cfg(any(test, feature = "memory"))]
extern crate alloc;

#[cfg(any(test, feature = "memory"))]
pub mod memory;
pub mod names;
pub mod table;
#[cfg(test)]
mod tests;

use core::ffi::CStr;
use core::fmt;
use oxide_trace::{Args, Shown};
use table::Row;

/// What an open does when the file is there, or is not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Create {
    /// Make it; fail if it is there.
    New,
    /// Make it empty, there or not.
    Always,
    /// Open it; fail if it is not there.
    Existing,
    /// Open it, made empty first if it is not there.
    OpenAlways,
    /// Empty it; fail if it is not there.
    Truncate,
}

const CREATES: [Create; 5] = [
    Create::New,
    Create::Always,
    Create::Existing,
    Create::OpenAlways,
    Create::Truncate,
];

/// What an open asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mode {
    pub read: bool,
    pub write: bool,
    pub create: Create,
}

/// Where a seek counts from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    Start,
    Here,
    End,
}

const ORIGINS: [Origin; 3] = [Origin::Start, Origin::Here, Origin::End];

/// A file's type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Disk,
    Char,
    Pipe,
}

const KINDS: [Kind; 3] = [Kind::Disk, Kind::Char, Kind::Pipe];

/// The way back: a plain ask in this file's own numbers, for a folder
/// that calls the file's real imports.
impl Row {
    /// An open's access bits.
    #[must_use]
    pub const fn access(&self, mode: Mode) -> u32 {
        let read = if mode.read { self.read } else { 0 };
        let write = if mode.write { self.write } else { 0 };
        read | write
    }

    #[must_use]
    pub const fn disposition(&self, create: Create) -> u32 {
        self.creates[create as usize]
    }

    #[must_use]
    pub const fn method(&self, origin: Origin) -> u32 {
        self.methods[origin as usize]
    }

    /// A file type the import answered, if the row knows it.
    #[must_use]
    pub fn kind(&self, number: u32) -> Option<Kind> {
        let at = self.kinds.iter().position(|&each| each == number)?;
        Some(KINDS[at])
    }
}

/// Why a call failed. A target's own code passes through as `Other`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    NotFound,
    NoPath,
    Denied,
    BadHandle,
    /// A find has no more names.
    NoMore,
    /// An open that must make the file found it there.
    Exists,
    /// The file is open.
    Busy,
    /// A move's new name is taken.
    Taken,
    /// A seek before the start.
    BadSeek,
    BadArgument,
    Other(u32),
}

impl Error {
    /// The last error the shell reads for this failure.
    #[must_use]
    pub const fn code(self, row: &Row) -> u32 {
        let codes = &row.errors;
        match self {
            Error::NotFound => codes.not_found,
            Error::NoPath => codes.no_path,
            Error::Denied => codes.denied,
            Error::BadHandle => codes.bad_handle,
            Error::NoMore => codes.no_more,
            Error::Exists => codes.exists,
            Error::Busy => codes.busy,
            Error::Taken => codes.taken,
            Error::BadSeek => codes.bad_seek,
            Error::BadArgument => codes.bad_argument,
            Error::Other(code) => code,
        }
    }
}

/// Where one file's files live: the seam between the calls and a target.
/// A handle or a find is the folder's own number; names come as the shell
/// gave them.
pub trait Folder {
    fn open(&self, name: &CStr, mode: Mode) -> Result<u32, Error>;
    /// Bytes from the position on, into `into`; the count moved.
    fn read(&self, file: u32, into: &mut [u8]) -> Result<u32, Error>;
    /// `from` at the position; the count moved.
    fn write(&self, file: u32, from: &[u8]) -> Result<u32, Error>;
    /// The new position.
    fn seek(&self, file: u32, distance: i32, origin: Origin) -> Result<u32, Error>;
    fn kind(&self, file: u32) -> Result<Kind, Error>;
    fn close(&self, file: u32) -> Result<(), Error>;
    fn delete(&self, name: &CStr) -> Result<(), Error>;
    fn rename(&self, from: &CStr, to: &CStr) -> Result<(), Error>;
    /// The first name `pattern` matches, into `name` with its NUL; the find.
    fn find_first(&self, pattern: &CStr, name: &mut [u8]) -> Result<u32, Error>;
    /// The find's next name, into `name` with its NUL.
    fn find_next(&self, find: u32, name: &mut [u8]) -> Result<(), Error>;
    fn find_close(&self, find: u32) -> Result<(), Error>;
    /// Keeps `code` as the last error the shell reads after a failure.
    fn fail(&self, code: u32);
    /// Writes one line around whatever the target must keep across it.
    fn quiet(&self, line: impl FnOnce()) {
        line();
    }
}

/// A name as the line shows it: its bytes to the first NUL, a byte
/// outside printable ASCII as `\xNN`.
pub struct Text<'a>(pub &'a [u8]);

impl fmt::Display for Text<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for &byte in self.0.iter().take_while(|&&byte| byte != 0) {
            match byte {
                b' '..=b'~' => fmt::Write::write_char(f, char::from(byte))?,
                _ => write!(f, "\\x{byte:02x}")?,
            }
        }
        Ok(())
    }
}

/// One file's file calls: its row, and its folder.
pub struct Files<F> {
    file: &'static Row,
    folder: F,
}

impl<F: Folder> Files<F> {
    pub const fn new(file: &'static Row, folder: F) -> Self {
        Self { file, folder }
    }

    /// The folder the calls ask.
    pub const fn folder(&self) -> &F {
        &self.folder
    }

    /// An open: the handle, or the row's invalid one.
    pub fn open(&self, name: &CStr, access: u32, share: u32, disposition: u32, flags: u32) -> u32 {
        let row = self.file;
        let answer = match row.creates.iter().position(|&each| each == disposition) {
            Some(at) => self.folder.open(
                name,
                Mode {
                    read: access & row.read != 0,
                    write: access & row.write != 0,
                    create: CREATES[at],
                },
            ),
            None => Err(Error::BadArgument),
        };
        let (value, error) = self.settle(answer, row.invalid);
        let arguments =
            [&access, &share, &disposition, &flags].map(|each| oxide_trace::__shown!(*each));
        self.folder.quiet(|| {
            log::debug!(
                c = row.calls.open, a:% = Args(arguments), v:% = oxide_trace::__shown!(value),
                p:% = Text(name.to_bytes()), e:% = oxide_trace::__shown!(error),
                by = "kernel-files"; "imp.call"
            );
        });
        value
    }

    /// A read: 1 and the count into `read`, or 0 and a count of 0.
    pub fn read(&self, file: u32, into: &mut [u8], read: Option<&mut u32>) -> i32 {
        let asked = into.len();
        let answer = self.folder.read(file, into);
        self.moved(self.file.calls.read, file, asked, answer, read)
    }

    /// A write: 1 and the count into `written`, or 0 and a count of 0.
    pub fn write(&self, file: u32, from: &[u8], written: Option<&mut u32>) -> i32 {
        let answer = self.folder.write(file, from);
        self.moved(self.file.calls.write, file, from.len(), answer, written)
    }

    /// A seek: the new position, or the row's invalid one.
    pub fn seek(&self, file: u32, distance: i32, method: u32) -> u32 {
        let row = self.file;
        let answer = match row.methods.iter().position(|&each| each == method) {
            Some(at) => self.folder.seek(file, distance, ORIGINS[at]),
            None => Err(Error::BadArgument),
        };
        let (value, error) = self.settle(answer, row.invalid);
        let arguments = [
            oxide_trace::__shown!(file),
            oxide_trace::__shown!(distance),
            oxide_trace::__shown!(method),
        ];
        self.handled(row.calls.seek, arguments, &value, error);
        value
    }

    /// The file's type, or the row's unknown one.
    pub fn kind(&self, file: u32) -> u32 {
        let row = self.file;
        let answer = self.folder.kind(file).map(|kind| {
            let at = KINDS
                .iter()
                .position(|&each| each == kind)
                .unwrap_or_default();
            row.kinds[at]
        });
        let (value, error) = self.settle(answer, row.unknown);
        self.handled(row.calls.kind, [oxide_trace::__shown!(file)], &value, error);
        value
    }

    pub fn close(&self, file: u32) -> i32 {
        let (value, error) = self.done(self.folder.close(file));
        self.handled(
            self.file.calls.close,
            [oxide_trace::__shown!(file)],
            &value,
            error,
        );
        value
    }

    pub fn delete(&self, name: &CStr) -> i32 {
        let (value, error) = self.done(self.folder.delete(name));
        self.folder.quiet(|| {
            log::debug!(
                c = self.file.calls.delete, v:% = oxide_trace::__shown!(value),
                p:% = Text(name.to_bytes()), e:% = oxide_trace::__shown!(error),
                by = "kernel-files"; "imp.call"
            );
        });
        value
    }

    pub fn rename(&self, from: &CStr, to: &CStr) -> i32 {
        let (value, error) = self.done(self.folder.rename(from, to));
        self.folder.quiet(|| {
            log::debug!(
                c = self.file.calls.rename, v:% = oxide_trace::__shown!(value),
                p:% = Text(from.to_bytes()), to:% = Text(to.to_bytes()),
                e:% = oxide_trace::__shown!(error), by = "kernel-files"; "imp.call"
            );
        });
        value
    }

    /// A find's start: the find, or the row's invalid one. `data` is the
    /// shell's find data, cleared but for the name the folder writes.
    pub fn find_first(&self, pattern: &CStr, data: &mut [u8]) -> u32 {
        let row = self.file;
        data.fill(0);
        let name = &mut data[row.name_at..row.name_at + row.name_bytes];
        let answer = self.folder.find_first(pattern, name);
        let (value, error) = self.settle(answer, row.invalid);
        self.folder.quiet(|| {
            log::debug!(
                c = row.calls.find_first, v:% = oxide_trace::__shown!(value),
                p:% = Text(pattern.to_bytes()), found:% = Text(name),
                e:% = oxide_trace::__shown!(error), by = "kernel-files"; "imp.call"
            );
        });
        value
    }

    /// A find's next name: 1, or 0 when there is none.
    pub fn find_next(&self, find: u32, data: &mut [u8]) -> i32 {
        let row = self.file;
        data.fill(0);
        let name = &mut data[row.name_at..row.name_at + row.name_bytes];
        let (value, error) = self.done(self.folder.find_next(find, name));
        self.folder.quiet(|| {
            log::debug!(
                c = row.calls.find_next, a:% = oxide_trace::__shown!(find),
                v:% = oxide_trace::__shown!(value), found:% = Text(name),
                e:% = oxide_trace::__shown!(error), by = "kernel-files"; "imp.call"
            );
        });
        value
    }

    pub fn find_close(&self, find: u32) -> i32 {
        let (value, error) = self.done(self.folder.find_close(find));
        self.handled(
            self.file.calls.find_close,
            [oxide_trace::__shown!(find)],
            &value,
            error,
        );
        value
    }

    /// A read's or write's answer: the count into the shell's out-pointer.
    fn moved(
        &self,
        name: &'static str,
        file: u32,
        asked: usize,
        answer: Result<u32, Error>,
        count: Option<&mut u32>,
    ) -> i32 {
        let moved = *answer.as_ref().unwrap_or(&0);
        let (value, error) = self.done(answer.map(|_| ()));
        if let Some(count) = count {
            *count = moved;
        }
        let arguments = [oxide_trace::__shown!(file), oxide_trace::__shown!(asked)];
        self.folder.quiet(|| {
            log::debug!(
                c = name, a:% = Args(arguments), v:% = oxide_trace::__shown!(value),
                n:% = oxide_trace::__shown!(moved), e:% = oxide_trace::__shown!(error),
                by = "kernel-files"; "imp.call"
            );
        });
        value
    }

    /// A handle's call, its line.
    fn handled<const N: usize>(
        &self,
        name: &'static str,
        arguments: [Shown<'_>; N],
        value: &impl oxide_trace::Value,
        error: u32,
    ) {
        self.folder.quiet(|| {
            log::debug!(
                c = name, a:% = Args(arguments), v:% = oxide_trace::__shown!(*value),
                e:% = oxide_trace::__shown!(error), by = "kernel-files"; "imp.call"
            );
        });
    }

    /// A number's answer: the number, or `failed` and the error kept.
    fn settle(&self, answer: Result<u32, Error>, failed: u32) -> (u32, u32) {
        match answer {
            Ok(value) => (value, 0),
            Err(error) => (failed, self.failure(error)),
        }
    }

    /// A yes-or-no answer: 1, or 0 and the error kept.
    fn done(&self, answer: Result<(), Error>) -> (i32, u32) {
        match answer {
            Ok(()) => (1, 0),
            Err(error) => (0, self.failure(error)),
        }
    }

    fn failure(&self, error: Error) -> u32 {
        let code = error.code(self.file);
        self.folder.fail(code);
        code
    }
}
