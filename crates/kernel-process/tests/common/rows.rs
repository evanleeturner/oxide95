// SPDX-License-Identifier: MIT
//! The rows the checks and the page's export ask through: a process, and each crate that keeps a
//! last error, with a disc to miss on.
use kernel_process::table::{Calls, Errors, Module, Row, Startup};

pub const BAD_ARGUMENT: u32 = 87;
pub const NOT_FOUND: u32 = 2;
/// Open-existing, read, shared, plain: a name no back holds is a miss.
pub const OPEN_EXISTING: u32 = 3;

pub static ROW: Row = Row {
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

pub static FILES: kernel_files::table::Row = kernel_files::table::Row {
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

pub static THREADS: kernel_threads::table::Row = kernel_threads::table::Row {
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

pub static TIME: kernel_time::table::Row = kernel_time::table::Row {
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

pub static TEXT: kernel_text::table::Row = kernel_text::table::Row {
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

/// An ISO 9660 volume holding one empty root, the least `Disc::open` takes.
pub fn empty_disc() -> Vec<u8> {
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
