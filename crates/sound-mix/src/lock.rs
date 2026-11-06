// SPDX-License-Identifier: MIT
//! The crate's one lock: a spin lock, since the crate holds no target's names.
//! Nothing long runs inside it, and a callback runs outside it.
#![allow(unsafe_code)]

use core::cell::UnsafeCell;
use core::hint::spin_loop;
use core::sync::atomic::{AtomicBool, Ordering};

pub(crate) struct Lock<T> {
    held: AtomicBool,
    value: UnsafeCell<T>,
}

// SAFETY: `with` is the only way to the value, and it admits one caller at a time.
unsafe impl<T: Send> Sync for Lock<T> {}

impl<T> Lock<T> {
    pub(crate) const fn new(value: T) -> Self {
        Self { held: AtomicBool::new(false), value: UnsafeCell::new(value) }
    }

    /// Runs `call` holding the lock. A panic inside leaves it held on purpose:
    /// a named stop ends the program, and no one should read that state again.
    pub(crate) fn with<R>(&self, call: impl FnOnce(&mut T) -> R) -> R {
        while self
            .held
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            spin_loop();
        }
        // SAFETY: the exchange above made this the only holder until the store below.
        let answer = call(unsafe { &mut *self.value.get() });
        self.held.store(false, Ordering::Release);
        answer
    }
}
