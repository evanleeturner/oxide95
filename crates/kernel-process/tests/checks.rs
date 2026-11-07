// SPDX-License-Identifier: MIT
//! The process calls beside the target's own answers: the start-up, the name table, the last
//! error's round, bad pointers, resources and the ends. Each row prints `CHECK <host> <name>
//! want=<a> got=<b>`, host `w` under Wine and `n` natively. Under Wine a `wine.` row asks the real
//! `kernel32` from this same binary, so the wanted numbers are read against the target; natively
//! they stand alone. Wine 9.0 is not Windows 95: a row holds what Wine answered, and no more.
//!
//! An end ends its process, so each runs in a child of this binary, the test `child`, named by
//! `CHECKS_CHILD`; the parent reads its code and its line. A child that survives its end exits 77.
//!
//! The rows print no environment string, path or command line: lengths and sameness alone.
#![allow(
    unsafe_code,
    reason = "the judge reaches the target's calls by hand-declared names"
)]

use kernel_process::host::Host;
use kernel_process::page::Page;
use kernel_process::table::{Calls, Errors, Module, Row, Startup};
use kernel_process::{Access, Front, Process};
use std::cell::Cell;
use std::fmt::Display;
use std::process::Command;

static ROW: Row = Row {
    file: "checks",
    calls: Calls {
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
    },
    command_line: "PROG.EXE -x",
    own: 0x0200_0000,
    path: "C:\\DIR\\PROG.EXE",
    version: 0x23f0_0206,
    current_process: 0xffff_ffff,
    environment: b"A=1\0PATH=C:\\\0\0",
    startup: Startup {
        flags: 0,
        show_window: 1,
        reserved_count: 0,
        reserved: 0,
    },
    modules: &[Module {
        name: "KERNEL32.DLL",
        handle: 0x2000_0000,
        entries: &["Sleep"],
    }],
    errors: Errors {
        module_not_found: 126,
        name_not_found: 127,
        bad_handle: 6,
        file_not_found: 2,
    },
};

/// A handle no table gave and no module holds.
const NO_MODULE: u32 = 0x3000_0000;
/// The exception code the ends raise: `STATUS_INTEGER_DIVIDE_BY_ZERO`.
const RAISED: u32 = 0xC000_0094;
/// What this host's parent reads of [`RAISED`]: a Unix parent reads the low 8 bits alone.
#[cfg(windows)]
const RAISED_READ: i32 = i32::from_ne_bytes(RAISED.to_ne_bytes());
#[cfg(not(windows))]
const RAISED_READ: i32 = 148;
/// A child's code when every call it made returned, as those calls must.
const RETURNED: i32 = 33;
/// A child's code when it outlived its end.
const SURVIVED: i32 = 77;

thread_local! {
    static WRONG: Cell<u32> = const { Cell::new(0) };
}

fn host() -> &'static str {
    if cfg!(windows) { "w" } else { "n" }
}

/// One row: printed whole, wrong or right, so a run shows every answer before it fails.
fn check(name: &str, want: impl Display, got: impl Display) {
    let (want, got) = (want.to_string(), got.to_string());
    println!("CHECK {} {name} want={want} got={got}", host());
    if want != got {
        WRONG.with(|wrong| wrong.set(wrong.get() + 1));
    }
}

