// SPDX-License-Identifier: MIT
//! One miss, read two ways. The last error is one slot a thread, so a miss in any crate should read
//! back through `kernel_process`. Each back is a test: `CHECK <host> <back>.own` prints what the
//! back kept, `CHECK <host> <back>.through` what the process answers, and the test asserts both.
//! Every back sets and reads the one shared slot, so the two rows agree. A page's threads share
//! one slot, so each miss test clears it first: a back that set nothing would otherwise read the
//! code the test before it left.
#![allow(
    unsafe_code,
    reason = "the judge reaches the target's calls by hand-declared names"
)]

#[allow(dead_code, reason = "each file reads only some of the shared rows")]
#[path = "common/rows.rs"]
mod rows;

use kernel_files::Files;
use kernel_files::memory::MemoryFolder;
use kernel_process::host::Host;
use kernel_process::page::Page;
use kernel_process::table::Module;
use kernel_process::{Access, Front};
use kernel_text::Portable;
use kernel_threads::ThreadCalls;
use kernel_time::Times;
use kernel_time::clock::Clock;
use logical_disc::{Disc, Source};
use rows::{BAD_ARGUMENT, FILES, NOT_FOUND, OPEN_EXISTING, ROW, TEXT, THREADS, TIME, empty_disc};

const HOST: &str = if cfg!(windows) { "w" } else { "n" };

fn entries(_: &Module, _: &str) -> u32 {
    0
}

fn map(_: u32, _: u32, _: Access) -> bool {
    true
}

fn host_process() -> Front<Host> {
    Front::new(&ROW, Host::new(entries))
}

fn page_process() -> Front<Page> {
    Front::new(&ROW, Page::new(entries, map))
}

fn read_both(back: &str, code: u32, own: u32, through: u32) {
    println!("CHECK {HOST} {back}.own want={code} got={own}");
    println!("CHECK {HOST} {back}.through want={code} got={through}");
    assert_eq!(own, code, "{back} kept no error of its own");
    assert_eq!(
        through, code,
        "{back}'s miss does not read through the process"
    );
}

#[test]
fn a_miss_in_the_memory_folder_reads_through_the_process() {
    kernel_last_error::set(0);
    let process = page_process();
    let files = Files::new(&FILES, MemoryFolder::default());
    files.open(c"NO.DAT", 0x8000_0000, 1, OPEN_EXISTING, 0x80);
    let own = files.folder().last_error();
    read_both("files.memory", NOT_FOUND, own, process.last_error());
}

#[test]
fn a_miss_on_the_disc_reads_through_the_process() {
    kernel_last_error::set(0);
    let process = page_process();
    let disc = Disc::open(Source::Iso(empty_disc())).expect("the volume opens");
    let files = Files::new(&FILES, disc);
    files.open(c"NO.DAT", 0x8000_0000, 1, OPEN_EXISTING, 0x80);
    let own = files.folder().last_error();
    read_both("files.disc", NOT_FOUND, own, process.last_error());
}

#[test]
fn a_refused_slot_on_the_host_reads_through_the_process() {
    kernel_last_error::set(0);
    let process = host_process();
    let calls = ThreadCalls::new(&THREADS, kernel_threads::host::Host::new());
    let _ = calls.slot_free(0, 0xFFFF_FFF0);
    let own = calls.threads().last_error();
    read_both("threads.host", BAD_ARGUMENT, own, process.last_error());
}

#[test]
fn a_refused_slot_on_the_page_reads_through_the_process() {
    kernel_last_error::set(0);
    let process = page_process();
    let calls = ThreadCalls::new(&THREADS, kernel_threads::page::Page::new());
    let _ = calls.slot_free(0, 0xFFFF_FFF0);
    let own = calls.threads().last_error();
    read_both("threads.page", BAD_ARGUMENT, own, process.last_error());
}

#[test]
fn a_refused_time_on_the_host_reads_through_the_process() {
    kernel_last_error::set(0);
    let process = host_process();
    let times = Times::new(&TIME, kernel_time::host::Host::new(u64::MAX));
    let mut date = [0; kernel_time::table::DATE_BYTES];
    times.to_system_time(1 << 63, &mut date);
    let own = times.clocks().last_error();
    read_both("time.host", BAD_ARGUMENT, own, process.last_error());
}

#[test]
fn a_refused_time_on_the_page_reads_through_the_process() {
    kernel_last_error::set(0);
    let process = page_process();
    let times = Times::new(
        &TIME,
        kernel_time::page::Page::new(Clock::new(u64::MAX, 0), 0),
    );
    let mut date = [0; kernel_time::table::DATE_BYTES];
    times.to_system_time(1 << 63, &mut date);
    let own = times.clocks().last_error();
    read_both("time.page", BAD_ARGUMENT, own, process.last_error());
}

#[test]
fn the_host_back_keeps_one_slot_a_thread() {
    let back = Host::new(entries);
    Front::new(&ROW, &back).set_last_error(1234);
    let other = std::thread::scope(|scope| {
        scope
            .spawn(|| Front::new(&ROW, &back).last_error())
            .join()
            .expect("the other thread ends")
    });
    println!("CHECK {HOST} process.host.other_thread want=0 got={other}");
    assert_eq!(other, 0, "a second thread read the first thread's error");
}

thread_local! {
    static HANDED: Front<Page> = page_process();
}

fn handed(code: u32) {
    HANDED.with(|process| process.set_last_error(code));
}

/// The text back holds no store: it hands the code to the `fn(u32)` its caller gave, so it joins
/// whatever slot that function reaches, and here the test's function is what reaches the process.
#[test]
fn the_text_back_keeps_no_error_and_hands_it_on() {
    kernel_last_error::set(0);
    let front = kernel_text::Front::new(&TEXT, Portable::new(handed));
    assert!(front.code_page_info(9999).is_none());
    let got = page_process().last_error();
    println!("CHECK {HOST} text.handed want={BAD_ARGUMENT} got={got}");
    assert_eq!(got, BAD_ARGUMENT);
}

/// What the promise is measured against: the target's own slot, a miss and a second thread.
#[cfg(windows)]
#[test]
fn the_target_keeps_one_slot_a_thread() {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateFileA(
            name: *const u8,
            access: u32,
            share: u32,
            security: *mut u8,
            disposition: u32,
            flags: u32,
            template: *mut u8,
        ) -> isize;
        fn GetLastError() -> u32;
        fn SetLastError(code: u32);
    }
    let null = std::ptr::null_mut();
    // SAFETY: the name is NUL-ended and the null security and template are allowed.
    let (missed, error) = unsafe {
        let missed = CreateFileA(
            c"no-such-file.dat".as_ptr().cast(),
            0x8000_0000,
            1,
            null,
            OPEN_EXISTING,
            0x80,
            null,
        );
        (missed, GetLastError())
    };
    println!("CHECK w wine.last_error.miss want=-1 error 2 got={missed} error {error}");
    assert_eq!((missed, error), (-1, NOT_FOUND));
    // SAFETY: no pointer crosses either call.
    let own = unsafe {
        SetLastError(1234);
        GetLastError()
    };
    // SAFETY: as above.
    let other = std::thread::spawn(|| unsafe { GetLastError() })
        .join()
        .expect("the other thread ends");
    println!("CHECK w wine.last_error.own want=1234 got={own}");
    println!("CHECK w wine.last_error.other_thread want=0 got={other}");
    assert_eq!((own, other), (1234, 0));
}
