// SPDX-License-Identifier: MIT
//! The thin export `process-browser.mjs` runs: the page's one thread answering through the fronts,
//! as text. It asks and answers; the script judges.
//!
//! `cargo build -p kernel_process --example page_process --target wasm32-unknown-unknown`
#![allow(
    unsafe_code,
    reason = "a page reaches a function only by its exported name"
)]

#[allow(dead_code, reason = "this export reads only some of the shared rows")]
#[path = "../tests/common/rows.rs"]
mod rows;

use core::fmt::{Display, Write as _};
use kernel_files::Files;
use kernel_files::memory::MemoryFolder;
use kernel_process::page::Page;
use kernel_process::table::Module;
use kernel_process::{Access, Front, Process};
use kernel_text::Portable;
use kernel_threads::ThreadCalls;
use kernel_time::Times;
use kernel_time::clock::Clock;
use logical_disc::{Disc, Source};
use rows::{FILES, OPEN_EXISTING, ROW, TEXT, THREADS, TIME, empty_disc};
use std::sync::OnceLock;

/// A handle no table gave and no module holds.
const NO_MODULE: u32 = 0x3000_0000;

static ANSWERS: OnceLock<String> = OnceLock::new();

thread_local! {
    static HANDED: Front<Page> = process();
}

fn entries(_: &Module, name: &str) -> u32 {
    u32::from(name == "Sleep") * 7
}

/// The page's map for the checks: a read and a run from `0x1000` to `0x2000`, a write below
/// `0x1800`, and an empty range anywhere.
fn map(address: u32, size: u32, access: Access) -> bool {
    let end = u64::from(address) + u64::from(size);
    let top = if access == Access::Write {
        0x1800
    } else {
        0x2000
    };
    size == 0 || (address >= 0x1000 && end <= top)
}

fn process() -> Front<Page> {
    Front::new(&ROW, Page::new(entries, map))
}

/// The answer and the error it left, the error cleared first.
fn both(front: &Front<Page>, call: impl FnOnce(&Front<Page>) -> i64) -> String {
    front.set_last_error(0);
    let value = call(front);
    format!("{value} error {}", front.last_error())
}

/// The answers so far, a line each: `name = answer`.
#[derive(Default)]
struct Rows(String);

impl Rows {
    fn row(&mut self, name: &str, answer: impl Display) {
        writeln!(self.0, "{name} = {answer}").ok();
    }
}

fn start(rows: &mut Rows) {
    let front = process();
    rows.row("start.command_line", front.command_line());
    rows.row("start.version", front.version());
    rows.row("start.own", front.module_handle(None));
    rows.row("start.current_process", front.current_process());
    let startup = front.startup_info();
    rows.row("start.flags", startup.flags);
    rows.row("start.show", startup.show_window);
    let block = front.environment();
    rows.row("start.block.len", block.len());
    rows.row(
        "start.block.end",
        format!("{} {}", block[block.len() - 2], block[block.len() - 1]),
    );
    let wide: Vec<u16> = front.environment_w().collect();
    rows.row("start.block_wide.len", wide.len());
    rows.row(
        "start.block_wide.end",
        format!("{} {}", wide[wide.len() - 2], wide[wide.len() - 1]),
    );
    let mut room = [0xff; 32];
    let count = front.module_file_name(0, &mut room);
    rows.row("start.path.count", count);
    rows.row(
        "start.path.same",
        &room[..ROW.path.len()] == ROW.path.as_bytes(),
    );
    rows.row("start.path.zero", room[ROW.path.len()]);
    rows.row("start.free_block", front.free_environment(0x50_0000));
    rows.row("start.free_block_wide", front.free_environment_w(0x50_0000));
    let mut short = [0xffu8; 4];
    let answer = both(&front, |f| i64::from(f.module_file_name(0, &mut short)));
    rows.row("start.room4", answer);
    rows.row("start.room4.last_is_zero", short[3] == 0);
}