/// Fails the test if any of its rows did, after all of them printed.
fn done() {
    let wrong = WRONG.with(Cell::take);
    assert_eq!(wrong, 0, "{wrong} rows left the table");
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

fn host_front() -> Front<Host> {
    Front::new(&ROW, Host::new(entries))
}

fn page_front() -> Front<Page> {
    Front::new(&ROW, Page::new(entries, map))
}

/// The answer and the error it left, the error cleared first.
fn both<P: Process>(front: &Front<P>, call: impl FnOnce(&Front<P>) -> i64) -> String {
    front.set_last_error(0);
    let value = call(front);
    format!("{value} error {}", front.last_error())
}

#[cfg(windows)]
mod real {
    use std::ffi::c_void;

    pub type H = *mut c_void;
    pub type Filter = unsafe extern "system" fn(*mut c_void) -> i32;
    pub type Kinds = extern "system" fn(H, *const u8, isize) -> i32;
    pub type Names = extern "system" fn(H, *const u8, *const u8, isize) -> i32;
    pub type Languages = extern "system" fn(H, *const u8, *const u8, u16, isize) -> i32;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        pub fn GetLastError() -> u32;
        pub fn SetLastError(code: u32);
        pub fn GetModuleHandleA(name: *const u8) -> H;
        pub fn LoadLibraryA(name: *const u8) -> H;
        pub fn LoadLibraryExA(name: *const u8, file: H, flags: u32) -> H;
        pub fn FreeLibrary(module: H) -> i32;
        pub fn GetProcAddress(module: H, name: *const u8) -> H;
        pub fn GetModuleFileNameA(module: H, into: *mut u8, room: u32) -> u32;
        pub fn GetStartupInfoA(info: *mut [u32; 17]);
        pub fn GetEnvironmentStringsW() -> *mut u16;
        pub fn FreeEnvironmentStringsW(block: *mut u16) -> i32;
        pub fn GetCurrentProcess() -> H;
        pub fn GetVersion() -> u32;
        pub fn ExitProcess(code: u32) -> !;
        pub fn TerminateProcess(process: H, code: u32) -> i32;
        pub fn RaiseException(code: u32, flags: u32, count: u32, arguments: *const usize);
        pub fn SetUnhandledExceptionFilter(filter: Option<Filter>) -> Option<Filter>;
        pub fn IsBadReadPtr(at: *const c_void, size: usize) -> i32;
        pub fn IsBadWritePtr(at: *mut c_void, size: usize) -> i32;
        pub fn FindResourceA(module: H, name: *const u8, kind: *const u8) -> H;
        pub fn FindResourceExA(module: H, kind: *const u8, name: *const u8, language: u16) -> H;
        pub fn SizeofResource(module: H, found: H) -> u32;
        pub fn LoadResource(module: H, found: H) -> H;
        pub fn LockResource(loaded: H) -> *const u8;
        pub fn EnumResourceTypesA(module: H, each: Kinds, state: isize) -> i32;
        pub fn EnumResourceNamesA(module: H, kind: *const u8, each: Names, state: isize) -> i32;
        pub fn EnumResourceLanguagesA(
            module: H,
            kind: *const u8,
            name: *const u8,
            each: Languages,
            state: isize,
        ) -> i32;
        pub fn GetSystemDirectoryA(into: *mut u8, room: u32) -> u32;
        pub fn LoadModule(name: *const u8, parameters: *const c_void) -> u32;
        pub fn CreateProcessA(
            application: *const u8,
            command: *mut u8,
            process_attributes: H,
            thread_attributes: H,
            inherit: i32,
            flags: u32,
            environment: H,
            folder: *const u8,
            startup: *mut [u32; 17],
            information: *mut [usize; 4],
        ) -> i32;
        pub fn WaitForSingleObject(handle: H, milliseconds: u32) -> u32;
        pub fn GetExitCodeProcess(process: H, code: *mut u32) -> i32;
        pub fn CloseHandle(handle: H) -> i32;
    }

    /// The target's answer and the error it left, the error cleared first.
    pub fn both(call: impl FnOnce() -> i64) -> String {
        // SAFETY: both calls take and answer plain numbers.
        unsafe { SetLastError(0) };
        let value = call();
        // SAFETY: as above.
        format!("{value} error {}", unsafe { GetLastError() })
    }

    /// The target's handle as the row's number, 0 kept 0.
    pub fn number(handle: H) -> i64 {
        i64::from(handle as usize as u32)
    }
}

#[test]
fn the_start_up_answers_are_the_row() {
    let front = host_front();
    check("start.command_line", ROW.command_line, front.command_line());
    check("start.version", ROW.version, front.version());
    check("start.own", ROW.own, front.module_handle(None));
    check(
        "start.current_process",
        ROW.current_process,
        front.current_process(),
    );
    let startup = front.startup_info();
    check("start.flags", 0, startup.flags);
    check("start.show", 1, startup.show_window);
    let block = front.environment();
    check("start.block.len", ROW.environment.len(), block.len());
    check(
        "start.block.end",
        "0 0",
        format!("{} {}", block[block.len() - 2], block[block.len() - 1]),
    );
    let wide: Vec<u16> = front.environment_w().collect();
    check("start.block_wide.len", ROW.environment.len(), wide.len());
    check(
        "start.block_wide.end",
        "0 0",
        format!("{} {}", wide[wide.len() - 2], wide[wide.len() - 1]),
    );
    let mut room = [0xff; 32];
    let count = front.module_file_name(0, &mut room);
    check("start.path.count", ROW.path.len(), count);
    check(
        "start.path.same",
        true,
        &room[..ROW.path.len()] == ROW.path.as_bytes(),
    );
    check("start.path.zero", 0, room[ROW.path.len()]);
    check("start.free_block", 1, front.free_environment(0x50_0000));
    check(
        "start.free_block_wide",
        1,
        front.free_environment_w(0x50_0000),
    );
    #[cfg(windows)]
    wine_start();
    done();
}

