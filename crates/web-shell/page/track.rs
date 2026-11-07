// SPDX-License-Identifier: MIT
//! The thin wasm the page's Worker runs. [`run`] makes the track and never returns:
//! a Rust loop sits on an import between turns, the shape a program's waiting loop
//! needs. Every import is named and justified at the head of `src/lib.rs`. Only a Worker
//! can answer them, so on any other target this file is empty.
#![cfg(target_arch = "wasm32")]
#![expect(
    unsafe_code,
    reason = "a wasm import is an extern block, and an export keeps its name by `no_mangle`"
)]

use kernel_files::Error;
use log::{LevelFilter, Log, Metadata, Record};
use logical_disc::ReadAt;
use web_shell::crossing::Words;
use web_shell::track::{Host, Track};

#[link(wasm_import_module = "env")]
unsafe extern "C" {
    safe fn word_load(word: u32) -> i32;
    safe fn word_store(word: u32, value: i32);
    safe fn sit(millis: u32);
    safe fn now() -> f64;
    fn present(rgba: *const u8, width: u32, height: u32);
    safe fn file_size() -> f64;
    fn read_at(at: f64, into: *mut u8, length: u32) -> u32;
    fn line(level: u32, text: *const u8, length: u32);
}

/// The crossing and the screen, as this Worker's imports reach them.
struct Shared;

impl Words for Shared {
    fn load(&self, word: u32) -> i32 {
        word_load(word)
    }

    fn store(&self, word: u32, value: i32) {
        word_store(word, value);
    }
}

impl Host for Shared {
    fn present(&self, rgba: &[u8], width: u32, height: u32) {
        // SAFETY: the import reads `width * height * 4` bytes, which `rgba` holds, and
        // keeps nothing past the call.
        unsafe { present(rgba.as_ptr(), width, height) };
    }
}

/// The file the player picked, which the Worker holds.
#[derive(Clone, Copy)]
struct PickedFile;

impl ReadAt for PickedFile {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a file's size"
    )]
    fn size(&self) -> u64 {
        file_size() as u64
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
        let failed = unsafe { read_at(at as f64, into.as_mut_ptr(), length) };
        if failed == 0 {
            Ok(())
        } else {
            Err(Error::Other(failed))
        }
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

/// The track, turning until the Worker ends. `debug` non-zero adds the DEBUG lines;
/// `poll_ms` is how long it sits between turns.
#[unsafe(no_mangle)]
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "whole milliseconds"
)]
pub extern "C" fn run(debug: u32, poll_ms: u32) {
    if log::set_logger(&SINK).is_ok() {
        log::set_max_level(if debug == 0 {
            LevelFilter::Info
        } else {
            LevelFilter::Debug
        });
    }
    std::panic::set_hook(Box::new(
        |panic| log::error!(panic:% = panic; "track.panic"),
    ));
    let file = (file_size() >= 0.0).then_some(PickedFile);
    let mut track = Track::new(Shared, file);
    loop {
        track.turn(now() as u32);
        sit(poll_ms);
    }
}
