// SPDX-License-Identifier: MIT
//! The front's rules on a row made for the test, no game's: the name
//! table's match and refusals, the last error, the path cut, the end,
//! the environment block, another program refused, the raise's end, the
//! filter kept and the memory map's answers.
//! The tests from real runs live in each game's rows crate.
use alloc::vec::Vec;
use core::cell::Cell;

use crate::page::Page;
use crate::table::{Calls, Errors, Module, Row, Startup};
use crate::{Access, Front, Process, Text, names, plant, resource};

const CALLS: Calls = Calls {
    command_line: "K!GetCommandLineA",
    module_handle: "K!GetModuleHandleA",
    module_file_name: "K!GetModuleFileNameA",
    version: "K!GetVersion",
    current_process: "K!GetCurrentProcess",
    load_library: "K!LoadLibraryA",
    proc_address: "K!GetProcAddress",
    free_library: "K!FreeLibrary",
    last_error: "K!GetLastError",
    set_last_error: "K!SetLastError",
    exit: "K!ExitProcess",
    environment: "K!GetEnvironmentStrings",
    environment_w: "K!GetEnvironmentStringsW",
    free_environment: "K!FreeEnvironmentStringsA",
    free_environment_w: "K!FreeEnvironmentStringsW",
    startup_info: "K!GetStartupInfoA",
    set_environment: "K!SetEnvironmentVariableA",
    terminate: "K!TerminateProcess",
    load_module: "K!LoadModule",
    create_process: "K!CreateProcessA",
    debug_string: "K!OutputDebugStringA",
    debug_break: "K!DebugBreak",
    raise: "K!RaiseException",
    set_filter: "K!SetUnhandledExceptionFilter",
    unhandled_filter: "K!UnhandledExceptionFilter",
    bad_read: "K!IsBadReadPtr",
    bad_write: "K!IsBadWritePtr",
    bad_code: "K!IsBadCodePtr",
    unwind: "K!RtlUnwind",
};

const ROW: Row = Row {
    file: "test",
    calls: CALLS,
    command_line: "PROG.EXE -x",
    own: 0x0200_0000,
    path: "C:\\DIR\\PROG.EXE",
    version: 0xc000_0a04,
    current_process: 0xffff_ffff,
    environment: b"A=1\0PATH=C:\\\0\0",
    startup: Startup {
        flags: 1,
        show_window: 3,
        reserved_count: 0,
        reserved: 0,
    },
    modules: &[
        Module {
            name: "LIB.DLL",
            handle: 0x2000_0000,
            entries: &["Main"],
        },
        Module {
            name: "SYS.DLL",
            handle: 0x2000_0000,
            entries: &[],
        },
    ],
    errors: Errors {
        module_not_found: 126,
        name_not_found: 127,
        bad_handle: 6,
        file_not_found: 2,
    },
};

fn entries(module: &Module, name: &str) -> u32 {
    u32::from(module.name == "LIB.DLL" && name == "Main") * 7
}

/// A map of one page at `0x1000`: read and run, written only below
/// `0x1800`.
fn memory(address: u32, size: u32, access: Access) -> bool {
    let end = u64::from(address) + u64::from(size);
    let top = if access == Access::Write {
        0x1800
    } else {
        0x2000
    };
    address >= 0x1000 && end <= top
}

fn front() -> Front<Page> {
    Front::new(&ROW, Page::new(entries, memory))
}

#[test]
fn a_module_name_matches_as_windows_does() {
    assert!(names(b"LIB.DLL", "LIB.DLL"));
    assert!(names(b"lib", "LIB.DLL"));
    assert!(names(b"C:\\X\\Lib.Dll", "LIB.DLL"));
    assert!(!names(b"LIB.EXE", "LIB.DLL"));
    assert!(!names(b"LI", "LIB.DLL"));
}

#[test]
fn a_name_in_the_table_is_found() {
    // The slot outlives a store: a fresh 0 is set, not assumed.
    kernel_last_error::set(0);
    let front = front();
    let handle = front.load_library(b"lib.dll");
    assert_eq!(handle, 0x2000_0000);
    assert_eq!(front.proc_address(handle, b"Main"), 7);
    assert_eq!(front.free_library(handle), 1);
    assert_eq!(front.process().last_error(), 0);
}

#[test]
fn a_missing_name_is_a_null_and_its_error() {
    let front = front();
    assert_eq!(front.load_library(b"NONE.DLL"), 0);
    assert_eq!(front.last_error(), 126);
    let system = front.module_handle(Some(b"sys.dll"));
    assert_eq!(system, 0x2000_0000);
    assert_eq!(front.proc_address(system, b"IsTNT"), 0);
    assert_eq!(front.last_error(), 127);
    assert_eq!(front.proc_address(0x3000_0000, b"Main"), 0);
    assert_eq!(front.last_error(), 6);
    assert_eq!(front.free_library(0), 0);
}

#[test]
fn the_start_up_answers_are_the_row() {
    let front = front();
    assert_eq!(front.command_line(), "PROG.EXE -x");
    assert_eq!(front.version(), 0xc000_0a04);
    assert_eq!(front.module_handle(None), 0x0200_0000);
    let mut path = [0xff; 8];
    assert_eq!(front.module_file_name(0, &mut path), 7);
    assert_eq!(&path, b"C:\\DIR\\\0");
    assert_eq!(front.module_file_name(0x2000_0000, &mut path), 0);
    assert_eq!(front.last_error(), 6);
}