/// The target's own start-up answers, then a row built of them and read back through the front.
#[cfg(windows)]
fn wine_start() {
    let mut info = [0u32; 17];
    // SAFETY: the record is the 68 bytes the call writes.
    unsafe { real::GetStartupInfoA(&mut info) };
    check("wine.start.cb", 68, info[0]);
    println!("MEASURE w wine.start.flags = {:#x}", info[11]);
    println!("MEASURE w wine.start.show = {}", info[12] & 0xFFFF);
    // SAFETY: the block ends at its double zero, and it is freed once, after the read.
    let units: Vec<u16> = unsafe {
        let block = real::GetEnvironmentStringsW();
        let mut at = 0;
        while *block.add(at) != 0 || *block.add(at + 1) != 0 {
            at += 1;
        }
        let units = std::slice::from_raw_parts(block, at + 2).to_vec();
        real::FreeEnvironmentStringsW(block);
        units
    };
    check(
        "wine.start.block_wide.end",
        "0 0",
        format!("{} {}", units[units.len() - 2], units[units.len() - 1]),
    );
    let narrow = units.iter().all(|unit| *unit < 0x100);
    check("wine.start.block.narrow", true, narrow);
    let bytes: Vec<u8> = units.iter().map(|unit| (*unit & 0xff) as u8).collect();
    let mut path = [0u8; 512];
    // SAFETY: the room given is the array's.
    let length =
        unsafe { real::GetModuleFileNameA(std::ptr::null_mut(), path.as_mut_ptr(), 512) } as usize;
    check("wine.start.path.zero", 0, path[length]);
    let path: String = path[..length]
        .iter()
        .map(|byte| char::from(*byte))
        .collect();
    // SAFETY: a plain number comes back.
    let version = unsafe { real::GetVersion() };
    let row: &'static Row = Box::leak(Box::new(Row {
        version,
        path: Box::leak(path.clone().into_boxed_str()),
        environment: Box::leak(bytes.into_boxed_slice()),
        startup: Startup {
            flags: info[11],
            show_window: (info[12] & 0xFFFF) as u16,
            reserved_count: 0,
            reserved: 0,
        },
        ..ROW
    }));
    let front = Front::new(row, Host::new(entries));
    check("wine.start.version", version, front.version());
    check("wine.start.flags", info[11], front.startup_info().flags);
    check(
        "wine.start.block_wide.same",
        true,
        front.environment_w().eq(units.iter().copied()),
    );
    let mut room = vec![0xff; length + 1];
    let count = front.module_file_name(0, &mut room);
    check("wine.start.path.count", length, count);
    check(
        "wine.start.path.same",
        true,
        room[..length] == *path.as_bytes(),
    );
    check("wine.start.path.front_zero", 0, room[length]);
}

/// The target's answer to a room too small for the path: the room, its error, and no zero.
#[test]
#[ignore = "the front cuts a short room with its zero and no error, where the target fills it and leaves error 122"]
fn a_short_room_is_answered_as_the_target_does() {
    #[cfg(windows)]
    {
        let mut room = [0xffu8; 4];
        let answer = real::both(|| {
            // SAFETY: the room given is the array's.
            i64::from(unsafe {
                real::GetModuleFileNameA(std::ptr::null_mut(), room.as_mut_ptr(), 4)
            })
        });
        check("wine.start.room4", "4 error 122", answer);
        check("wine.start.room4.last_is_zero", false, room[3] == 0);
    }
    let front = host_front();
    let mut room = [0xffu8; 4];
    let answer = both(&front, |front| {
        i64::from(front.module_file_name(0, &mut room))
    });
    check("start.room4", "4 error 122", answer);
    check("start.room4.last_is_zero", false, room[3] == 0);
    done();
}

