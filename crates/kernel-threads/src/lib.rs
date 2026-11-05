// SPDX-License-Identifier: MIT
//! The Kernel's threads: 11 KERNEL32 names, written
//! once. A thread's number (`GetCurrentThreadId`); a lock, the critical
//! section (`Initialize`, `Enter`, `Leave`, `DeleteCriticalSection`); a
//! slot a thread (`TlsAlloc`, `TlsFree`, `TlsGetValue`, `TlsSetValue`);
//! a counter (`InterlockedIncrement`, `InterlockedDecrement`). Which files
//! call each is its game's rows crate's to name.
//!
//! Each file the seam serves is a [`table::Row`]; where the threads come
//! from is a back behind [`Threads`]: the page's one thread,
//! `page::Page`; a host's, `host::Host`; the original's calls under
//! Wine. [`ThreadCalls`] answers in the file's numbers and writes one
//! `imp.call` line a call: `c` the row's name, `a` the call's numbers, `v`
//! its answer, `from` the address the call returns to in the file that
//! made it (each front's first argument; 0 where a target has none).
//!
//! The fronts carry only what the callers read back:
//! a lock is the record's address alone, no reader reads its fields; a
//! slot's index, [`table::NO_SLOT`] its one tested failure; a counter's
//! new value.
//!
//! Not fronted: the 8 names a second thread is started and steered by
//! (`CreateThread`, `ExitThread`, `ResumeThread`, `SuspendThread`,
//! `TerminateThread`, `SetThreadPriority`, `GetThreadPriority`,
//! `GetCurrentThread`). A game that imports them cuts them with its row:
//! a page has a second thread only from a Worker.
//!
//! The crate names no game and nothing of any one target, so it builds
//! for `wasm32-unknown-unknown`; a game's numbers live in its rows crate.
#![cfg_attr(not(any(test, feature = "std")), no_std)]

#[cfg(any(test, feature = "page"))]
extern crate alloc;

#[cfg(any(test, feature = "std"))]
pub mod host;
#[cfg(any(test, feature = "page"))]
pub mod page;
pub mod table;
#[cfg(test)]
mod tests;

use core::sync::atomic::{AtomicI32, Ordering};
use oxide_trace::Args;
use table::{NO_SLOT, Row};

/// Where one file's threads come from: the seam between the calls and a
/// target. A lock is named by its record's address; a slot by its index.
pub trait Threads {
    /// The calling thread's number, never 0.
    fn id(&self) -> u32;
    /// Makes `lock` free.
    fn initialize(&self, lock: u32);
    /// Takes `lock`, waiting while another thread holds it; the thread that
    /// holds it takes it again, once more each time.
    fn enter(&self, lock: u32);
    /// Gives back one take of `lock`; free once every take is given back.
    fn leave(&self, lock: u32);
    /// Forgets `lock`.
    fn delete(&self, lock: u32);
    /// A free slot, every thread's value in it 0; `None` when all are taken.
    fn slot_alloc(&self) -> Option<u32>;
    /// Frees `slot`; false when it was not taken.
    fn slot_free(&self, slot: u32) -> bool;
    /// The calling thread's value in `slot`; `None` when it is not taken.
    fn slot_get(&self, slot: u32) -> Option<u32>;
    /// Sets the calling thread's value in `slot`; false when it is not taken.
    fn slot_set(&self, slot: u32, value: u32) -> bool;
    /// Keeps `code` as the calling thread's last error.
    fn set_last_error(&self, code: u32);
    /// Writes one line around whatever the target must keep across it.
    fn quiet(&self, line: impl FnOnce()) {
        line();
    }
}

/// One file's thread calls: its row, and its threads.
pub struct ThreadCalls<T> {
    file: &'static Row,
    threads: T,
}

impl<T: Threads> ThreadCalls<T> {
    pub const fn new(file: &'static Row, threads: T) -> Self {
        Self { file, threads }
    }

    /// The threads the calls ask.
    pub const fn threads(&self) -> &T {
        &self.threads
    }