fn names(rows: &mut Rows) {
    let front = process();
    let handle = ROW.modules[0].handle;
    let mut asked = |name: &str, call: &dyn Fn(&Front<Page>) -> i64| {
        rows.row(name, both(&front, call));
    };
    asked("names.load_miss", &|f| {
        i64::from(f.load_library(b"NO-SUCH.DLL"))
    });
    asked("names.handle_miss", &|f| {
        i64::from(f.module_handle(Some(b"NO-SUCH.DLL")))
    });
    asked("names.proc_miss", &|f| {
        i64::from(f.proc_address(handle, b"NoSuchEntry"))
    });
    asked("names.proc_found", &|f| {
        i64::from(f.proc_address(handle, b"Sleep"))
    });
    asked("names.load_full", &|f| {
        i64::from(f.load_library(b"KERNEL32.DLL"))
    });
    asked("names.load_lower", &|f| {
        i64::from(f.load_library(b"kernel32.dll"))
    });
    asked("names.load_no_extension", &|f| {
        i64::from(f.load_library(b"kernel32"))
    });
    asked("names.free_found", &|f| i64::from(f.free_library(handle)));
    asked("names.proc_bad", &|f| {
        i64::from(f.proc_address(NO_MODULE, b"Sleep"))
    });
    asked("names.free_bad", &|f| i64::from(f.free_library(NO_MODULE)));
    asked("names.file_bad", &|f| {
        let mut room = [0xffu8; 32];
        i64::from(f.module_file_name(NO_MODULE, &mut room))
    });
    asked("names.terminate_bad", &|f| {
        i64::from(f.terminate(0x1234, 9))
    });
    asked("names.path_miss", &|f| {
        i64::from(f.load_library(b"C:\\NO-SUCH\\KERNEL32.DLL"))
    });
}

fn round(rows: &mut Rows) {
    let front = process();
    front.set_last_error(1234);
    rows.row("round.page.set", front.last_error());
    front.set_last_error(0);
    rows.row("round.page.clear", front.last_error());
    front.load_library(b"NO-SUCH.DLL");
    rows.row("round.page.refused", front.last_error());
    rows.row("round.page.back", front.process().last_error());
}

fn pointers(rows: &mut Rows) {
    let front = process();
    rows.row("pointers.page.read_null_1", front.bad_read(0, 1));
    rows.row("pointers.page.read_null_0", front.bad_read(0, 0));
    rows.row("pointers.page.read_good", front.bad_read(0x1000, 4));
    rows.row("pointers.page.read_past", front.bad_read(0x1fff, 2));
    rows.row("pointers.page.write_good", front.bad_write(0x1000, 4));
    rows.row("pointers.page.write_past", front.bad_write(0x17ff, 2));
    rows.row("pointers.page.code_good", front.bad_code(0x1000));
    rows.row("pointers.page.code_past", front.bad_code(0x2000));
}

/// One miss in each crate that sets the last error, read in that crate and through the process.
/// Every crate sets and reads the one shared slot, so the two rows agree. Each miss clears the
/// slot first: a crate that set nothing would otherwise read the code the miss before it left.
fn last_error(rows: &mut Rows) {
    let front = process();
    let files = Files::new(&FILES, MemoryFolder::default());
    kernel_last_error::set(0);
    files.open(c"NO.DAT", 0x8000_0000, 1, OPEN_EXISTING, 0x80);
    rows.row("files.memory.own", files.folder().last_error());
    rows.row("files.memory.through", front.last_error());

    let disc = Disc::open(Source::Iso(empty_disc())).expect("the volume opens");
    let files = Files::new(&FILES, disc);
    kernel_last_error::set(0);
    files.open(c"NO.DAT", 0x8000_0000, 1, OPEN_EXISTING, 0x80);
    rows.row("files.disc.own", files.folder().last_error());
    rows.row("files.disc.through", front.last_error());

    let calls = ThreadCalls::new(&THREADS, kernel_threads::page::Page::new());
    kernel_last_error::set(0);
    let _ = calls.slot_free(0, 0xFFFF_FFF0);
    rows.row("threads.page.own", calls.threads().last_error());
    rows.row("threads.page.through", front.last_error());

    let times = Times::new(
        &TIME,
        kernel_time::page::Page::new(Clock::new(u64::MAX, 0), 0),
    );
    let mut date = [0; kernel_time::table::DATE_BYTES];
    kernel_last_error::set(0);
    times.to_system_time(1 << 63, &mut date);
    rows.row("time.page.own", times.clocks().last_error());
    rows.row("time.page.through", front.last_error());

    let text = kernel_text::Front::new(&TEXT, Portable::new(kernel_last_error::set));
    kernel_last_error::set(0);
    let refused = text.code_page_info(9999).is_none();
    rows.row("text.refused", refused);
    rows.row("text.handed", HANDED.with(Front::last_error));
}

fn ask() -> String {
    let mut rows = Rows::default();
    start(&mut rows);
    names(&mut rows);
    round(&mut rows);
    pointers(&mut rows);
    last_error(&mut rows);
    rows.0
}

/// The answers' first byte in the module's memory.
#[unsafe(no_mangle)]
pub extern "C" fn answers() -> *const u8 {
    ANSWERS.get_or_init(ask).as_ptr()
}

/// The answers' length in bytes.
#[unsafe(no_mangle)]
pub extern "C" fn answers_len() -> usize {
    ANSWERS.get_or_init(ask).len()
}