#[test]
fn the_name_table_answers_as_the_target_does() {
    let front = host_front();
    let found = format!("{} error 0", ROW.modules[0].handle);
    check(
        "names.load_miss",
        "0 error 126",
        both(&front, |f| i64::from(f.load_library(b"NO-SUCH.DLL"))),
    );
    check(
        "names.handle_miss",
        "0 error 126",
        both(&front, |f| i64::from(f.module_handle(Some(b"NO-SUCH.DLL")))),
    );
    let handle = ROW.modules[0].handle;
    check(
        "names.proc_miss",
        "0 error 127",
        both(&front, |f| {
            i64::from(f.proc_address(handle, b"NoSuchEntry"))
        }),
    );
    check(
        "names.proc_found",
        "7 error 0",
        both(&front, |f| i64::from(f.proc_address(handle, b"Sleep"))),
    );
    for (name, asked) in [
        ("names.load_full", &b"KERNEL32.DLL"[..]),
        ("names.load_lower", b"kernel32.dll"),
        ("names.load_no_extension", b"kernel32"),
    ] {
        check(
            name,
            &found,
            both(&front, |f| i64::from(f.load_library(asked))),
        );
    }
    check(
        "names.free_found",
        "1 error 0",
        both(&front, |f| i64::from(f.free_library(handle))),
    );
    #[cfg(windows)]
    wine_names();
    done();
}

#[cfg(windows)]
fn wine_names() {
    use real::{GetModuleHandleA, GetProcAddress, LoadLibraryA, both, number};
    // SAFETY: each name is NUL-ended; each call answers a handle, kept as a number.
    unsafe {
        let kernel = GetModuleHandleA(b"KERNEL32.DLL\0".as_ptr());
        check(
            "wine.names.load_miss",
            "0 error 126",
            both(|| number(LoadLibraryA(b"NO-SUCH.DLL\0".as_ptr()))),
        );
        check(
            "wine.names.handle_miss",
            "0 error 126",
            both(|| number(GetModuleHandleA(b"NO-SUCH.DLL\0".as_ptr()))),
        );
        check(
            "wine.names.proc_miss",
            "0 error 127",
            both(|| number(GetProcAddress(kernel, b"NoSuchEntry\0".as_ptr()))),
        );
        for (name, asked) in [
            ("wine.names.load_full", &b"KERNEL32.DLL\0"[..]),
            ("wine.names.load_lower", b"kernel32.dll\0"),
            ("wine.names.load_no_extension", b"kernel32\0"),
        ] {
            check(name, number(kernel), number(LoadLibraryA(asked.as_ptr())));
        }
    }
}

/// The refusals of a handle the table never gave, beside the target's.
#[test]
#[ignore = "one bad-handle number in the row serves a module and a process, where the target answers 126 for a module and 6 for a process"]
fn a_bad_handle_is_answered_as_the_target_does() {
    let front = host_front();
    let mut room = [0u8; 16];
    check(
        "names.proc_bad",
        "0 error 126",
        both(&front, |f| i64::from(f.proc_address(NO_MODULE, b"Sleep"))),
    );
    check(
        "names.free_bad",
        "0 error 126",
        both(&front, |f| i64::from(f.free_library(NO_MODULE))),
    );
    check(
        "names.file_bad",
        "0 error 126",
        both(&front, |f| {
            i64::from(f.module_file_name(NO_MODULE, &mut room))
        }),
    );
    check(
        "names.terminate_bad",
        "0 error 6",
        both(&front, |f| i64::from(f.terminate(0x1234, 9))),
    );
    #[cfg(windows)]
    {
        use real::{
            FreeLibrary, GetModuleFileNameA, GetProcAddress, H, TerminateProcess, both, number,
        };
        let module = NO_MODULE as usize as H;
        let mut room = [0u8; 16];
        // SAFETY: the target refuses these handles; no memory but the room is written.
        unsafe {
            check(
                "wine.names.proc_bad",
                "0 error 126",
                both(|| number(GetProcAddress(module, b"Sleep\0".as_ptr()))),
            );
            check(
                "wine.names.free_bad",
                "0 error 126",
                both(|| i64::from(FreeLibrary(module))),
            );
            check(
                "wine.names.file_bad",
                "0 error 126",
                both(|| i64::from(GetModuleFileNameA(module, room.as_mut_ptr(), 16))),
            );
            check(
                "wine.names.terminate_bad",
                "0 error 6",
                both(|| i64::from(TerminateProcess(0x1234 as H, 9))),
            );
        }
    }
    done();
}

