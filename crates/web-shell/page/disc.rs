// SPDX-License-Identifier: MIT
//! The thin wasm the disc's Worker runs. [`read`] opens the player's pick as a disc, the
//! one picked image or a mounted folder's files by their paths from the disc's top, and
//! reads one byte at each place the page names, through `logical_disc` alone: the read
//! the launcher's kept copy stands on. [`check`] holds a mounted folder's pick against
//! the files the page says the game expects, and hands out one sentence a wrong file.
//! Only a Worker can answer its imports, a `File` being readable in step there alone, so
//! on any other target this file is empty.
#![cfg(target_arch = "wasm32")]
#![expect(
    unsafe_code,
    reason = "a wasm import is an extern block, and an export keeps its name by `no_mangle`"
)]

use std::ffi::CString;

use kernel_files::{Create, Error, Folder, Mode, Origin};
use log::{LevelFilter, Log, Metadata, Record};
use logical_disc::{Disc, Expected, ReadAt, Source};

/// The most bytes a path may take: a picked file's or a place's.
const ROOM: u32 = 512;

#[link(wasm_import_module = "env")]
unsafe extern "C" {
    safe fn file_count() -> u32;
    safe fn file_size(file: u32) -> f64;
    fn file_path(file: u32, into: *mut u8, room: u32) -> u32;
    fn read_at(file: u32, at: f64, into: *mut u8, length: u32) -> u32;
    safe fn place_count() -> u32;
    fn place_path(place: u32, into: *mut u8, room: u32) -> u32;
    safe fn place_at(place: u32) -> f64;
    safe fn found(place: u32, byte: i32);
    safe fn expected_count() -> u32;
    fn expected_path(expected: u32, into: *mut u8, room: u32) -> u32;
    safe fn expected_empty(expected: u32) -> u32;
    fn finding(text: *const u8, length: u32);
    fn line(level: u32, text: *const u8, length: u32);
}

/// One picked file, by its index in the pick the Worker holds.
#[derive(Clone, Copy)]
struct Picked(u32);

impl ReadAt for Picked {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a file's size"
    )]
    fn size(&self) -> u64 {
        file_size(self.0) as u64
    }

    #[expect(
        clippy::cast_precision_loss,
        reason = "a disc's offset is far below 2^53"
    )]
    fn read_at(&self, at: u64, into: &mut [u8]) -> Result<(), Error> {
        let past = at.checked_add(into.len() as u64);
        if past.is_none_or(|end| end > self.size()) {
            return Err(Error::BadArgument);
        }
        let Ok(length) = u32::try_from(into.len()) else {
            return Err(Error::BadArgument);
        };
        // SAFETY: the import writes at most `length` bytes, which `into` holds, and
        // keeps nothing past the call.
        let failed = unsafe { read_at(self.0, at as f64, into.as_mut_ptr(), length) };
        if failed == 0 {
            Ok(())
        } else {
            Err(Error::Other(failed))
        }
    }
}

/// The text an import writes into a buffer of [`ROOM`] bytes; `None` when it did not
/// fit or is not UTF-8. The import answers the text's whole length, writing only when
/// it fits.
fn text(write: impl FnOnce(*mut u8, u32) -> u32) -> Option<String> {
    let mut bytes = [0; ROOM as usize];
    let length = write(bytes.as_mut_ptr(), ROOM) as usize;
    String::from_utf8(bytes.get(..length)?.to_vec()).ok()
}

/// The one byte at `at` of the disc's file at `path`, in the kernel's spelling.
fn byte_at(disc: &impl Folder, path: &str, at: f64) -> Result<u8, Error> {
    #[expect(clippy::cast_possible_truncation, reason = "a whole offset")]
    let distance = i32::try_from(at as i64).map_err(|_| Error::BadArgument)?;
    let name = CString::new(path).map_err(|_| Error::BadArgument)?;
    let file = disc.open(
        &name,
        Mode {
            read: true,
            write: false,
            create: Create::Existing,
        },
    )?;
    let mut byte = [0];
    let read = disc
        .seek(file, distance, Origin::Start)
        .and_then(|_| disc.read(file, &mut byte));
    disc.close(file)?;
    match read? {
        1 => Ok(byte[0]),
        _ => Err(Error::BadArgument),
    }
}

/// The log's sink: `oxide_logline`'s own `install` reads a clock this target lacks.
struct Sink;