#[test]
fn the_last_error_set_is_got_and_the_end_kept() {
    let front = front();
    front.set_last_error(0x57);
    assert_eq!(front.last_error(), 0x57);
    front.exit(3);
    assert_eq!(front.process().ended(), Some(3));
}

#[test]
fn the_environment_is_the_row_block_wide_and_narrow() {
    let front = front();
    assert_eq!(front.environment(), b"A=1\0PATH=C:\\\0\0");
    let wide: [u16; 14] = core::array::from_fn(|at| u16::from(ROW.environment[at]));
    assert!(front.environment_w().eq(wide));
    assert_eq!(front.free_environment(0x50_0000), 1);
    assert_eq!(front.free_environment_w(0x50_0000), 1);
    assert_eq!(front.set_environment(b"A", Some(b"2")), 1);
    assert_eq!(front.environment(), ROW.environment);
    assert_eq!(front.startup_info(), ROW.startup);
}

#[test]
fn the_text_shows_every_byte_on_one_line() {
    assert_eq!(format!("{}", Text(b"A=1\0\xff")), "A=1\\x00\\xff");
}

#[test]
fn another_program_is_refused_as_not_found() {
    let front = front();
    assert_eq!(front.load_module(b"OTHER.EXE", 0), 2);
    assert_eq!(front.last_error(), 2);
    front.set_last_error(0);
    assert_eq!(front.create_process(None, Some(b"x.exe")), 0);
    assert_eq!(front.last_error(), 2);
    assert_eq!(front.process().ended(), None);
}

#[test]
fn a_raise_ends_the_run_with_its_code() {
    let front = front();
    front.debug_string(b"a line");
    front.debug_break();
    front.unwind(0, 0, 0, 0);
    assert_eq!(front.process().ended(), None);
    front.raise(0xc000_0094, 0, 0, 0);
    assert_eq!(front.process().ended(), Some(0xc000_0094));
}

#[test]
fn only_the_own_process_is_terminated() {
    let front = front();
    assert_eq!(front.terminate(0x1234, 9), 0);
    assert_eq!(front.last_error(), 6);
    assert_eq!(front.process().ended(), None);
    assert_eq!(front.terminate(0xffff_ffff, 9), 1);
    assert_eq!(front.process().ended(), Some(9));
}

#[test]
fn the_filter_set_answers_the_one_replaced() {
    let front = front();
    assert_eq!(front.set_filter(0x2007_8190), 0);
    assert_eq!(front.set_filter(0), 0x2007_8190);
    assert_eq!(front.unhandled_filter(0x0012_f000), 1);
}

#[test]
fn the_bad_pointers_answer_the_memory_map() {
    let front = front();
    assert_eq!(front.bad_read(0x1000, 0x1000), 0);
    assert_eq!(front.bad_read(0x1fff, 2), 1);
    assert_eq!(front.bad_write(0x1000, 0x800), 0);
    assert_eq!(front.bad_write(0x17ff, 2), 1);
    assert_eq!(front.bad_code(0x0fff), 1);
    assert_eq!(front.bad_code(0x1fff), 0);
}

/// A back whose disc holds one file for the program and its modules alike; it
/// counts the lines written around it.
struct Disc {
    image: Vec<u8>,
    lines: Cell<u32>,
    error: Cell<u32>,
}

impl Process for Disc {
    fn entry(&self, _: &Module, _: &str) -> u32 {
        0
    }

    fn end(&self, _: u32) {}

    fn fail(&self, code: u32) {
        self.error.set(code);
    }

    fn last_error(&self) -> u32 {
        self.error.get()
    }

    fn reaches(&self, _: u32, _: u32, _: Access) -> bool {
        false
    }

    fn image(&self, handle: u32) -> Option<&[u8]> {
        [ROW.own, ROW.modules[0].handle]
            .contains(&handle)
            .then_some(self.image.as_slice())
    }

    fn quiet(&self, line: impl FnOnce()) {
        self.lines.set(self.lines.get() + 1);
        line();
    }
}

#[test]
fn a_resource_is_read_by_the_handle_with_no_line_and_no_error() {
    let disc = Disc {
        image: plant::image(&[(resource::MENU, 7, b"menu")]),
        lines: Cell::new(0),
        error: Cell::new(9),
    };
    let front = Front::new(&ROW, &disc);
    for handle in [0, ROW.own, ROW.modules[0].handle] {
        let read = front.resource(handle, resource::MENU, 7);
        assert_eq!(read, Some(&b"menu"[..]), "{handle:x}");
    }
    assert_eq!(front.resource(0x1234, resource::MENU, 7), None);
    assert_eq!(front.resource(0, resource::MENU, 8), None);
    assert_eq!(front.resource(0, resource::DIALOG, 7), None);
    assert_eq!(disc.lines.get(), 0);
    assert_eq!(front.last_error(), 9);
    assert_eq!(disc.lines.get(), 1, "the count sees a line that is written");
}

#[test]
fn a_back_with_no_disc_has_no_resource() {
    assert_eq!(front().resource(0, resource::MENU, 7), None);
}
