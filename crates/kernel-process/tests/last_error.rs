// SPDX-License-Identifier: MIT
//! One miss, read two ways. The last error is one slot a thread, so a miss in any crate should read
//! back through `kernel_process`. Each back is a test: `CHECK <host> <back>.own` prints what the
//! back kept, `CHECK <host> <back>.through` what the process answers, and the test asserts both.
//! Today each back keeps its own, so those tests are ignored with that reason; `--ignored` keeps
//! them red until the slots are one, and they then pass with no edit here.
#![allow(
    unsafe_code,
    reason = "the judge reaches the target's calls by hand-declared names"
)]

use kernel_files::Files;
use kernel_files::memory::MemoryFolder;
use kernel_process::host::Host;
use kernel_process::page::Page;
use kernel_process::table::{Calls, Errors, Module, Row, Startup};
use kernel_process::{Access, Front};
use kernel_text::Portable;
use kernel_threads::ThreadCalls;
use kernel_time::Times;
use kernel_time::clock::Clock;
use logical_disc::{Disc, Source};

const HOST: &str = if cfg!(windows) { "w" } else { "n" };
const BAD_ARGUMENT: u32 = 87;
const NOT_FOUND: u32 = 2;
/// Open-existing, read, shared, plain: a name no back holds is a miss.
const OPEN_EXISTING: u32 = 3;

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

static FILES: kernel_files::table::Row = kernel_files::table::Row {
    file: "last-error",
    calls: kernel_files::table::Calls {
        open: "K!CreateFileA",
        read: "K!ReadFile",
        write: "K!WriteFile",
        seek: "K!SetFilePointer",
        kind: "K!GetFileType",
        close: "K!CloseHandle",
        delete: "K!DeleteFileA",
        rename: "K!MoveFileA",
        find_first: "K!FindFirstFileA",
        find_next: "K!FindNextFileA",
        find_close: "K!FindClose",
    },
    read: 0x8000_0000,
    write: 0x4000_0000,
    share: 1,
    plain: 0x80,
    creates: [1, 2, 3, 4, 5],
    methods: [0, 1, 2],
    kinds: [1, 2, 3],
    unknown: 0,
    invalid: 0xFFFF_FFFF,
    errors: kernel_files::table::Errors {
        not_found: NOT_FOUND,
        no_path: 3,
        denied: 5,
        bad_handle: 6,
        no_more: 18,
        exists: 80,
        busy: 32,
        taken: 183,
        bad_seek: 131,
        bad_argument: BAD_ARGUMENT,
    },
    name_at: 44,
    name_bytes: 260,
};

static THREADS: kernel_threads::table::Row = kernel_threads::table::Row {
    file: "last-error",
    calls: kernel_threads::table::Calls {
        current_thread_id: "K!GetCurrentThreadId",
        initialize: "K!InitializeCriticalSection",
        enter: "K!EnterCriticalSection",
        leave: "K!LeaveCriticalSection",
        delete: "K!DeleteCriticalSection",
        slot_alloc: "K!TlsAlloc",
        slot_free: "K!TlsFree",
        slot_get: "K!TlsGetValue",
        slot_set: "K!TlsSetValue",
        increment: Some("K!InterlockedIncrement"),
        decrement: Some("K!InterlockedDecrement"),
    },
    bad_argument: BAD_ARGUMENT,
};

static TIME: kernel_time::table::Row = kernel_time::table::Row {
    file: "last-error",
    calls: kernel_time::table::Calls {
        tick_count: "K!GetTickCount",
        time_get_time: "W!timeGetTime",
        sleep: "K!Sleep",
        counter: "K!QueryPerformanceCounter",
        system_time: "K!GetSystemTime",
        local_time: "K!GetLocalTime",
        zone: "K!GetTimeZoneInformation",
        to_local_file_time: "K!FileTimeToLocalFileTime",
        to_system_time: "K!FileTimeToSystemTime",
        time: "C!time",
    },
    zone_unknown: 0,
    bad_argument: BAD_ARGUMENT,
};

static TEXT: kernel_text::table::Row = kernel_text::table::Row {
    file: "last-error",
    calls: kernel_text::table::Calls {
        ansi_code_page: "K!GetACP",
        oem_code_page: "K!GetOEMCP",
        code_page_info: "K!GetCPInfo",
        to_wide: "K!MultiByteToWideChar",
        to_bytes: "K!WideCharToMultiByte",
        string_type: "K!GetStringTypeA",
        string_type_w: "K!GetStringTypeW",
        map_string: "K!LCMapStringA",
        map_string_w: "K!LCMapStringW",
        compare: "K!CompareStringA",
        compare_w: "K!CompareStringW",
        locale_info: "K!GetLocaleInfoA",
        locale_info_w: "K!GetLocaleInfoW",
    },
    ansi: 1252,
    oem: 437,
    locale: 0x409,
    locale_info: &[],
    errors: kernel_text::table::Errors {
        invalid_parameter: BAD_ARGUMENT,
        insufficient_buffer: 122,
        invalid_flags: 1004,
    },
};

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

