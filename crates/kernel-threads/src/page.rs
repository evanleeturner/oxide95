// SPDX-License-Identifier: MIT
//! The page's threads: one. Its number is a constant, a lock a count of
//! takes that never waits, a slot one table. No Worker, no shared memory;
//! a callback another target runs on a second thread runs here from the
//! frame loop. The host tests use it as their one-thread back.
use crate::Threads;
use crate::table::SLOTS;
use alloc::collections::BTreeMap;
use core::cell::RefCell;

/// The page's one thread's number.
pub const PAGE_THREAD: u32 = 1;

/// The one thread's locks and slots.
pub struct Page {
    takes: RefCell<BTreeMap<u32, u32>>,
    slots: RefCell<[Option<u32>; SLOTS as usize]>,
}

impl Page {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            takes: RefCell::new(BTreeMap::new()),
            slots: RefCell::new([None; SLOTS as usize]),
        }
    }

    /// The takes `lock` holds now.
    #[must_use]
    pub fn takes(&self, lock: u32) -> u32 {
        self.takes.borrow().get(&lock).copied().unwrap_or(0)
    }

    /// The calling thread's last error, the slot every back shares.
    #[must_use]
    pub fn last_error(&self) -> u32 {
        kernel_last_error::get()
    }

    fn slot(&self, slot: u32) -> Option<usize> {
        let at = usize::try_from(slot).ok()?;
        self.slots.borrow().get(at)?.map(|_| at)
    }
}

impl Default for Page {
    fn default() -> Self {
        Self::new()
    }
}

impl Threads for Page {
    fn id(&self) -> u32 {
        PAGE_THREAD
    }

    fn initialize(&self, lock: u32) {
        self.takes.borrow_mut().insert(lock, 0);
    }

    fn enter(&self, lock: u32) {
        *self.takes.borrow_mut().entry(lock).or_insert(0) += 1;
    }

    fn leave(&self, lock: u32) {
        if let Some(takes) = self.takes.borrow_mut().get_mut(&lock) {
            *takes = takes.saturating_sub(1);
        }
    }

    fn delete(&self, lock: u32) {
        self.takes.borrow_mut().remove(&lock);
    }

    fn slot_alloc(&self) -> Option<u32> {
        let mut slots = self.slots.borrow_mut();
        let at = slots.iter().position(Option::is_none)?;
        slots[at] = Some(0);
        u32::try_from(at).ok()
    }

    fn slot_free(&self, slot: u32) -> bool {
        self.slot(slot)
            .map(|at| self.slots.borrow_mut()[at] = None)
            .is_some()
    }

    fn slot_get(&self, slot: u32) -> Option<u32> {
        self.slot(slot).and_then(|at| self.slots.borrow()[at])
    }

    fn slot_set(&self, slot: u32, value: u32) -> bool {
        self.slot(slot)
            .map(|at| self.slots.borrow_mut()[at] = Some(value))
            .is_some()
    }

    fn set_last_error(&self, code: u32) {
        kernel_last_error::set(code);
    }
}
