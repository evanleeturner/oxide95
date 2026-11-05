// SPDX-License-Identifier: MIT
//! The page's process: everything handed in. The page's owner gives the
//! entries' numbers (each a function's place in the module's table) and
//! reads the end after the call returns; nothing here blocks or exits.
//! The host tests use it as their fixed process.
use crate::table::Module;
use crate::{Access, Process};
use core::cell::Cell;

/// The process a page's owner feeds.
pub struct Page {
    entries: fn(&Module, &str) -> u32,
    memory: fn(u32, u32, Access) -> bool,
    error: Cell<u32>,
    ended: Cell<Option<u32>>,
}

impl Page {
    /// The entries' numbers from `entries`, 0 for a name it has none for;
    /// the memory map's answers from `memory`.
    #[must_use]
    pub const fn new(
        entries: fn(&Module, &str) -> u32,
        memory: fn(u32, u32, Access) -> bool,
    ) -> Self {
        Self {
            entries,
            memory,
            error: Cell::new(0),
            ended: Cell::new(None),
        }
    }

    /// The code the run ended with, once it has.
    #[must_use]
    pub fn ended(&self) -> Option<u32> {
        self.ended.get()
    }
}

impl Process for Page {
    fn entry(&self, module: &Module, name: &str) -> u32 {
        (self.entries)(module, name)
    }

    fn end(&self, code: u32) {
        self.ended.set(Some(code));
    }

    fn fail(&self, code: u32) {
        self.error.set(code);
    }

    fn last_error(&self) -> u32 {
        self.error.get()
    }

    fn reaches(&self, address: u32, size: u32, access: Access) -> bool {
        (self.memory)(address, size, access)
    }
}