/// An ISO 9660 volume holding one empty root, the least `Disc::open` takes.
fn empty_disc() -> Vec<u8> {
    const BLOCK: usize = 2048;
    const SECTORS: u32 = 19;
    fn folder(name: u8) -> [u8; 34] {
        let mut bytes = [0; 34];
        bytes[0] = 34;
        bytes[2..6].copy_from_slice(&18u32.to_le_bytes());
        bytes[6..10].copy_from_slice(&18u32.to_be_bytes());
        bytes[10..14].copy_from_slice(&2048u32.to_le_bytes());
        bytes[14..18].copy_from_slice(&2048u32.to_be_bytes());
        bytes[25] = 2;
        bytes[28..30].copy_from_slice(&1u16.to_le_bytes());
        bytes[30..32].copy_from_slice(&1u16.to_be_bytes());
        bytes[32] = 1;
        bytes[33] = name;
        bytes
    }
    let mut image = vec![0; SECTORS as usize * BLOCK];
    let mut put = |sector: usize, at: usize, bytes: &[u8]| {
        let from = sector * BLOCK + at;
        image[from..from + bytes.len()].copy_from_slice(bytes);
    };
    put(16, 0, &[1]);
    put(16, 1, b"CD001");
    put(16, 6, &[1]);
    put(16, 40, &[b' '; 32]);
    put(16, 80, &SECTORS.to_le_bytes());
    put(16, 84, &SECTORS.to_be_bytes());
    put(16, 128, &2048u16.to_le_bytes());
    put(16, 130, &2048u16.to_be_bytes());
    put(16, 156, &folder(0));
    put(17, 0, &[255]);
    put(17, 1, b"CD001");
    put(17, 6, &[1]);
    put(18, 0, &folder(0));
    put(18, 34, &folder(1));
    image
}

#[test]
#[ignore = "the memory folder keeps its own last error, so its miss reads 0 through the process"]
fn a_miss_in_the_memory_folder_reads_through_the_process() {
    let process = page_process();
    let files = Files::new(&FILES, MemoryFolder::default());
    files.open(c"NO.DAT", 0x8000_0000, 1, OPEN_EXISTING, 0x80);
    let own = files.folder().last_error();
    read_both("files.memory", NOT_FOUND, own, process.last_error());
}

#[test]
#[ignore = "the disc keeps its own last error, so its miss reads 0 through the process"]
fn a_miss_on_the_disc_reads_through_the_process() {
    let process = page_process();
    let disc = Disc::open(Source::Iso(empty_disc())).expect("the volume opens");
    let files = Files::new(&FILES, disc);
    files.open(c"NO.DAT", 0x8000_0000, 1, OPEN_EXISTING, 0x80);
    let own = files.folder().last_error();
    read_both("files.disc", NOT_FOUND, own, process.last_error());
}

#[test]
#[ignore = "the thread back keeps its own last error, so its refusal reads 0 through the process"]
fn a_refused_slot_on_the_host_reads_through_the_process() {
    let process = host_process();
    let calls = ThreadCalls::new(&THREADS, kernel_threads::host::Host::new());
    calls.slot_free(0, 0xFFFF_FFF0);
    let own = calls.threads().last_error();
    read_both("threads.host", BAD_ARGUMENT, own, process.last_error());
}

#[test]
#[ignore = "the thread back keeps its own last error, so its refusal reads 0 through the process"]
fn a_refused_slot_on_the_page_reads_through_the_process() {
    let process = page_process();
    let calls = ThreadCalls::new(&THREADS, kernel_threads::page::Page::new());
    calls.slot_free(0, 0xFFFF_FFF0);
    let own = calls.threads().last_error();
    read_both("threads.page", BAD_ARGUMENT, own, process.last_error());
}

#[test]
#[ignore = "the time back keeps its own last error, so its refusal reads 0 through the process"]
fn a_refused_time_on_the_host_reads_through_the_process() {
    let process = host_process();
    let times = Times::new(&TIME, kernel_time::host::Host::new(u64::MAX));
    let mut date = [0; kernel_time::table::DATE_BYTES];
    times.to_system_time(1 << 63, &mut date);
    let own = times.clocks().last_error();
    read_both("time.host", BAD_ARGUMENT, own, process.last_error());
}

#[test]
#[ignore = "the time back keeps its own last error, so its refusal reads 0 through the process"]
fn a_refused_time_on_the_page_reads_through_the_process() {
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
#[ignore = "the host back keeps one slot for the whole process, where the promise is one a thread"]
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
    let front = kernel_text::Front::new(&TEXT, Portable::new(handed));
    assert!(front.code_page_info(9999).is_none());
    let got = HANDED.with(Front::last_error);
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
