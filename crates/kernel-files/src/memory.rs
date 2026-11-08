// SPDX-License-Identifier: MIT
//! The in-memory folder: the host tests' backend, and a page's. Two
//! halves: the given files, the disc's and the install's, never changed;
//! and the folder's own, every file the shell makes, writes, deletes or
//! moves. A write-open of a given file copies it into the own half first;
//! a delete of one leaves a mark there.
//!
//! Its rules, a1's or the body's own:
//!
//! - **Names** match as Windows matches them: ASCII case folded, `/` read
//!   as `\` (a1 deletes a name in capitals, then opens it in small
//!   letters). A folder is given, or holds a given file; the top folder
//!   always is.
//! - **A name in a folder never given** fails `NoPath` (a1: 3 calls); a
//!   folder opened or deleted as a file `Denied`.
//! - **A find** matches `*` and `?` in its last part, against files alone,
//!   in key order; past the last name `NoMore` (a1: 0x12). One that matches
//!   none fails `NotFound`, as Windows answers: a1 holds none.
//! - **Handles** and finds count from 4 by 4, as Windows' do.
//! - **A delete or move of an open file** fails `Busy`; a move onto a
//!   taken name `Taken`.
//! - **A read** on a handle opened to write alone fails `Denied`, and a
//!   write on one opened to read alone.
//! - **A seek** before the start fails `BadSeek`; past the end it stands,
//!   and a write there fills the gap with zeros.
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;
use core::cell::{Cell, RefCell};
use core::ffi::CStr;

use crate::names::{self, last, matches, parent};
use crate::{Create, Error, Folder, Kind, Mode, Origin};

/// A file's name as it was given or made, and its bytes.
#[derive(Clone)]
struct File {
    name: Vec<u8>,
    bytes: Vec<u8>,
}

/// An open file: its key, its position, what it was opened for.
struct Open {
    key: Vec<u8>,
    at: u32,
    read: bool,
    write: bool,
}

/// A find's names, and the next one to answer.
struct Find {
    names: Vec<Vec<u8>>,
    next: usize,
}

/// A folder in memory.
#[derive(Default)]
pub struct MemoryFolder {
    given: BTreeMap<Vec<u8>, File>,
    folders: BTreeSet<Vec<u8>>,
    /// The own half: a file made or changed, or `None` over a given file
    /// deleted or moved away.
    own: RefCell<BTreeMap<Vec<u8>, Option<File>>>,
    open: RefCell<BTreeMap<u32, Open>>,
    finds: RefCell<BTreeMap<u32, Find>>,
    last_handle: Cell<u32>,
}

impl MemoryFolder {
    /// The folder with `name` given, holding `bytes`, its folders with it.
    #[must_use]
    pub fn give(mut self, name: &str, bytes: &[u8]) -> Self {
        let key = key(name.as_bytes());
        self.give_parents(&key);
        let file = File {
            name: last(name.as_bytes()).to_vec(),
            bytes: bytes.to_vec(),
        };
        self.given.insert(key, file);
        self
    }

    /// The folder with the empty folder `name` given, its folders with it.
    #[must_use]
    pub fn give_folder(mut self, name: &str) -> Self {
        let key = key(name.as_bytes());
        self.give_parents(&key);
        self.folders.insert(key);
        self
    }

    /// A file's bytes as the folder holds them now: the own half's, else
    /// the given half's.
    #[must_use]
    pub fn contents(&self, name: &str) -> Option<Vec<u8>> {
        self.with_file(&key(name.as_bytes()), |file| {
            file.map(|file| file.bytes.clone())
        })
    }

    /// The calling thread's last error, the slot every back shares.
    #[must_use]
    pub fn last_error(&self) -> u32 {
        kernel_last_error::get()
    }

    fn give_parents(&mut self, key: &[u8]) {
        let mut folder = parent(key);
        while !folder.is_empty() {
            self.folders.insert(folder.to_vec());
            folder = parent(folder);
        }
    }

    fn has_folder(&self, folder: &[u8]) -> bool {
        folder.is_empty() || self.folders.contains(folder)
    }

    /// The file at `key` as the folder holds it now, handed to `then`.
    fn with_file<T>(&self, key: &[u8], then: impl FnOnce(Option<&File>) -> T) -> T {
        let own = self.own.borrow();
        match own.get(key) {
            Some(made) => then(made.as_ref()),
            None => then(self.given.get(key)),
        }
    }

