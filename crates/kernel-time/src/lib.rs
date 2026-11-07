// SPDX-License-Identifier: MIT
//! The Kernel's time: the 8 KERNEL32 time
//! names the four files import, written once, WINMM's `timeGetTime` that
//! reads the same game clock, and the C library's `time` that reads the
//! same wall. Every name that measures an
//! interval reads one game clock, [`clock::Clock`], handed in and moved
//! only by its owner: the frame loop on a page, a back on a host, a test.
//! The date names read a wall clock, handed in the same way. No front
//! reads the OS.
//!
//! No front asks for a time zone: every target answers UTC. Local
//! time is system time, a file time is its own local time, and the zone
//! call answers the row's no-zone id with its record all 0.
//!
//! Each file the seam serves is a [`table::Row`], kept in its game's rows
//! crate; where the clocks come from is a back behind [`Clocks`]: the
//! page's, [`page::Page`]; a host's, `host::Host`; the original's calls
//! under Wine. [`Times`] answers in the file's
//! numbers and writes one `imp.call` line a call: `c` the row's name, `a`
//! the call's numbers, `v` its answer, `t` the instant read (the wall's
//! ticks, or the game clock's microseconds), `w` the 8 date words written,
//! `f` the file time written, `e` the error left.
//!
//! The front answers only what the callers read back:
//! a date's 8 words, the counter's two halves, every byte of the zone
//! record. The counter's rate is the clock's, 1 MHz: its one reader
//! compares counts with each other alone.
//!
//! The crate names no game and nothing of any one target, so it builds
//! for `wasm32-unknown-unknown`; a game's numbers live in its rows crate.
#![cfg_attr(not(any(test, feature = "std")), no_std)]

pub mod clock;
pub mod date;
#[cfg(any(test, feature = "std"))]
pub mod host;
pub mod page;
pub mod table;
#[cfg(test)]
mod tests;

use date::{Date, UNIX_EPOCH};
use oxide_trace::Args;
use table::{DATE_BYTES, Row, ZONE_BYTES};

/// File-time ticks a second.
const TICKS_PER_SECOND: u64 = 10_000_000;

/// Where one file's clocks come from: the seam between the calls and a
/// target.
pub trait Clocks {
    /// The game clock's microseconds; a host's back moves it a frame first.
    fn micros(&self) -> u64;
    /// The wall clock: file-time ticks, UTC.
    fn wall(&self) -> u64;
    /// Waits `millis`; a back that cannot block returns at once.
    fn sleep(&self, millis: u32);
    /// Keeps `code` as the last error the file reads after a failure.
    fn fail(&self, code: u32);
    /// Writes one line around whatever the target must keep across it.
    fn quiet(&self, line: impl FnOnce()) {
        line();
    }
}

/// One file's time calls: its row, and its clocks.
pub struct Times<C> {
    file: &'static Row,
    clocks: C,
}

impl<C: Clocks> Times<C> {
    pub const fn new(file: &'static Row, clocks: C) -> Self {
        Self { file, clocks }
    }

    /// The clocks the calls ask.
    pub const fn clocks(&self) -> &C {
        &self.clocks
    }

