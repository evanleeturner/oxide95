// SPDX-License-Identifier: MIT
//! The disc as a [`Folder`]: the data track's files or a picked folder's, read
//! only, and never read whole. Its rules, the memory folder's where a disc
//! answers the same:
//!
//! - **Names** compare by `kernel_files::names`' one rule, from the
//!   volume's top.
//! - **A name in a folder the volume lacks** fails `NoPath`; a folder
//!   opened as a file `Denied`; a file not there `NotFound`.
//! - **Nothing on a disc changes:** an open to write, to make a file or to
//!   empty one fails `Denied` (one that must make a file found there
//!   `Exists`, as the memory folder's); a write `Denied`; a delete and a
//!   move `Denied`, their names never looked at.
//! - **A find** matches `*` and `?` in its last part against the folder's
//!   files and its folders, as Windows' finds answer both, in key order;
//!   past the last name `NoMore`; one that matches none `NotFound`. The
//!   memory folder's finds take files alone.
//! - **Handles** and finds count from 4 by 4, as Windows' do.
//! - **A read** takes the file's own sectors, one range a sector; a picked
//!   file's own bytes, one range.
//! - **A seek** before the start fails `BadSeek`; past the end it stands,
//!   and a read there moves nothing.
//! - **Every call** writes one `disc.call` line: `c` the
//!   call, `a` the handle or find it was given, `p` the name or pattern,
//!   `v` its answer, `n` the bytes a read moved, `e` its failure or `ok`.
//!   `fail` and `quiet` are the front's bookkeeping, no call on the disc,
//!   and write none.
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::cell::{Cell, RefCell};
use core::ffi::CStr;
use core::fmt;

use kernel_files::names::{last, matches, parent};
use kernel_files::{Create, Error, Folder, Kind, Mode, Origin, Text};

use crate::ReadAt;
use crate::disc::Disc;
use crate::iso::{Entry, key};

/// An open file: its key, its position, whether it was opened to read.
struct Open {
    key: Vec<u8>,
    at: u32,
    read: bool,
}

/// A find's names, and the next one to answer.
struct Find {
    names: Vec<Vec<u8>>,
    next: usize,
}

/// The disc's open files and finds.
#[derive(Default)]
pub(crate) struct Handles {
    open: RefCell<BTreeMap<u32, Open>>,
    finds: RefCell<BTreeMap<u32, Find>>,
    last_handle: Cell<u32>,
}

impl Handles {
    fn next(&self) -> u32 {
        let handle = self.last_handle.get() + 4;
        self.last_handle.set(handle);
        handle
    }

    fn is_open(&self, file: u32) -> bool {
        self.open.borrow().contains_key(&file)
    }
}

impl<R: ReadAt> Disc<R> {
    /// The calling thread's last error, the slot every back shares.
    #[must_use]
    pub fn last_error(&self) -> u32 {
        kernel_last_error::get()
    }

    fn is_folder(&self, key: &[u8]) -> bool {
        key.is_empty() || self.entry(key).is_some_and(Entry::is_folder)
    }

    /// `name`'s key, once its folder is on the volume and it names no
    /// folder.
    fn file_key(&self, name: &[u8]) -> Result<Vec<u8>, Error> {
        let key = key(name);
        if !self.is_folder(parent(&key)) {
            return Err(Error::NoPath);
        }
        if self.is_folder(&key) {
            return Err(Error::Denied);
        }
        Ok(key)
    }

    fn opened(&self, name: &[u8], mode: Mode) -> Result<u32, Error> {
        let key = self.file_key(name)?;
        let there = self.entry(&key).is_some();
        match (mode.create, there) {
            (Create::Existing | Create::OpenAlways, true) if !mode.write => {}
            (Create::New, true) => return Err(Error::Exists),
            (Create::Existing | Create::Truncate, false) => return Err(Error::NotFound),
            _ => return Err(Error::Denied),
        }
        let handle = self.handles.next();
        let open = Open {
            key,
            at: 0,
            read: mode.read,
        };
        self.handles.open.borrow_mut().insert(handle, open);
        Ok(handle)
    }

    fn read_open(&self, file: u32, into: &mut [u8]) -> Result<u32, Error> {
        let mut open = self.handles.open.borrow_mut();
        let open = open.get_mut(&file).ok_or(Error::BadHandle)?;
        if !open.read {
            return Err(Error::Denied);
        }
        // An open file's entry stays: nothing on a disc changes.
        let entry = self.entry(&open.key).ok_or(Error::BadHandle)?;
        let count = self.read(entry, u64::from(open.at), into)?;
        let count = u32::try_from(count).map_err(|_| Error::BadArgument)?;
        open.at += count;
        Ok(count)
    }

    fn sought(&self, file: u32, distance: i32, origin: Origin) -> Result<u32, Error> {
        let mut open = self.handles.open.borrow_mut();
        let open = open.get_mut(&file).ok_or(Error::BadHandle)?;
        let base = match origin {
            Origin::Start => 0,
            Origin::Here => i64::from(open.at),
            Origin::End => self
                .entry(&open.key)
                .map_or(0, |entry| i64::from(entry.size)),
        };
        let at = u32::try_from(base + i64::from(distance)).map_err(|_| Error::BadSeek)?;
        open.at = at;
        Ok(at)
    }