/// A path whose folder does not exist: the front matches its last part alone.
#[test]
#[ignore = "the front finds a module by a path's last part, whatever the folder, where the target answers 0 and error 126 for a path no file lies at"]
fn a_path_to_no_file_is_answered_as_the_target_does() {
    let front = host_front();
    let asked = b"C:\\NO-SUCH\\KERNEL32.DLL";
    check(
        "names.path_miss",
        "0 error 126",
        both(&front, |f| i64::from(f.load_library(asked))),
    );
    #[cfg(windows)]
    {
        // SAFETY: the name is NUL-ended.
        let answer = real::both(|| {
            real::number(unsafe { real::LoadLibraryA(b"C:\\NO-SUCH\\KERNEL32.DLL\0".as_ptr()) })
        });
        check("wine.names.path_miss", "0 error 126", answer);
    }
    done();
}

fn round<P: Process>(name: &str, front: &Front<P>) {
    front.set_last_error(1234);
    check(&format!("round.{name}.set"), 1234, front.last_error());
    front.set_last_error(0);
    check(&format!("round.{name}.clear"), 0, front.last_error());
    front.load_library(b"NO-SUCH.DLL");
    check(&format!("round.{name}.refused"), 126, front.last_error());
    check(
        &format!("round.{name}.back"),
        126,
        front.process().last_error(),
    );
}

#[test]
fn the_last_error_round_holds_on_each_back() {
    round("host", &host_front());
    round("page", &page_front());
    #[cfg(windows)]
    // SAFETY: both calls take and answer plain numbers.
    unsafe {
        real::SetLastError(1234);
        check("wine.round.set", 1234, real::GetLastError());
    }
    done();
}

#[test]
fn the_bad_pointers_answer_as_the_target_does() {
    let word = [0u8; 4];
    let good = if cfg!(windows) {
        u32::try_from(word.as_ptr() as usize).unwrap_or(0x1000)
    } else {
        0x1000
    };
    let front = host_front();
    check("pointers.read_null_1", 1, front.bad_read(0, 1));
    check("pointers.read_null_0", 0, front.bad_read(0, 0));
    check("pointers.read_good", 0, front.bad_read(good, 4));
    check("pointers.read_wrap", 1, front.bad_read(0xffff_ffff, 2));
    check("pointers.write_good", 0, front.bad_write(good, 4));
    check("pointers.write_null_1", 1, front.bad_write(0, 1));
    let page = page_front();
    check("pointers.page.read_null_1", 1, page.bad_read(0, 1));
    check("pointers.page.read_null_0", 0, page.bad_read(0, 0));
    check("pointers.page.read_good", 0, page.bad_read(0x1000, 4));
    check("pointers.page.read_past", 1, page.bad_read(0x1fff, 2));
    check("pointers.page.write_good", 0, page.bad_write(0x1000, 4));
    check("pointers.page.write_past", 1, page.bad_write(0x17ff, 2));
    check("pointers.page.code_good", 0, page.bad_code(0x1000));
    check("pointers.page.code_past", 1, page.bad_code(0x2000));
    #[cfg(windows)]
    {
        use real::{IsBadReadPtr, IsBadWritePtr};
        use std::ptr::{null, null_mut};
        let mut held = [0u8; 4];
        // SAFETY: the target's own checkers read the map, and touch nothing that faults.
        unsafe {
            check("wine.pointers.read_null_1", 1, IsBadReadPtr(null(), 1));
            check("wine.pointers.read_null_0", 0, IsBadReadPtr(null(), 0));
            check(
                "wine.pointers.read_good",
                0,
                IsBadReadPtr(word.as_ptr().cast(), 4),
            );
            check(
                "wine.pointers.read_wrap",
                1,
                IsBadReadPtr(0xffff_ffffusize as *const _, 2),
            );
            check(
                "wine.pointers.write_good",
                0,
                IsBadWritePtr(held.as_mut_ptr().cast(), 4),
            );
            check(
                "wine.pointers.write_null_1",
                1,
                IsBadWritePtr(null_mut(), 1),
            );
        }
    }
    done();
}