    /// `GetTickCount`: the game clock's whole milliseconds, wrapping at
    /// 32 bits. One game alone imports it; no run of it is recorded yet.
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        reason = "the count wraps at 32 bits, as Windows' does"
    )]
    pub fn tick_count(&self) -> u32 {
        let micros = self.clocks.micros();
        let value = (micros / 1000) as u32;
        self.clocks.quiet(|| {
            log::debug!(
                c = self.file.calls.tick_count, v:% = oxide_trace::__shown!(value),
                t:% = oxide_trace::__shown!(micros), by = "kernel-time"; "imp.call"
            );
        });
        value
    }

    /// `timeGetTime`: the game clock's whole milliseconds, wrapping at 32
    /// bits, off the one clock `GetTickCount` reads, so the two names never
    /// disagree. The line's `by` says who computed the answer: a front, not
    /// WINMM.
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        reason = "the count wraps at 32 bits, as Windows' does"
    )]
    pub fn time_get_time(&self) -> u32 {
        let micros = self.clocks.micros();
        let value = (micros / 1000) as u32;
        self.clocks.quiet(|| {
            log::debug!(
                c = self.file.calls.time_get_time, v:% = oxide_trace::__shown!(value),
                t:% = oxide_trace::__shown!(micros), by = "kernel-time"; "imp.call"
            );
        });
        value
    }

    /// `Sleep`: the back waits, or returns at once on the page.
    pub fn sleep(&self, millis: u32) {
        self.clocks.sleep(millis);
        let arguments = [oxide_trace::__shown!(millis)];
        self.clocks.quiet(|| {
            log::debug!(c = self.file.calls.sleep, a:% = Args(arguments),
                by = "kernel-time"; "imp.call");
        });
    }

    /// `QueryPerformanceCounter`: the game clock's microseconds into
    /// `into`, both halves; 1.
    #[allow(
        clippy::cast_possible_wrap,
        reason = "the counter is Windows' signed 64-bit count"
    )]
    pub fn counter(&self, into: &mut i64) -> i32 {
        let micros = self.clocks.micros();
        *into = micros as i64;
        self.clocks.quiet(|| {
            log::debug!(
                c = self.file.calls.counter, v:% = oxide_trace::__shown!(1),
                t:% = oxide_trace::__shown!(micros), by = "kernel-time"; "imp.call"
            );
        });
        1
    }

    /// `GetSystemTime`: the wall clock's UTC date into `into`.
    pub fn system_time(&self, into: &mut [u8; DATE_BYTES]) {
        let wall = self.clocks.wall();
        let date = Date::of(wall);
        *into = date.to_bytes();
        self.dated(self.file.calls.system_time, wall, &date);
    }

    /// `GetLocalTime`: the wall clock's UTC date into `into`, as
    /// `GetSystemTime`'s.
    pub fn local_time(&self, into: &mut [u8; DATE_BYTES]) {
        let wall = self.clocks.wall();
        let date = Date::of(wall);
        *into = date.to_bytes();
        self.dated(self.file.calls.local_time, wall, &date);
    }

    /// `GetTimeZoneInformation`: no zone, the record all 0 into
    /// `into`; the row's id for no zone in force.
    pub fn zone(&self, into: &mut [u8; ZONE_BYTES]) -> u32 {
        *into = [0; ZONE_BYTES];
        let value = self.file.zone_unknown;
        self.clocks.quiet(|| {
            log::debug!(c = self.file.calls.zone, v:% = oxide_trace::__shown!(value),
                by = "kernel-time"; "imp.call");
        });
        value
    }

    /// `FileTimeToLocalFileTime`: `ticks` itself into `into`, UTC being
    /// its own local time; 1.
    pub fn to_local_file_time(&self, ticks: u64, into: &mut u64) -> i32 {
        *into = ticks;
        let arguments = [oxide_trace::__shown!(ticks)];
        self.clocks.quiet(|| {
            log::debug!(
                c = self.file.calls.to_local_file_time, a:% = Args(arguments),
                v:% = oxide_trace::__shown!(1), f:% = oxide_trace::__shown!(ticks),
                by = "kernel-time"; "imp.call"
            );
        });
        1
    }

    /// `FileTimeToSystemTime`: the date of `ticks` into `into`, and 1; a
    /// time at or past 2^63 is refused, 0 and the row's bad argument
    /// (Windows' documented refusal; its code unmeasured).
    pub fn to_system_time(&self, ticks: u64, into: &mut [u8; DATE_BYTES]) -> i32 {
        let (value, error, date) = if ticks >> 63 == 0 {
            let date = Date::of(ticks);
            *into = date.to_bytes();
            (1, 0, date)
        } else {
            self.clocks.fail(self.file.bad_argument);
            (0, self.file.bad_argument, Date::default())
        };
        let arguments = [oxide_trace::__shown!(ticks)];
        let words = date.0;
        self.clocks.quiet(|| {
            log::debug!(
                c = self.file.calls.to_system_time, a:% = Args(arguments),
                v:% = oxide_trace::__shown!(value),
                w:% = Args(words.each_ref().map(|word| oxide_trace::__shown!(*word))),
                e:% = oxide_trace::__shown!(error), by = "kernel-time"; "imp.call"
            );
        });
        value
    }

    /// The C library's `time`: the wall clock's whole seconds since 1970,
    /// UTC, also into `timer` when there is one; the line's `a` is the
    /// timer's address, 0 for none. A wall before 1970 answers 0, one past
    /// 32 bits of seconds `i32::MAX`.
    pub fn seconds(&self, timer: Option<&mut i32>) -> i32 {
        let wall = self.clocks.wall();
        let seconds = wall.saturating_sub(UNIX_EPOCH) / TICKS_PER_SECOND;
        let value = i32::try_from(seconds).unwrap_or(i32::MAX);
        let address = timer
            .as_deref()
            .map_or(core::ptr::null(), core::ptr::from_ref);
        if let Some(timer) = timer {
            *timer = value;
        }
        let arguments = [oxide_trace::__shown!(address)];
        self.clocks.quiet(|| {
            log::debug!(
                c = self.file.calls.time, a:% = Args(arguments),
                v:% = oxide_trace::__shown!(value), t:% = oxide_trace::__shown!(wall),
                by = "kernel-time"; "imp.call"
            );
        });
        value
    }

    /// A date call's line.
    fn dated(&self, name: &'static str, wall: u64, date: &Date) {
        let words = date.0;
        let shown = words.each_ref().map(|word| oxide_trace::__shown!(*word));
        self.clocks.quiet(|| {
            log::debug!(
                c = name, t:% = oxide_trace::__shown!(wall), w:% = Args(shown),
                by = "kernel-time"; "imp.call"
            );
        });
    }
}