    fn found(&self, pattern: &[u8], name: &mut [u8]) -> Result<u32, Error> {
        let key = key(pattern);
        let folder = parent(&key);
        if !self.is_folder(folder) {
            return Err(Error::NoPath);
        }
        let names: Vec<Vec<u8>> = self
            .within(folder)
            .filter(|(each, _)| matches(last(&key), last(each)))
            .map(|(_, entry)| entry.name.clone())
            .collect();
        let first = names.first().ok_or(Error::NotFound)?;
        put(first, name);
        let find = self.handles.next();
        self.handles
            .finds
            .borrow_mut()
            .insert(find, Find { names, next: 1 });
        Ok(find)
    }

    fn found_next(&self, find: u32, name: &mut [u8]) -> Result<(), Error> {
        let mut finds = self.handles.finds.borrow_mut();
        let find = finds.get_mut(&find).ok_or(Error::BadHandle)?;
        let found = find.names.get(find.next).ok_or(Error::NoMore)?;
        put(found, name);
        find.next += 1;
        Ok(())
    }
}

impl<R: ReadAt> Folder for Disc<R> {
    fn open(&self, name: &CStr, mode: Mode) -> Result<u32, Error> {
        let answer = self.opened(name.to_bytes(), mode);
        line("open", 0, name.to_bytes(), answer, 0);
        answer
    }

    fn read(&self, file: u32, into: &mut [u8]) -> Result<u32, Error> {
        let answer = self.read_open(file, into);
        line("read", file, b"", answer, answer.unwrap_or(0));
        answer
    }

    fn write(&self, file: u32, _from: &[u8]) -> Result<u32, Error> {
        let answer = Err(if self.handles.is_open(file) {
            Error::Denied
        } else {
            Error::BadHandle
        });
        line("write", file, b"", answer, 0);
        answer
    }

    fn seek(&self, file: u32, distance: i32, origin: Origin) -> Result<u32, Error> {
        let answer = self.sought(file, distance, origin);
        line("seek", file, b"", answer, 0);
        answer
    }

    fn kind(&self, file: u32) -> Result<Kind, Error> {
        let answer = if self.handles.is_open(file) {
            Ok(Kind::Disk)
        } else {
            Err(Error::BadHandle)
        };
        line("kind", file, b"", answer.map(|kind| kind as u32), 0);
        answer
    }

    fn close(&self, file: u32) -> Result<(), Error> {
        let answer = self
            .handles
            .open
            .borrow_mut()
            .remove(&file)
            .map(|_| ())
            .ok_or(Error::BadHandle);
        line("close", file, b"", answer.map(|()| 0), 0);
        answer
    }

    fn delete(&self, name: &CStr) -> Result<(), Error> {
        let answer = Err(Error::Denied);
        line("delete", 0, name.to_bytes(), answer.map(|()| 0), 0);
        answer
    }

    fn rename(&self, from: &CStr, _to: &CStr) -> Result<(), Error> {
        let answer = Err(Error::Denied);
        line("rename", 0, from.to_bytes(), answer.map(|()| 0), 0);
        answer
    }

    fn find_first(&self, pattern: &CStr, name: &mut [u8]) -> Result<u32, Error> {
        let answer = self.found(pattern.to_bytes(), name);
        line("find_first", 0, pattern.to_bytes(), answer, 0);
        answer
    }

    fn find_next(&self, find: u32, name: &mut [u8]) -> Result<(), Error> {
        let answer = self.found_next(find, name);
        line("find_next", find, b"", answer.map(|()| 0), 0);
        answer
    }

    fn find_close(&self, find: u32) -> Result<(), Error> {
        let answer = self
            .handles
            .finds
            .borrow_mut()
            .remove(&find)
            .map(|_| ())
            .ok_or(Error::BadHandle);
        line("find_close", find, b"", answer.map(|()| 0), 0);
        answer
    }

    fn fail(&self, code: u32) {
        kernel_last_error::set(code);
    }
}

/// One call's `disc.call` line.
pub(crate) fn line(
    call: &'static str,
    given: u32,
    name: &[u8],
    answer: Result<u32, Error>,
    moved: u32,
) {
    let (value, failed) = match answer {
        Ok(value) => (value, None),
        Err(error) => (0, Some(error)),
    };
    log::debug!(
        c = call, a:% = Hex(given), p:% = Text(name), v:% = Hex(value),
        n:% = Hex(moved), e:% = Outcome(failed); "disc.call"
    );
}

/// A number as the line shows it, in hex.
struct Hex(u32);

impl fmt::Display for Hex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:x}", self.0)
    }
}

/// A call's failure as the line shows it: its name, or `ok`.
struct Outcome(Option<Error>);

impl fmt::Display for Outcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            None => f.write_str("ok"),
            Some(error) => write!(f, "{error:?}"),
        }
    }
}

/// `found` into `name`, cut to its room, with its NUL.
fn put(found: &[u8], name: &mut [u8]) {
    let count = found.len().min(name.len().saturating_sub(1));
    name[..count].copy_from_slice(&found[..count]);
    if let Some(end) = name.get_mut(count) {
        *end = 0;
    }
}
