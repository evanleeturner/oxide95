// SPDX-License-Identifier: MIT
//! A host's threads, Linux's, macOS's and Windows', through `std` alone. A
//! critical section re-enters on its own thread and `Mutex` does not, so a
//! lock is an owner and a count under a `Mutex`, waited on by a `Condvar`.
//! A thread's number is handed out at its first ask, from 1. A slot's
//! values live a thread each; a slot taken again is a new generation, so
//! every thread's old value in it reads 0, as Windows zeroes them.
use crate::Threads;
use crate::table::SLOTS;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};

static NEXT_THREAD: AtomicU32 = AtomicU32::new(1);

/// Every slot's generations, all hosts': a value kept under one never
/// reads under another.
static NEXT_GENERATION: AtomicU32 = AtomicU32::new(1);

thread_local! {
    static THREAD: u32 = NEXT_THREAD.fetch_add(1, Ordering::Relaxed);
    static ERROR: Cell<u32> = const { Cell::new(0) };
    /// This thread's (generation, value) a slot.
    static VALUES: RefCell<[(u32, u32); SLOTS as usize]> =
        const { RefCell::new([(0, 0); SLOTS as usize]) };
}

/// A lock's holder and its takes.
#[derive(Default)]
struct Lock {
    held: Mutex<(u32, u32)>,
    freed: Condvar,
}

/// The locks and slots one process shares.
pub struct Host {
    locks: Mutex<HashMap<u32, Arc<Lock>>>,
    /// Each slot's generation, 0 when free.
    slots: Mutex<[u32; SLOTS as usize]>,
}

fn held<V>(mutex: &Mutex<V>) -> MutexGuard<'_, V> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Default for Host {
    fn default() -> Self {
        Self {
            locks: Mutex::default(),
            slots: Mutex::new([0; SLOTS as usize]),
        }
    }
}

impl Host {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self, lock: u32) -> Arc<Lock> {
        Arc::clone(held(&self.locks).entry(lock).or_default())
    }

    fn generation(&self, slot: u32) -> Option<(usize, u32)> {
        let at = usize::try_from(slot).ok()?;
        let generation = *held(&self.slots).get(at)?;
        (generation != 0).then_some((at, generation))
    }
}

impl Threads for Host {
    fn id(&self) -> u32 {
        THREAD.with(|id| *id)
    }

    fn initialize(&self, lock: u32) {
        held(&self.locks).insert(lock, Arc::default());
    }

    fn enter(&self, lock: u32) {
        let me = self.id();
        let lock = self.lock(lock);
        let mut held = held(&lock.held);
        while held.1 > 0 && held.0 != me {
            held = lock
                .freed
                .wait(held)
                .unwrap_or_else(PoisonError::into_inner);
        }
        *held = (me, held.1 + 1);
    }

    fn leave(&self, lock: u32) {
        let me = self.id();
        let lock = self.lock(lock);
        let mut held = held(&lock.held);
        if held.0 == me && held.1 > 0 {
            held.1 -= 1;
            if held.1 == 0 {
                held.0 = 0;
                lock.freed.notify_one();
            }
        }
    }

    fn delete(&self, lock: u32) {
        held(&self.locks).remove(&lock);
    }

    fn slot_alloc(&self) -> Option<u32> {
        let mut slots = held(&self.slots);
        let at = slots.iter().position(|generation| *generation == 0)?;
        slots[at] = NEXT_GENERATION.fetch_add(1, Ordering::Relaxed);
        u32::try_from(at).ok()
    }

    fn slot_free(&self, slot: u32) -> bool {
        self.generation(slot)
            .map(|(at, _)| held(&self.slots)[at] = 0)
            .is_some()
    }

    fn slot_get(&self, slot: u32) -> Option<u32> {
        let (at, generation) = self.generation(slot)?;
        let (kept, value) = VALUES.with(|values| values.borrow()[at]);
        Some(if kept == generation { value } else { 0 })
    }

    fn slot_set(&self, slot: u32, value: u32) -> bool {
        self.generation(slot)
            .map(|(at, generation)| {
                VALUES.with(|values| values.borrow_mut()[at] = (generation, value));
            })
            .is_some()
    }

    fn set_last_error(&self, code: u32) {
        ERROR.with(|error| error.set(code));
    }
}

impl Host {
    /// The calling thread's last error.
    #[must_use]
    pub fn last_error(&self) -> u32 {
        ERROR.with(Cell::get)
    }
}