/// The host back holds no memory map, by its own head: it answers from the address alone.
#[test]
#[ignore = "the host back keeps no memory map, so an unmapped address and a code page answer good where the target answers bad"]
fn the_host_back_reads_the_memory_map() {
    let front = host_front();
    #[cfg(windows)]
    let code = {
        use real::{GetModuleHandleA, GetProcAddress};
        // SAFETY: each name is NUL-ended.
        let sleep = unsafe {
            GetProcAddress(
                GetModuleHandleA(b"KERNEL32.DLL\0".as_ptr()),
                b"Sleep\0".as_ptr(),
            )
        };
        // SAFETY: the target's own checkers touch nothing that faults.
        unsafe {
            check("wine.pointers.write_code", 1, real::IsBadWritePtr(sleep, 1));
            check(
                "wine.pointers.read_unmapped",
                1,
                real::IsBadReadPtr(0x10usize as *const _, 1),
            );
        }
        u32::try_from(sleep as usize).unwrap_or(0x1000)
    };
    #[cfg(not(windows))]
    let code = 0x1000;
    check("pointers.write_code", 1, front.bad_write(code, 1));
    check("pointers.read_unmapped", 1, front.bad_read(0x10, 1));
    done();
}

/// Every numbered resource of one DLL in the prefix, the target's reading beside the crate's.
#[cfg(windows)]
#[test]
fn a_resource_reads_as_the_target_reads_it() {
    use real::*;
    use std::ptr::{null, null_mut};

    extern "system" fn kind(_: H, kind: *const u8, state: isize) -> i32 {
        // SAFETY: the state is the list this test hands in.
        let list = unsafe { &mut *(state as *mut Vec<u32>) };
        if (kind as usize) < 0x1_0000 {
            list.push(kind as usize as u32);
        }
        1
    }
    extern "system" fn name(_: H, _: *const u8, name: *const u8, state: isize) -> i32 {
        // SAFETY: as above.
        let list = unsafe { &mut *(state as *mut Vec<u32>) };
        if (name as usize) < 0x1_0000 {
            list.push(name as usize as u32);
        }
        1
    }
    extern "system" fn language(
        _: H,
        _: *const u8,
        _: *const u8,
        language: u16,
        state: isize,
    ) -> i32 {
        // SAFETY: as above.
        let list = unsafe { &mut *(state as *mut Vec<u16>) };
        list.push(language);
        1
    }

    const DLL: &str = "comdlg32.dll";
    let mut folder = [0u8; 260];
    // SAFETY: the room given is the array's.
    let length = unsafe { GetSystemDirectoryA(folder.as_mut_ptr(), 260) } as usize;
    let mut file = String::from_utf8_lossy(&folder[..length]).into_owned();
    file.push('\\');
    file.push_str(DLL);
    let bytes = std::fs::read(&file).expect("the prefix's file reads");
    println!("MEASURE w resource.dll = {DLL} bytes {}", bytes.len());
    let asked = format!("{DLL}\0");
    // SAFETY: the name is NUL-ended; flag 2 maps it as data and runs nothing.
    let module = unsafe { LoadLibraryExA(asked.as_ptr(), null_mut(), 2) };
    check("wine.resource.mapped", true, !module.is_null());

    let (mut compared, mut same, mut default_same) = (0u32, 0u32, 0u32);
    let mut kinds: Vec<u32> = Vec::new();
    // SAFETY: the handle is the module above; each list outlives its call.
    unsafe {
        EnumResourceTypesA(module, kind, std::ptr::from_mut(&mut kinds) as isize);
        for &asked_kind in &kinds {
            let mut ids: Vec<u32> = Vec::new();
            EnumResourceNamesA(
                module,
                asked_kind as usize as *const u8,
                name,
                std::ptr::from_mut(&mut ids) as isize,
            );
            for &id in &ids {
                let mut languages: Vec<u16> = Vec::new();
                EnumResourceLanguagesA(
                    module,
                    asked_kind as usize as *const u8,
                    id as usize as *const u8,
                    language,
                    std::ptr::from_mut(&mut languages) as isize,
                );
                let read = |found: H| -> &[u8] {
                    let size = SizeofResource(module, found) as usize;
                    let at = LockResource(LoadResource(module, found));
                    if at == null() {
                        &[]
                    } else {
                        std::slice::from_raw_parts(at, size)
                    }
                };
                let first = FindResourceExA(
                    module,
                    asked_kind as usize as *const u8,
                    id as usize as *const u8,
                    languages[0],
                );
                let plain = FindResourceA(
                    module,
                    id as usize as *const u8,
                    asked_kind as usize as *const u8,
                );
                let crate_read = kernel_process::resource::find(&bytes, asked_kind, id);
                compared += 1;
                if crate_read == Some(read(first)) {
                    same += 1;
                } else {
                    println!(
                        "MEASURE w resource.differs = kind {asked_kind} id {id} first language {:#x}",
                        languages[0]
                    );
                }
                if crate_read == Some(read(plain)) {
                    default_same += 1;
                }
            }
        }
        FreeLibrary(module);
    }
    println!("MEASURE w resource.kinds = {}", kinds.len());
    println!("MEASURE w resource.default_language_same = {default_same} of {compared}");
    check("resource.some_compared", true, compared > 0);
    check("resource.first_language_same", compared, same);
    done();
}