    fn exists(&self, key: &[u8]) -> bool {
        self.with_file(key, |file| file.is_some())
    }

    fn is_open(&self, key: &[u8]) -> bool {
        self.open.borrow().values().any(|open| open.key == key)
    }

    /// `name`'s key, once its folder is there and it names no folder.
    fn file_key(&self, name: &CStr) -> Result<Vec<u8>, Error> {
        let key = key(name.to_bytes());
        if !self.has_folder(parent(&key)) {
            return Err(Error::NoPath);
        }
        if self.folders.contains(&key) {
            return Err(Error::Denied);
        }
        Ok(key)
    }

    /// The file at `key` gone from the folder: a mark over a given one.
    fn remove(&self, key: &[u8]) {
        let mut own = self.own.borrow_mut();
        if self.given.contains_key(key) {
            own.insert(key.to_vec(), None);
        } else {
            own.remove(key);
        }
    }

    fn handle(&self) -> u32 {
        let handle = self.last_handle.get() + 4;
        self.last_handle.set(handle);
        handle
    }

    /// The names of the files in `folder` whose last part `pattern`
    /// matches, in key order.
    fn found(&self, folder: &[u8], pattern: &[u8]) -> Vec<Vec<u8>> {
        let own = self.own.borrow();
        let given = self.given.iter().filter(|(key, _)| !own.contains_key(*key));
        let made = own
            .iter()
            .filter_map(|(key, file)| Some((key, file.as_ref()?)));
        let mut names: BTreeMap<&Vec<u8>, &File> = given.collect();
        names.extend(made);
        names
            .into_iter()
            .filter(|(key, _)| parent(key) == folder && matches(pattern, last(key)))
            .map(|(_, file)| file.name.clone())
            .collect()
    }
}

impl Folder for MemoryFolder {
    fn open(&self, name: &CStr, mode: Mode) -> Result<u32, Error> {
        let key = self.file_key(name)?;
        let there = self.exists(&key);
        match (mode.create, there) {
            (Create::New, true) => return Err(Error::Exists),
            (Create::Existing | Create::Truncate, false) => return Err(Error::NotFound),
            _ => {}
        }
        let emptied = matches!(mode.create, Create::New | Create::Always | Create::Truncate);
        if emptied || !there {
            let name = self.with_file(&key, |file| {
                file.map_or_else(|| last(name.to_bytes()).to_vec(), |file| file.name.clone())
            });
            let file = File {
                name,
                bytes: Vec::new(),
            };
            self.own.borrow_mut().insert(key.clone(), Some(file));
        } else if mode.write && !self.own.borrow().contains_key(&key) {
            let copy = self.given.get(&key).cloned();
            self.own.borrow_mut().insert(key.clone(), copy);
        }
        let handle = self.handle();
        let open = Open {
            key,
            at: 0,
            read: mode.read,
            write: mode.write,
        };
        self.open.borrow_mut().insert(handle, open);
        Ok(handle)
    }

    fn read(&self, file: u32, into: &mut [u8]) -> Result<u32, Error> {
        let mut open = self.open.borrow_mut();
        let open = open.get_mut(&file).ok_or(Error::BadHandle)?;
        if !open.read {
            return Err(Error::Denied);
        }
        let count = self.with_file(&open.key, |file| {
            let bytes = file.map_or(&[][..], |file| &file.bytes);
            let from = usize::try_from(open.at).map_or(bytes.len(), |at| at.min(bytes.len()));
            let count = into.len().min(bytes.len() - from);
            into[..count].copy_from_slice(&bytes[from..from + count]);
            count
        });
        let count = u32::try_from(count).map_err(|_| Error::BadArgument)?;
        open.at += count;
        Ok(count)
    }

