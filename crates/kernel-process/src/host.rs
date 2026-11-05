// SPDX-License-Identifier: MIT
//! A host's process, Linux's, macOS's and Windows', through `std` alone:
//! no C library call, so no target gate. The entries' numbers are the
//! owner's, as [`crate::page::Page`]'s; the end is the host's own exit;
//! the last error one atomic, so a file's three fronts can share it
//! through `&Host`.
//!
//! The memory map is unread here (a named wall): `kernel-memory` keeps no
//! region list, so a pointer reaches when it is not null and its bytes do
//! not run past the address space. The C library's pointer checkers are
//! the only callers of the three `IsBad*Ptr` names.
use crate::table::Module;
use crate::{Access, Process};
use std::sync::atomic::{AtomicU32, Ordering};

/// The process a host's program runs in.
pub struct Host {
    entries: fn(&Module, &str) -> u32,
    error: AtomicU32,
}

impl Host {
    /// The entries' numbers from `entries`, 0 for a name it has none for.
    #[must_use]
    pub const fn new(entries: fn(&Module, &str) -> u32) -> Self {
        Self {
            entries,
            error: AtomicU32::new(0),
        }
    }
}

impl Process for Host {
    fn entry(&self, module: &Module, name: &str) -> u32 {
        (self.entries)(module, name)
    }

    /// Exits with `code`'s bits as the host's status: a Unix parent reads
    /// the low 8 alone, so `0xC000_0094` reads 148 there.
    fn end(&self, code: u32) {
        std::process::exit(i32::from_ne_bytes(code.to_ne_bytes()));
    }

    fn fail(&self, code: u32) {
        self.error.store(code, Ordering::Relaxed);
    }

    fn last_error(&self) -> u32 {
        self.error.load(Ordering::Relaxed)
    }

    fn reaches(&self, address: u32, size: u32, _access: Access) -> bool {
        size == 0 || (address != 0 && address.checked_add(size - 1).is_some())
    }
}
