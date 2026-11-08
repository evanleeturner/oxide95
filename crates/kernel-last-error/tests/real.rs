// SPDX-License-Identifier: MIT
//! On Windows the slot is the thread's own last error, not a copy of it: a
//! code set through the slot is what the system's `GetLastError` reads, and a
//! code the system's `SetLastError` keeps is what the slot reads. Each row
//! prints `CHECK w <row> want=<code> got=<code>`, and the test asserts it.
#![cfg(windows)]
#![allow(
    unsafe_code,
    reason = "the judge reaches the system's calls by hand-declared names"
)]

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetLastError() -> u32;
    fn SetLastError(code: u32);
}

fn check(row: &str, want: u32, got: u32) {
    println!("CHECK w {row} want={want} got={got}");
    assert_eq!(
        got, want,
        "{row}: the slot is not the thread's own last error"
    );
}

#[test]
fn a_code_set_through_the_slot_is_the_systems_last_error() {
    kernel_last_error::set(4321);
    // SAFETY: reads the calling thread's own last error; no pointer.
    let got = unsafe { GetLastError() };
    check("slot.real.set", 4321, got);
}

#[test]
fn a_code_the_system_keeps_reads_through_the_slot() {
    // SAFETY: sets the calling thread's own last error; no pointer.
    unsafe { SetLastError(4322) };
    check("slot.real.get", 4322, kernel_last_error::get());
}