    fn write(&self, file: u32, from: &[u8]) -> Result<u32, Error> {
        let mut open = self.open.borrow_mut();
        let open = open.get_mut(&file).ok_or(Error::BadHandle)?;
        if !open.write {
            return Err(Error::Denied);
        }
        let count = u32::try_from(from.len()).map_err(|_| Error::BadArgument)?;
        let end = open.at.checked_add(count).ok_or(Error::BadArgument)?;
        let mut own = self.own.borrow_mut();
        // A write-open put the file in the own half, and no delete or move
        // takes an open one.
        let made = own
            .get_mut(&open.key)
            .and_then(Option::as_mut)
            .ok_or(Error::BadHandle)?;
        let (at, end) = (open.at as usize, end as usize);
        if made.bytes.len() < end {
            made.bytes.resize(end, 0);
        }
        made.bytes[at..end].copy_from_slice(from);
        open.at += count;
        Ok(count)
    }

    fn seek(&self, file: u32, distance: i32, origin: Origin) -> Result<u32, Error> {
        let mut open = self.open.borrow_mut();
        let open = open.get_mut(&file).ok_or(Error::BadHandle)?;
        let base = match origin {
            Origin::Start => 0,
            Origin::Here => i64::from(open.at),
            Origin::End => self.with_file(&open.key, |file| {
                file.map_or(0, |file| {
                    i64::try_from(file.bytes.len()).unwrap_or(i64::MAX)
                })
            }),
        };
        let at = u32::try_from(base + i64::from(distance)).map_err(|_| Error::BadSeek)?;
        open.at = at;
        Ok(at)
    }

    fn kind(&self, file: u32) -> Result<Kind, Error> {
        if self.open.borrow().contains_key(&file) {
            Ok(Kind::Disk)
        } else {
            Err(Error::BadHandle)
        }
    }

    fn close(&self, file: u32) -> Result<(), Error> {
        self.open
            .borrow_mut()
            .remove(&file)
            .map(|_| ())
            .ok_or(Error::BadHandle)
    }

    fn delete(&self, name: &CStr) -> Result<(), Error> {
        let key = self.file_key(name)?;
        if !self.exists(&key) {
            return Err(Error::NotFound);
        }
        if self.is_open(&key) {
            return Err(Error::Busy);
        }
        self.remove(&key);
        Ok(())
    }

    fn rename(&self, from: &CStr, to: &CStr) -> Result<(), Error> {
        let from_key = self.file_key(from)?;
        let to_key = self.file_key(to)?;
        let Some(bytes) = self.with_file(&from_key, |file| file.map(|file| file.bytes.clone()))
        else {
            return Err(Error::NotFound);
        };
        if self.exists(&to_key) {
            return Err(Error::Taken);
        }
        if self.is_open(&from_key) {
            return Err(Error::Busy);
        }
        self.remove(&from_key);
        let file = File {
            name: last(to.to_bytes()).to_vec(),
            bytes,
        };
        self.own.borrow_mut().insert(to_key, Some(file));
        Ok(())
    }

    fn find_first(&self, pattern: &CStr, name: &mut [u8]) -> Result<u32, Error> {
        let key = key(pattern.to_bytes());
        let folder = parent(&key);
        if !self.has_folder(folder) {
            return Err(Error::NoPath);
        }
        let names = self.found(folder, last(&key));
        let first = names.first().ok_or(Error::NotFound)?;
        put(first, name);
        let find = self.handle();
        self.finds
            .borrow_mut()
            .insert(find, Find { names, next: 1 });
        Ok(find)
    }

    fn find_next(&self, find: u32, name: &mut [u8]) -> Result<(), Error> {
        let mut finds = self.finds.borrow_mut();
        let find = finds.get_mut(&find).ok_or(Error::BadHandle)?;
        let found = find.names.get(find.next).ok_or(Error::NoMore)?;
        put(found, name);
        find.next += 1;
        Ok(())
    }

    fn find_close(&self, find: u32) -> Result<(), Error> {
        self.finds
            .borrow_mut()
            .remove(&find)
            .map(|_| ())
            .ok_or(Error::BadHandle)
    }

    fn fail(&self, code: u32) {
        kernel_last_error::set(code);
    }
}

/// A name's key: each byte folded as names compare.
fn key(name: &[u8]) -> Vec<u8> {
    name.iter().map(|&byte| names::fold(byte)).collect()
}

/// `found` into `name`, cut to its room, with its NUL.
fn put(found: &[u8], name: &mut [u8]) {
    let count = found.len().min(name.len().saturating_sub(1));
    name[..count].copy_from_slice(&found[..count]);
    if let Some(end) = name.get_mut(count) {
        *end = 0;
    }
}
