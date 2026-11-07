// SPDX-License-Identifier: MIT
//! The Kernel's process: the KERNEL32
//! names a program asks of its own process, written once. The start-up's
//! answers (the command line, the module's path, the version) are the
//! game's row, never the host's; a module and its entry names are the
//! stand-in's own name table, [`table::Module`], never a host library
//! load; the last error and the end are the back's, behind [`Process`].
//!
//! Each file the seam serves is a [`table::Row`], kept in its game's rows
//! crate; the back is the page's, [`page::Page`], a host's, or the
//! original's calls under Wine. [`Front`] answers in the
//! file's numbers and writes one `imp.call` line a call: `c` the row's
//! name, `a` the call's numbers, `m` the module name it was given, `q`
//! the entry name it was asked, `s` any other text it was given, `r` the
//! start-up record it wrote, `v` its answer, `e` the error left.
//!
//! The crate names no game and nothing of any one target, so it builds
//! for `wasm32-unknown-unknown`; a game's facts live in its rows crate.
//! `FatalAppExitA`, `FindResourceA`, `LoadResource`, `LockResource`,
//! imported by a game no row serves yet, have no front: each comes with
//! its first measured caller. [`Front::resource`] is no front of theirs:
//! it serves `LoadMenuA` and `DialogBoxParamA`, which read a template with
//! no such call, so it writes no line.
#![cfg_attr(not(any(test, feature = "std")), no_std)]

#[cfg(any(test, feature = "plant"))]
extern crate alloc;

#[cfg(any(test, feature = "std"))]
pub mod host;
pub mod page;
#[cfg(any(test, feature = "plant"))]
pub mod plant;
pub mod resource;
pub mod table;
#[cfg(test)]
mod tests;

use core::cell::Cell;
use core::fmt;
use oxide_trace::Args;
use table::{Module, Row, Startup};

/// What a call asks of the memory it names: `IsBadReadPtr`,
/// `IsBadWritePtr` or `IsBadCodePtr`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    Read,
    Write,
    Run,
}

/// Where one file's process state lives: the seam between the calls and
/// a target.
pub trait Process {
    /// The number `name` in `module` answers on this back, 0 for none:
    /// the function's place in the page's table, or its address.
    fn entry(&self, module: &Module, name: &str) -> u32;
    /// Ends the run with `code`: a host's process exits; on the page the
    /// call returns and the frame loop stops.
    fn end(&self, code: u32);
    /// Keeps `code` as the last error the file reads.
    fn fail(&self, code: u32);
    /// The last error kept.
    fn last_error(&self) -> u32;
    /// Whether all `size` bytes at `address` allow `access`: the memory
    /// map's answer, never a read that could fault.
    fn reaches(&self, address: u32, size: u32, access: Access) -> bool;
    /// The bytes of the module `handle` as they lie on the disc, for
    /// [`resource::find`]; `None` for a module or a back with no disc.
    fn image(&self, handle: u32) -> Option<&[u8]> {
        let _ = handle;
        None
    }
    /// Writes one line around whatever the target must keep across it.
    fn quiet(&self, line: impl FnOnce()) {
        line();
    }
}

/// A borrowed back: each of a program's files keeps its own [`Front`] and
/// all share the one process, its last error included.
impl<P: Process> Process for &P {
    fn entry(&self, module: &Module, name: &str) -> u32 {
        (**self).entry(module, name)
    }

    fn end(&self, code: u32) {
        (**self).end(code);
    }

    fn fail(&self, code: u32) {
        (**self).fail(code);
    }

    fn last_error(&self) -> u32 {
        (**self).last_error()
    }

    fn reaches(&self, address: u32, size: u32, access: Access) -> bool {
        (**self).reaches(address, size, access)
    }

    fn image(&self, handle: u32) -> Option<&[u8]> {
        (**self).image(handle)
    }

    fn quiet(&self, line: impl FnOnce()) {
        (**self).quiet(line);
    }
}

/// Text as the line shows it: a printable byte as its character, any
/// other `\xNN`, so an environment block's NULs stay on one line.
pub struct Text<'a>(pub &'a [u8]);

impl fmt::Display for Text<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|byte| match byte {
            b' '..=b'~' => fmt::Write::write_char(f, char::from(*byte)),
            _ => write!(f, "\\x{byte:02x}"),
        })
    }
}

/// An environment variable as `SetEnvironmentVariableA` was given it:
/// `NAME=VALUE`, or the name alone for a variable taken away.
struct Variable<'a>(&'a [u8], Option<&'a [u8]>);