impl Log for Sink {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let text = oxide_logline::body(record);
        let Ok(length) = u32::try_from(text.len()) else {
            return;
        };
        // SAFETY: the import reads `length` bytes, which `text` holds, and keeps
        // nothing past the call.
        unsafe { line(record.level() as u32, text.as_ptr(), length) };
    }

    fn flush(&self) {}
}

static SINK: Sink = Sink;

/// Installs the sink, at INFO, or DEBUG when `debug` is non-zero. A Worker runs one act
/// on one instance, so the first install is the only one.
fn install(debug: u32) {
    if log::set_logger(&SINK).is_ok() {
        log::set_max_level(if debug == 0 {
            LevelFilter::Info
        } else {
            LevelFilter::Debug
        });
    }
}

/// The picked file's path by the import, or `None` when it did not fit [`ROOM`].
fn picked_path(file: u32) -> Option<String> {
    text(|into, room| {
        // SAFETY: the import writes at most `room` bytes at `into`, the buffer `text`
        // holds, and keeps nothing past the call.
        unsafe { file_path(file, into, room) }
    })
}

/// Opens the pick and reads one byte at each place, handing each to `found`, or -1
/// for a place that could not be read. `folder` non-zero takes every picked file by its
/// path; zero takes the one picked file as an iso. `debug` non-zero adds the DEBUG
/// lines. Answers 0 when the disc opened, 1 when it did not, 2 when a path did not fit
/// [`ROOM`] or was not text.
#[unsafe(no_mangle)]
pub extern "C" fn read(debug: u32, folder: u32) -> u32 {
    install(debug);
    let source = if folder == 0 {
        Source::Iso(Picked(0))
    } else {
        let mut picked = Vec::new();
        for file in 0..file_count() {
            let Some(path) = picked_path(file) else {
                return 2;
            };
            picked.push((path, Picked(file)));
        }
        Source::Folder(picked)
    };
    let disc = match Disc::open(source) {
        Ok(disc) => disc,
        Err(error) => {
            log::warn!(e:? = error; "disc.open.failed");
            return 1;
        }
    };
    for place in 0..place_count() {
        let Some(path) = text(|into, room| {
            // SAFETY: the import writes at most `room` bytes at `into`, the buffer
            // `text` holds, and keeps nothing past the call.
            unsafe { place_path(place, into, room) }
        }) else {
            return 2;
        };
        let byte = byte_at(&disc, &path, place_at(place)).map_or(-1, i32::from);
        log::debug!(i = place, p:% = path, b = byte; "disc.place");
        found(place, byte);
    }
    0
}

/// Checks the picked files, each by its path from the disc's top, against the files the
/// page says the game expects, and hands each finding's one sentence to `finding`, in
/// the expected list's order. A picked file counts as readable when it is empty or its
/// first byte reads now; a file moved or removed after the pick does not read, and its
/// size goes to the check as unknown. `debug` non-zero adds the check's DEBUG lines.
/// Answers 0 when the check ran, 2 when a path did not fit [`ROOM`] or was not text.
#[unsafe(no_mangle)]
pub extern "C" fn check(debug: u32) -> u32 {
    install(debug);
    let mut picked = Vec::new();
    for file in 0..file_count() {
        let Some(path) = picked_path(file) else {
            return 2;
        };
        let size = Picked(file).size();
        let readable = size == 0 || Picked(file).read_at(0, &mut [0]).is_ok();
        picked.push((path, readable.then_some(size)));
    }
    let mut expected = Vec::new();
    for item in 0..expected_count() {
        let Some(path) = text(|into, room| {
            // SAFETY: the import writes at most `room` bytes at `into`, the buffer
            // `text` holds, and keeps nothing past the call.
            unsafe { expected_path(item, into, room) }
        }) else {
            return 2;
        };
        expected.push((path, expected_empty(item) != 0));
    }
    let picked: Vec<_> = picked
        .iter()
        .map(|(path, size)| logical_disc::Picked {
            path: path.as_str(),
            size: *size,
        })
        .collect();
    let expected: Vec<_> = expected
        .iter()
        .map(|(path, empty)| Expected {
            path: path.as_str(),
            empty_at_source: *empty,
        })
        .collect();
    for item in logical_disc::check(&expected, &picked) {
        let sentence = item.to_string();
        let Ok(length) = u32::try_from(sentence.len()) else {
            continue;
        };
        // SAFETY: the import reads `length` bytes, which `sentence` holds, and keeps
        // nothing past the call.
        unsafe { finding(sentence.as_ptr(), length) };
    }
    0
}