/// One child's code and everything it wrote.
fn ended(which: &str) -> (i32, String) {
    let out = Command::new(std::env::current_exe().expect("this binary has a path"))
        .args(["--exact", "child", "--nocapture", "--test-threads=1"])
        .env("CHECKS_CHILD", which)
        .output()
        .expect("a child starts");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

/// The line the child wrote for `call`, from its first word, or `none`.
fn line_of(out: &str, call: &str) -> String {
    let wanted = format!("c={call} ");
    out.lines()
        .filter_map(|line| line.find("LINE ").map(|at| &line[at..]))
        .find(|line| line.contains(&wanted))
        .unwrap_or("none")
        .to_string()
}

#[test]
fn an_end_ends_with_its_code_and_its_line() {
    let (code, out) = ended("exit");
    check("ends.exit.code", 3, code);
    check(
        "ends.exit.line",
        "LINE c=K!ExitProcess a=3 by=kernel-process",
        line_of(&out, "K!ExitProcess"),
    );
    let (code, out) = ended("terminate");
    check("ends.terminate.code", 9, code);
    check(
        "ends.terminate.line",
        "LINE c=K!TerminateProcess a=ffffffff,9 v=1 e=0 by=kernel-process",
        line_of(&out, "K!TerminateProcess"),
    );
    let (code, out) = ended("raise");
    check("ends.raise.code", RAISED_READ, code);
    check(
        "ends.raise.line",
        format!("LINE c=K!RaiseException a={RAISED:x},0,0,0 by=kernel-process"),
        line_of(&out, "K!RaiseException"),
    );
    let (code, out) = ended("returns");
    check("ends.returns.code", RETURNED, code);
    check(
        "ends.unwind.line",
        "LINE c=K!RtlUnwind a=1,2,3,4 by=kernel-process",
        line_of(&out, "K!RtlUnwind"),
    );
    check(
        "ends.debug_break.line",
        "LINE c=K!DebugBreak by=kernel-process",
        line_of(&out, "K!DebugBreak"),
    );
    check(
        "ends.filter.first",
        "FILTER first=0",
        out.lines()
            .find_map(|line| line.find("FILTER first").map(|at| &line[at..]))
            .unwrap_or("none"),
    );
    check(
        "ends.filter.second",
        "FILTER second=4096",
        out.lines()
            .find_map(|line| line.find("FILTER second").map(|at| &line[at..]))
            .unwrap_or("none"),
    );
    check(
        "ends.filter.unhandled",
        "FILTER unhandled=1",
        out.lines()
            .find_map(|line| line.find("FILTER unhandled").map(|at| &line[at..]))
            .unwrap_or("none"),
    );
    let front = host_front();
    check(
        "ends.create.miss",
        "0 error 2",
        both(&front, |f| {
            i64::from(f.create_process(None, Some(b"NO-SUCH.EXE")))
        }),
    );
    check(
        "ends.load_module.miss",
        "2 error 2",
        both(&front, |f| i64::from(f.load_module(b"NO-SUCH.EXE", 0))),
    );
    #[cfg(windows)]
    wine_ends();
    done();
}

#[cfg(windows)]
fn wine_ends() {
    use real::*;
    use std::ptr::{null, null_mut};
    check("wine.ends.exit.code", 3, ended("real-exit").0);
    check("wine.ends.terminate.code", 9, ended("real-terminate").0);
    check("wine.ends.raise.code", RAISED_READ, ended("real-raise").0);
    let (code, out) = ended("real-create");
    check(
        "wine.ends.create.own",
        "REAL create own=1 code=3",
        out.lines()
            .find_map(|line| line.find("REAL create").map(|at| &line[at..]))
            .unwrap_or("none"),
    );
    check("wine.ends.create.own.parent", RETURNED, code);
    let mut command = *b"NO-SUCH.EXE\0";
    // SAFETY: each pointer is a NUL-ended name or a record of the size the call writes.
    unsafe {
        let mut startup = [0u32; 17];
        startup[0] = 68;
        let mut information = [0usize; 4];
        let answer = both(|| {
            i64::from(CreateProcessA(
                null(),
                command.as_mut_ptr(),
                null_mut(),
                null_mut(),
                0,
                0,
                null_mut(),
                null(),
                &mut startup,
                &mut information,
            ))
        });
        check("wine.ends.create.miss", "0 error 2", answer);
        let answer = both(|| i64::from(LoadModule(b"NO-SUCH.EXE\0".as_ptr(), null())));
        check("wine.ends.load_module.miss", "2 error 2", answer);
    }
}

/// Answers 1, run the handler: the exception ends the process with its own code.
#[cfg(windows)]
unsafe extern "system" fn run_the_handler(_: *mut std::ffi::c_void) -> i32 {
    1
}

/// The target's own ends, for the parent to read.
#[cfg(windows)]
fn real_child(which: &str) {
    use real::*;
    use std::ptr::{null, null_mut};
    // SAFETY: each end is the call under judgment, in a child of its own; each pointer is a
    // NUL-ended string or a record of the size the call writes.
    unsafe {
        match which {
            "real-exit" => ExitProcess(3),
            "real-terminate" => {
                TerminateProcess(GetCurrentProcess(), 9);
            }
            "real-raise" => {
                SetUnhandledExceptionFilter(Some(run_the_handler));
                RaiseException(RAISED, 0, 0, null());
            }
            "real-create" => {
                std::env::set_var("CHECKS_CHILD", "exit");
                let exe = std::env::current_exe().expect("this binary has a path");
                let mut command = format!(
                    "\"{}\" --exact child --nocapture --test-threads=1\0",
                    exe.display()
                )
                .into_bytes();
                let mut startup = [0u32; 17];
                startup[0] = 68;
                let mut information = [0usize; 4];
                let started = CreateProcessA(
                    null(),
                    command.as_mut_ptr(),
                    null_mut(),
                    null_mut(),
                    0,
                    0,
                    null_mut(),
                    null(),
                    &mut startup,
                    &mut information,
                );
                let process = information[0] as H;
                WaitForSingleObject(process, 30_000);
                let mut code = 0;
                GetExitCodeProcess(process, &mut code);
                CloseHandle(process);
                CloseHandle(information[1] as H);
                println!("REAL create own={started} code={code}");
                std::process::exit(RETURNED);
            }
            _ => {}
        }
    }
}

struct Lines;

impl log::Log for Lines {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &log::Record<'_>) {
        struct Pairs(String);
        impl<'kvs> log::kv::VisitSource<'kvs> for Pairs {
            fn visit_pair(
                &mut self,
                key: log::kv::Key<'kvs>,
                value: log::kv::Value<'kvs>,
            ) -> Result<(), log::kv::Error> {
                self.0.push_str(&format!(" {key}={value}"));
                Ok(())
            }
        }
        let mut pairs = Pairs(String::new());
        record
            .key_values()
            .visit(&mut pairs)
            .expect("a line's pairs read");
        println!("LINE{}", pairs.0);
    }

    fn flush(&self) {}
}

static LINES: Lines = Lines;

/// Not a check of its own: the ends' children, each named by the parent. It does nothing with no
/// name, so a plain run passes it by.
#[test]
fn child() {
    let Ok(which) = std::env::var("CHECKS_CHILD") else {
        return;
    };
    log::set_logger(&LINES).expect("one logger");
    log::set_max_level(log::LevelFilter::Debug);
    #[cfg(windows)]
    real_child(&which);
    let front = host_front();
    match which.as_str() {
        "exit" => front.exit(3),
        "terminate" => {
            front.terminate(ROW.current_process, 9);
        }
        "raise" => front.raise(RAISED, 0, 0, 0),
        "returns" => {
            front.unwind(1, 2, 3, 4);
            front.debug_break();
            front.debug_string(b"a line");
            println!("FILTER first={}", front.set_filter(0x1000));
            println!("FILTER second={}", front.set_filter(0x2000));
            println!("FILTER unhandled={}", front.unhandled_filter(0x12_f000));
            std::process::exit(RETURNED);
        }
        _ => {}
    }
    println!("SURVIVED {which}");
    std::process::exit(SURVIVED);
}