impl fmt::Display for Variable<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Text(self.0).fmt(f)?;
        self.1.map_or(Ok(()), |value| write!(f, "={}", Text(value)))
    }
}

/// Whether the module name `asked` names `module`, as `LoadLibraryA`
/// matches one: its last path part, any case, `.DLL` taken when it has no
/// extension.
fn names(asked: &[u8], module: &str) -> bool {
    let last = asked
        .rsplit(|byte| matches!(byte, b'\\' | b'/'))
        .next()
        .unwrap_or(asked);
    let module = module.as_bytes();
    if last.contains(&b'.') {
        return last.eq_ignore_ascii_case(module);
    }
    module.len() == last.len() + 4
        && module[..last.len()].eq_ignore_ascii_case(last)
        && module[last.len()..].eq_ignore_ascii_case(b".DLL")
}

/// One file's process calls: its row, and its process.
pub struct Front<P> {
    file: &'static Row,
    process: P,
    /// The filter `SetUnhandledExceptionFilter` was last given, 0 for none.
    filter: Cell<u32>,
}

impl<P: Process> Front<P> {
    pub const fn new(file: &'static Row, process: P) -> Self {
        Self {
            file,
            process,
            filter: Cell::new(0),
        }
    }

    /// The process the calls ask.
    pub const fn process(&self) -> &P {
        &self.process
    }

