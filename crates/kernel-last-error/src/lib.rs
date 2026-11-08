// SPDX-License-Identifier: MIT
//! The calling thread's last error, one slot under every seam: each back's
//! refusal sets it and each `GetLastError` reads it, so a code one call
//! keeps reads back through any other, as on one Windows thread. The slot
//! is picked by target, never by a feature, as features unify across one
//! build and a test build could pick a slot no target ships:
//!
//! - **Windows:** the thread's own, through `SetLastError` and
//!   `GetLastError`, so the program and the system read one value.
//! - **A wasm page:** one static, as a page runs one thread; a callback
//!   from its frame loop shares the static with the code around it.
//! - **Any other target:** a thread-local, through `std`; such a target
//!   without `std` does not build this crate.
//!
//! A fresh thread's slot reads 0, but a slot outlives the store a test
//! makes: a test that wants a fresh 0 sets it first.
#![no_std]

/// Keeps `code` as the calling thread's last error.
pub fn set(code: u32) {
    slot::set(code);
}

/// The calling thread's last error.
#[must_use]
pub fn get() -> u32 {
    slot::get()
}

#[cfg(windows)]
#[allow(unsafe_code)]
mod slot {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetLastError() -> u32;
        fn SetLastError(code: u32);
    }

    pub(crate) fn set(code: u32) {
        // SAFETY: sets the calling thread's own last error; no pointer.
        unsafe { SetLastError(code) };
    }

    pub(crate) fn get() -> u32 {
        // SAFETY: reads the calling thread's own last error; no pointer.
        unsafe { GetLastError() }
    }
}

#[cfg(all(not(windows), target_family = "wasm"))]
mod slot {
    use core::sync::atomic::{AtomicU32, Ordering};

    /// The page's one thread's last error.
    static ERROR: AtomicU32 = AtomicU32::new(0);

    pub(crate) fn set(code: u32) {
        ERROR.store(code, Ordering::Relaxed);
    }

    pub(crate) fn get() -> u32 {
        ERROR.load(Ordering::Relaxed)
    }
}

#[cfg(not(any(windows, target_family = "wasm")))]
mod slot {
    extern crate std;

    use core::cell::Cell;

    std::thread_local! {
        /// This thread's last error.
        static ERROR: Cell<u32> = const { Cell::new(0) };
    }

    pub(crate) fn set(code: u32) {
        ERROR.with(|error| error.set(code));
    }

    pub(crate) fn get() -> u32 {
        ERROR.with(Cell::get)
    }
}