    fn line<const N: usize>(
        &self,
        from: u32,
        name: &str,
        arguments: [oxide_trace::Shown<'_>; N],
        value: u32,
    ) {
        self.threads.quiet(|| {
            log::debug!(
                c = name, a:% = Args(arguments), v:% = oxide_trace::__shown!(value),
                from:% = oxide_trace::__shown!(from); "imp.call"
            );
        });
    }

    /// `GetCurrentThreadId`.
    #[must_use]
    pub fn current_thread_id(&self, from: u32) -> u32 {
        let id = self.threads.id();
        self.line(from, self.file.calls.current_thread_id, [], id);
        id
    }

    /// `InitializeCriticalSection` on the record at `lock`.
    pub fn initialize(&self, from: u32, lock: u32) {
        self.threads.initialize(lock);
        self.line(
            from,
            self.file.calls.initialize,
            [oxide_trace::__shown!(lock)],
            0,
        );
    }

    /// `EnterCriticalSection`.
    pub fn enter(&self, from: u32, lock: u32) {
        self.threads.enter(lock);
        self.line(
            from,
            self.file.calls.enter,
            [oxide_trace::__shown!(lock)],
            0,
        );
    }

    /// `LeaveCriticalSection`.
    pub fn leave(&self, from: u32, lock: u32) {
        self.threads.leave(lock);
        self.line(
            from,
            self.file.calls.leave,
            [oxide_trace::__shown!(lock)],
            0,
        );
    }

    /// `DeleteCriticalSection`.
    pub fn delete(&self, from: u32, lock: u32) {
        self.threads.delete(lock);
        self.line(
            from,
            self.file.calls.delete,
            [oxide_trace::__shown!(lock)],
            0,
        );
    }

    /// `TlsAlloc`: a slot's index, or [`NO_SLOT`].
    #[must_use]
    pub fn slot_alloc(&self, from: u32) -> u32 {
        let slot = self.threads.slot_alloc().unwrap_or(NO_SLOT);
        self.line(from, self.file.calls.slot_alloc, [], slot);
        slot
    }

    /// `TlsFree`: 1, or 0 with the row's bad argument left.
    #[must_use]
    pub fn slot_free(&self, from: u32, slot: u32) -> u32 {
        let freed = self.threads.slot_free(slot);
        if !freed {
            self.threads.set_last_error(self.file.bad_argument);
        }
        self.line(
            from,
            self.file.calls.slot_free,
            [oxide_trace::__shown!(slot)],
            u32::from(freed),
        );
        u32::from(freed)
    }

    /// `TlsGetValue`: the value, the last error 0 even on success (Windows
    /// clears it); 0 with the row's bad argument for a slot not taken.
    #[must_use]
    pub fn slot_get(&self, from: u32, slot: u32) -> u32 {
        let (value, error) = self
            .threads
            .slot_get(slot)
            .map_or((0, self.file.bad_argument), |value| (value, 0));
        self.threads.set_last_error(error);
        self.line(
            from,
            self.file.calls.slot_get,
            [oxide_trace::__shown!(slot)],
            value,
        );
        value
    }

    /// `TlsSetValue`: 1, or 0 with the row's bad argument left.
    #[must_use]
    pub fn slot_set(&self, from: u32, slot: u32, value: u32) -> u32 {
        let set = self.threads.slot_set(slot, value);
        if !set {
            self.threads.set_last_error(self.file.bad_argument);
        }
        let arguments = [oxide_trace::__shown!(slot), oxide_trace::__shown!(value)];
        self.line(from, self.file.calls.slot_set, arguments, u32::from(set));
        u32::from(set)
    }

    /// `InterlockedIncrement`: the new value, as NT answers; Win95 promised
    /// only its sign, which the new value keeps. A plain add where the
    /// target has one thread.
    #[must_use]
    pub fn increment(&self, from: u32, counter: &AtomicI32) -> i32 {
        let value = counter.fetch_add(1, Ordering::SeqCst).wrapping_add(1);
        self.counted(from, self.file.calls.increment, value);
        value
    }

    /// `InterlockedDecrement`: the new value.
    #[must_use]
    pub fn decrement(&self, from: u32, counter: &AtomicI32) -> i32 {
        let value = counter.fetch_sub(1, Ordering::SeqCst).wrapping_sub(1);
        self.counted(from, self.file.calls.decrement, value);
        value
    }

    #[allow(
        clippy::cast_sign_loss,
        reason = "the line shows the 32 bits as Windows returns them"
    )]
    fn counted(&self, from: u32, name: Option<&'static str>, value: i32) {
        self.line(from, name.unwrap_or("unimported"), [], value as u32);
    }
}