    /// `GetCommandLineA`: the row's text, whole.
    #[must_use]
    pub fn command_line(&self) -> &'static str {
        let value = self.file.command_line;
        self.process.quiet(|| {
            log::debug!(c = self.file.calls.command_line, v = value,
                by = "kernel-process"; "imp.call");
        });
        value
    }

    /// `GetVersion`: the row's answer, every bit.
    #[must_use]
    pub fn version(&self) -> u32 {
        let value = self.file.version;
        self.process.quiet(|| {
            log::debug!(c = self.file.calls.version, v:% = oxide_trace::__shown!(value),
                by = "kernel-process"; "imp.call");
        });
        value
    }

    /// `GetCurrentProcess`: the row's answer.
    #[must_use]
    pub fn current_process(&self) -> u32 {
        let value = self.file.current_process;
        self.process.quiet(|| {
            log::debug!(
                c = self.file.calls.current_process, v:% = oxide_trace::__shown!(value),
                by = "kernel-process"; "imp.call"
            );
        });
        value
    }

    /// `GetModuleHandleA`: the file's own handle for no name, else the
    /// table's handle for a module it names; 0 and the row's error for a
    /// name it lacks.
    pub fn module_handle(&self, name: Option<&[u8]>) -> u32 {
        let value = match name {
            None => self.file.own,
            Some(name) => self.find(name).map_or(0, |module| module.handle),
        };
        let error = self.refused(value, self.file.errors.module_not_found);
        let asked = name.unwrap_or_default();
        self.process.quiet(|| {
            log::debug!(
                c = self.file.calls.module_handle, m:% = Text(asked),
                v:% = oxide_trace::__shown!(value), e:% = oxide_trace::__shown!(error),
                by = "kernel-process"; "imp.call"
            );
        });
        value
    }

    /// `LoadLibraryA`: the table's handle for a module it names; 0 and the
    /// row's error for one it lacks. Nothing is loaded: the table is the
    /// program's own.
    pub fn load_library(&self, name: &[u8]) -> u32 {
        let value = self.find(name).map_or(0, |module| module.handle);
        let error = self.refused(value, self.file.errors.module_not_found);
        self.process.quiet(|| {
            log::debug!(
                c = self.file.calls.load_library, m:% = Text(name),
                v:% = oxide_trace::__shown!(value), e:% = oxide_trace::__shown!(error),
                by = "kernel-process"; "imp.call"
            );
        });
        value
    }

    /// `GetProcAddress`: the back's number for an entry name the module's
    /// row holds; 0 and the row's error for a handle the table never gave
    /// or a name its row lacks, as Windows answers.
    pub fn proc_address(&self, handle: u32, name: &[u8]) -> u32 {
        let module = self.module(handle);
        let (value, missing) = match module {
            None => (0, self.file.errors.bad_handle),
            Some(module) => {
                let entry = module.entries.iter().find(|entry| entry.as_bytes() == name);
                let value = entry.map_or(0, |entry| self.process.entry(module, entry));
                (value, self.file.errors.name_not_found)
            }
        };
        let error = self.refused(value, missing);
        let arguments = [oxide_trace::__shown!(handle)];
        self.process.quiet(|| {
            log::debug!(
                c = self.file.calls.proc_address, a:% = Args(arguments), q:% = Text(name),
                v:% = oxide_trace::__shown!(value), e:% = oxide_trace::__shown!(error),
                by = "kernel-process"; "imp.call"
            );
        });
        value
    }

    /// The bytes of resource `kind`, `id` in the module `handle`: the file's
    /// own for 0 or its own handle, else one the table gave; `None` for any
    /// other handle, a back with no disc, or a resource the module lacks.
    /// Writes no line and leaves no error: the program made no call.
    #[must_use]
    pub fn resource(&self, handle: u32, kind: u32, id: u32) -> Option<&[u8]> {
        let module = match handle {
            0 => self.file.own,
            own if own == self.file.own => own,
            other => self.module(other)?.handle,
        };
        resource::find(self.process.image(module)?, kind, id)
    }

    /// `FreeLibrary`: 1 for a handle the table gave, its row kept, as the
    /// program never loads one twice; 0 and the row's error for another.
    pub fn free_library(&self, handle: u32) -> i32 {
        let value = i32::from(self.module(handle).is_some());
        let error = self.refused(u32::from(value != 0), self.file.errors.bad_handle);
        let arguments = [oxide_trace::__shown!(handle)];
        self.process.quiet(|| {
            log::debug!(
                c = self.file.calls.free_library, a:% = Args(arguments),
                v:% = oxide_trace::__shown!(value), e:% = oxide_trace::__shown!(error),
                by = "kernel-process"; "imp.call"
            );
        });
        value
    }

    /// `GetModuleFileNameA`: the row's path for the file's own handle (0
    /// or [`Row::own`]), cut to fit `into` with its NUL; the count written,
    /// the NUL left out. Another handle's path no reader reads: 0 and the
    /// row's error.
    pub fn module_file_name(&self, handle: u32, into: &mut [u8]) -> u32 {
        let (value, error) = if (handle == 0 || handle == self.file.own) && !into.is_empty() {
            let path = self.file.path.as_bytes();
            let count = path.len().min(into.len() - 1);
            into[..count].copy_from_slice(&path[..count]);
            into[count] = 0;
            (u32::try_from(count).unwrap_or(u32::MAX), 0)
        } else {
            self.process.fail(self.file.errors.bad_handle);
            (0, self.file.errors.bad_handle)
        };
        let size = into.len();
        let arguments = [oxide_trace::__shown!(handle), oxide_trace::__shown!(size)];
        let written = &into[..into.len().min(value as usize)];
        self.process.quiet(|| {
            log::debug!(
                c = self.file.calls.module_file_name, a:% = Args(arguments),
                m:% = Text(written), v:% = oxide_trace::__shown!(value),
                e:% = oxide_trace::__shown!(error), by = "kernel-process"; "imp.call"
            );
        });
        value
    }

    /// `GetLastError`: the back's.
    #[must_use]
    pub fn last_error(&self) -> u32 {
        let value = self.process.last_error();
        self.process.quiet(|| {
            log::debug!(c = self.file.calls.last_error, v:% = oxide_trace::__shown!(value),
                by = "kernel-process"; "imp.call");
        });
        value
    }

    /// `SetLastError`: kept by the back.
    pub fn set_last_error(&self, code: u32) {
        self.process.fail(code);
        let arguments = [oxide_trace::__shown!(code)];
        self.process.quiet(|| {
            log::debug!(c = self.file.calls.set_last_error, a:% = Args(arguments),
                by = "kernel-process"; "imp.call");
        });
    }

    /// `ExitProcess`: the line first, then the back ends the run.
    pub fn exit(&self, code: u32) {
        let arguments = [oxide_trace::__shown!(code)];
        self.process.quiet(|| {
            log::debug!(c = self.file.calls.exit, a:% = Args(arguments),
                by = "kernel-process"; "imp.call");
        });
        self.process.end(code);
    }

    /// `GetEnvironmentStrings`: the row's block, whole.
    #[must_use]
    pub fn environment(&self) -> &'static [u8] {
        let value = self.file.environment;
        self.process.quiet(|| {
            log::debug!(c = self.file.calls.environment, s:% = Text(value),
                by = "kernel-process"; "imp.call");
        });
        value
    }

    /// `GetEnvironmentStringsW`: the row's block, each byte one UTF-16
    /// unit. The start-up asks for it first, the narrow block only when
    /// this answers none.
    pub fn environment_w(&self) -> impl Iterator<Item = u16> + 'static {
        let value = self.file.environment;
        self.process.quiet(|| {
            log::debug!(c = self.file.calls.environment_w, s:% = Text(value),
                by = "kernel-process"; "imp.call");
        });
        value.iter().map(|byte| u16::from(*byte))
    }

    /// `FreeEnvironmentStringsA`: 1; the block is the row's, never freed.
    pub fn free_environment(&self, block: u32) -> i32 {
        self.freed(self.file.calls.free_environment, block)
    }

    /// `FreeEnvironmentStringsW`: 1, as the narrow one.
    pub fn free_environment_w(&self, block: u32) -> i32 {
        self.freed(self.file.calls.free_environment_w, block)
    }

    /// `GetStartupInfoA`: the row's record.
    #[must_use]
    pub fn startup_info(&self) -> Startup {
        let value = self.file.startup;
        let fields = [
            oxide_trace::__shown!(value.flags),
            oxide_trace::__shown!(value.show_window),
            oxide_trace::__shown!(value.reserved_count),
            oxide_trace::__shown!(value.reserved),
        ];
        self.process.quiet(|| {
            log::debug!(c = self.file.calls.startup_info, r:% = Args(fields),
                by = "kernel-process"; "imp.call");
        });
        value
    }

    /// `SetEnvironmentVariableA`: 1, nothing kept. The C library's own
    /// copy of the block is the one its readers read, and the row's block
    /// is read once, at the start-up, before any call sets one.
    pub fn set_environment(&self, name: &[u8], value: Option<&[u8]>) -> i32 {
        let answer: i32 = 1;
        self.process.quiet(|| {
            log::debug!(
                c = self.file.calls.set_environment, s:% = Variable(name, value),
                v:% = oxide_trace::__shown!(answer), by = "kernel-process"; "imp.call"
            );
        });
        answer
    }

    /// `TerminateProcess`: for the row's own process the line, then the
    /// back ends the run with `code`; 0 and the row's error for another.
    pub fn terminate(&self, process: u32, code: u32) -> i32 {
        let own = process == self.file.current_process;
        let value = i32::from(own);
        let error = self.refused(u32::from(own), self.file.errors.bad_handle);
        let arguments = [oxide_trace::__shown!(process), oxide_trace::__shown!(code)];
        self.process.quiet(|| {
            log::debug!(
                c = self.file.calls.terminate, a:% = Args(arguments),
                v:% = oxide_trace::__shown!(value), e:% = oxide_trace::__shown!(error),
                by = "kernel-process"; "imp.call"
            );
        });
        if own {
            self.process.end(code);
        }
        value
    }

    /// `LoadModule`: another program is never run; the row's file-not-found
    /// error, answered and kept, as Windows answers a name it cannot find.
    pub fn load_module(&self, name: &[u8], parameters: u32) -> u32 {
        let value = self.file.errors.file_not_found;
        self.process.fail(value);
        let arguments = [oxide_trace::__shown!(parameters)];
        self.process.quiet(|| {
            log::debug!(
                c = self.file.calls.load_module, a:% = Args(arguments), m:% = Text(name),
                v:% = oxide_trace::__shown!(value), e:% = oxide_trace::__shown!(value),
                by = "kernel-process"; "imp.call"
            );
        });
        value
    }

    /// `CreateProcessA`: another program is never run; 0 and the row's
    /// file-not-found error.
    pub fn create_process(&self, application: Option<&[u8]>, command: Option<&[u8]>) -> i32 {
        let value: i32 = 0;
        let error = self.refused(0, self.file.errors.file_not_found);
        let (application, command) = (application.unwrap_or_default(), command.unwrap_or_default());
        self.process.quiet(|| {
            log::debug!(
                c = self.file.calls.create_process, m:% = Text(application), s:% = Text(command),
                v:% = oxide_trace::__shown!(value), e:% = oxide_trace::__shown!(error),
                by = "kernel-process"; "imp.call"
            );
        });
        value
    }

    /// `OutputDebugStringA`: the game's text, its line at DEBUG.
    pub fn debug_string(&self, text: &[u8]) {
        self.process.quiet(|| {
            log::debug!(c = self.file.calls.debug_string, s:% = Text(text),
                by = "kernel-process"; "imp.call");
        });
    }

    /// `DebugBreak`: the line; no trap, as no debugger holds the run.
    pub fn debug_break(&self) {
        self.process.quiet(|| {
            log::debug!(c = self.file.calls.debug_break, by = "kernel-process"; "imp.call");
        });
    }

    /// `RaiseException`: the line, then the end the C library's start-up
    /// gives an exception nothing catches: its filter answers the handler,
    /// which ends the run with the exception's code. On the page the call
    /// returns and the frame loop stops.
    pub fn raise(&self, code: u32, flags: u32, count: u32, arguments: u32) {
        let numbers = [
            oxide_trace::__shown!(code),
            oxide_trace::__shown!(flags),
            oxide_trace::__shown!(count),
            oxide_trace::__shown!(arguments),
        ];
        self.process.quiet(|| {
            log::debug!(c = self.file.calls.raise, a:% = Args(numbers),
                by = "kernel-process"; "imp.call");
        });
        self.process.end(code);
    }

    /// `SetUnhandledExceptionFilter`: keeps `filter`, answers the one it
    /// replaces.
    pub fn set_filter(&self, filter: u32) -> u32 {
        let value = self.filter.replace(filter);
        let arguments = [oxide_trace::__shown!(filter)];
        self.process.quiet(|| {
            log::debug!(
                c = self.file.calls.set_filter, a:% = Args(arguments),
                v:% = oxide_trace::__shown!(value), by = "kernel-process"; "imp.call"
            );
        });
        value
    }

    /// `UnhandledExceptionFilter`: 1, run the handler, as Windows answers
    /// with no debugger. The filter kept is never called: no rebuilt code
    /// stands at its address.
    pub fn unhandled_filter(&self, pointers: u32) -> i32 {
        let value: i32 = 1;
        let arguments = [oxide_trace::__shown!(pointers)];
        self.process.quiet(|| {
            log::debug!(
                c = self.file.calls.unhandled_filter, a:% = Args(arguments),
                v:% = oxide_trace::__shown!(value), by = "kernel-process"; "imp.call"
            );
        });
        value
    }

    /// `IsBadReadPtr`: 0 when the back's memory map lets all `size` bytes
    /// be read, else 1.
    pub fn bad_read(&self, address: u32, size: u32) -> i32 {
        self.bad(self.file.calls.bad_read, address, size, Access::Read)
    }

    /// `IsBadWritePtr`: 0 when the map lets all `size` bytes be written.
    pub fn bad_write(&self, address: u32, size: u32) -> i32 {
        self.bad(self.file.calls.bad_write, address, size, Access::Write)
    }

    /// `IsBadCodePtr`: 0 when the map lets the byte at `address` run.
    pub fn bad_code(&self, address: u32) -> i32 {
        self.bad(self.file.calls.bad_code, address, 1, Access::Run)
    }

    /// `RtlUnwind`: the line alone. No rebuilt code sets up a frame it
    /// could unwind; its one caller is the C library's.
    pub fn unwind(&self, frame: u32, target: u32, record: u32, value: u32) {
        let arguments = [
            oxide_trace::__shown!(frame),
            oxide_trace::__shown!(target),
            oxide_trace::__shown!(record),
            oxide_trace::__shown!(value),
        ];
        self.process.quiet(|| {
            log::debug!(c = self.file.calls.unwind, a:% = Args(arguments),
                by = "kernel-process"; "imp.call");
        });
    }

    /// A free of the row's environment block: 1, its line.
    fn freed(&self, call: &'static str, block: u32) -> i32 {
        let value: i32 = 1;
        let arguments = [oxide_trace::__shown!(block)];
        self.process.quiet(|| {
            log::debug!(c = call, a:% = Args(arguments), v:% = oxide_trace::__shown!(value),
                by = "kernel-process"; "imp.call");
        });
        value
    }

    /// One of the `IsBad*Ptr` three: the map's answer, turned.
    fn bad(&self, call: &'static str, address: u32, size: u32, access: Access) -> i32 {
        let value = i32::from(!self.process.reaches(address, size, access));
        let arguments = [oxide_trace::__shown!(address), oxide_trace::__shown!(size)];
        self.process.quiet(|| {
            log::debug!(c = call, a:% = Args(arguments), v:% = oxide_trace::__shown!(value),
                by = "kernel-process"; "imp.call");
        });
        value
    }

    /// The table's row a module name names.
    fn find(&self, name: &[u8]) -> Option<&'static Module> {
        self.file
            .modules
            .iter()
            .find(|module| names(name, module.name))
    }

    /// The table's row a handle names.
    fn module(&self, handle: u32) -> Option<&'static Module> {
        self.file
            .modules
            .iter()
            .find(|module| handle != 0 && module.handle == handle)
    }

    /// Keeps `code` for a refused call, `value` 0; the error the line shows.
    fn refused(&self, value: u32, code: u32) -> u32 {
        if value == 0 {
            self.process.fail(code);
            code
        } else {
            0
        }
    }
}
