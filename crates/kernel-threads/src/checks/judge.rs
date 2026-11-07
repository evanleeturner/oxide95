// SPDX-License-Identifier: MIT
//! The target's own answers, by calls declared here.
use super::Back;
use std::cell::UnsafeCell;
use std::ffi::c_void;
use std::sync::atomic::AtomicI32;

/// `CRITICAL_SECTION`'s record; only the target reads its fields.
#[repr(C)]
#[derive(Default)]
struct Section {
    debug: usize,
    lock_count: i32,
    recursion_count: i32,
    owning_thread: usize,
    lock_semaphore: usize,
    spin_count: usize,
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentThreadId() -> u32;
    fn InitializeCriticalSection(section: *mut Section);
    fn EnterCriticalSection(section: *mut Section);
    fn LeaveCriticalSection(section: *mut Section);
    fn DeleteCriticalSection(section: *mut Section);
    fn TlsAlloc() -> u32;
    fn TlsFree(index: u32) -> i32;
    fn TlsGetValue(index: u32) -> *mut c_void;
    fn TlsSetValue(index: u32, value: *mut c_void) -> i32;
    fn GetLastError() -> u32;
    fn SetLastError(code: u32);
    fn InterlockedIncrement(value: *mut i32) -> i32;
    fn InterlockedDecrement(value: *mut i32) -> i32;
}

/// The judge's own records, one a lock the checks name.
pub struct Kernel32 {
    sections: [UnsafeCell<Section>; 2],
}

// SAFETY: only the target's calls touch a record, and sharing a critical
// section between threads is what those calls are for.
unsafe impl Sync for Kernel32 {}

impl Kernel32 {
    pub fn new() -> Self {
        Self {
            sections: std::array::from_fn(|_| UnsafeCell::default()),
        }
    }

    fn section(&self, lock: u32) -> *mut Section {
        self.sections[usize::try_from(lock).expect("a lock's number fits")].get()
    }
}

impl Back for Kernel32 {
    fn id(&self) -> u32 {
        // SAFETY: takes nothing.
        unsafe { GetCurrentThreadId() }
    }
    fn initialize(&self, lock: u32) {
        // SAFETY: the record is this judge's and does not move.
        unsafe { InitializeCriticalSection(self.section(lock)) }
    }
    fn enter(&self, lock: u32) {
        // SAFETY: the checks initialize a lock before they enter it.
        unsafe { EnterCriticalSection(self.section(lock)) }
    }
    fn leave(&self, lock: u32) {
        // SAFETY: the checks leave only what this thread entered.
        unsafe { LeaveCriticalSection(self.section(lock)) }
    }
    fn delete(&self, lock: u32) {
        // SAFETY: the checks delete a lock only when nothing holds it.
        unsafe { DeleteCriticalSection(self.section(lock)) }
    }
    fn slot_alloc(&self) -> u32 {
        // SAFETY: takes nothing.
        unsafe { TlsAlloc() }
    }
    fn slot_free(&self, slot: u32) -> u32 {
        // SAFETY: a bad index is refused, which is the question.
        u32::from(unsafe { TlsFree(slot) } != 0)
    }
    fn slot_get(&self, slot: u32) -> u32 {
        // SAFETY: a bad index is refused, which is the question.
        let value = unsafe { TlsGetValue(slot) };
        u32::try_from(value as usize).expect("a pointer is 32 bits here")
    }
    fn slot_set(&self, slot: u32, value: u32) -> u32 {
        let value = usize::try_from(value).expect("a pointer holds 32 bits");
        // SAFETY: the value is kept and never read through.
        u32::from(unsafe { TlsSetValue(slot, value as *mut c_void) } != 0)
    }
    fn last_error(&self) -> u32 {
        // SAFETY: takes nothing.
        unsafe { GetLastError() }
    }
    fn set_last_error(&self, code: u32) {
        // SAFETY: takes a number.
        unsafe { SetLastError(code) }
    }
    fn increment(&self, counter: &AtomicI32) -> i32 {
        // SAFETY: the pointer is the atomic's own, live for the call.
        unsafe { InterlockedIncrement(counter.as_ptr()) }
    }
    fn decrement(&self, counter: &AtomicI32) -> i32 {
        // SAFETY: the pointer is the atomic's own, live for the call.
        unsafe { InterlockedDecrement(counter.as_ptr()) }
    }
}
