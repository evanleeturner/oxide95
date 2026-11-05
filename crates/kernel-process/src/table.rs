// SPDX-License-Identifier: MIT
//! The process rows: each call's name as the counter labels it, and every
//! answer the start-up's readers read back, in the file's own spelling. A
//! file the seam serves is a row, kept in its game's rows crate, and
//! nothing else.

/// Each call's name, the line's `c`: the import as the counter spells it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Calls {
    pub command_line: &'static str,
    pub module_handle: &'static str,
    pub module_file_name: &'static str,
    pub version: &'static str,
    pub current_process: &'static str,
    pub load_library: &'static str,
    pub proc_address: &'static str,
    pub free_library: &'static str,
    pub last_error: &'static str,
    pub set_last_error: &'static str,
    pub exit: &'static str,
    pub environment: &'static str,
    pub environment_w: &'static str,
    pub free_environment: &'static str,
    pub free_environment_w: &'static str,
    pub startup_info: &'static str,
    pub set_environment: &'static str,
    pub terminate: &'static str,
    pub load_module: &'static str,
    pub create_process: &'static str,
    pub debug_string: &'static str,
    pub debug_break: &'static str,
    pub raise: &'static str,
    pub set_filter: &'static str,
    pub unhandled_filter: &'static str,
    pub bad_read: &'static str,
    pub bad_write: &'static str,
    pub bad_code: &'static str,
    pub unwind: &'static str,
}

/// A module the name table holds: its name as the program asks for it,
/// the handle it answers, and the entry names it can find.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Module {
    pub name: &'static str,
    pub handle: u32,
    pub entries: &'static [&'static str],
}

/// The last errors a refusal leaves, Windows' numbers in the file's
/// spelling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Errors {
    /// A module the table has no row for.
    pub module_not_found: u32,
    /// An entry name its module's row does not hold.
    pub name_not_found: u32,
    /// A handle the table never gave.
    pub bad_handle: u32,
    /// Another program's file, never run.
    pub file_not_found: u32,
}

/// `GetStartupInfoA`'s fields the start-up reads; the record's others
/// are written 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Startup {
    /// `dwFlags`: bit 0 says [`Startup::show_window`] holds.
    pub flags: u32,
    /// `wShowWindow`: `WinMain`'s fourth argument when bit 0 is set.
    pub show_window: u16,
    /// `cbReserved2`: the inherited handles' block's length, 0 for none.
    pub reserved_count: u16,
    /// `lpReserved2`: that block's address, 0 for none.
    pub reserved: u32,
}

/// One file's process calls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    pub file: &'static str,
    pub calls: Calls,
    /// `GetCommandLineA`'s text, whole.
    pub command_line: &'static str,
    /// The file's own module handle: `GetModuleHandleA` with no name.
    pub own: u32,
    /// The program's path, `GetModuleFileNameA`'s answer for [`Row::own`].
    pub path: &'static str,
    /// `GetVersion`'s answer, every bit: its readers split it three ways.
    pub version: u32,
    /// `GetCurrentProcess`'s answer.
    pub current_process: u32,
    /// The environment block: each `NAME=VALUE` NUL-ended, a second NUL
    /// after the last. `GetEnvironmentStringsW` widens each byte.
    pub environment: &'static [u8],
    /// `GetStartupInfoA`'s record.
    pub startup: Startup,
    /// The program's name table: every module a file of it may ask for.
    pub modules: &'static [Module],
    pub errors: Errors,
}
