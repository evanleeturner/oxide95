// SPDX-License-Identifier: MIT
//! The Kernel's registry: the ADVAPI32 names a
//! program asks of its saved settings, written once. The settings are the
//! back's, behind [`Settings`]: on the page the tree in memory, its save
//! handed in ([`page::Page`]); on Linux, macOS and Windows one plain text
//! file in the host's settings folder ([`file::SettingsFile`]), never the
//! host's own registry; under Wine the original's calls, logged in the
//! swap.
//!
//! The 5 fronts: `RegOpenKeyExA`, `RegQueryValueExA`, `RegCloseKey`,
//! `RegCreateKeyExA` and `RegSetValueExA`; the files calling each are
//! named in the game's rows crate. `RegCreateKeyA`, imported by a game no
//! row serves yet, has no front: it comes with its first measured caller.
//!
//! Each file the seam serves is a [`table::Row`], kept in its game's rows
//! crate. [`Front`] answers in the file's numbers and writes one
//! `imp.call` line a call: `c` the row's name, `a` the call's numbers,
//! `s` the key path or value name it was given, `o` what it wrote or kept,
//! `v` its answer. A registry call answers its error as its value and
//! leaves no last error, so the line has no `e`.
//!
//! The walls, named: the store holds a `REG_DWORD` of 4 bytes under a key
//! alone, the one root the row names, no volatile key, no class and no
//! security; each other shape answers a logged refusal, the stand-in's
//! own limit ([`table::Errors::invalid_parameter`]).
//!
//! The crate names no game and nothing of any one target, so it builds for
//! `wasm32-unknown-unknown`; a game's facts live in its rows crate.
#![cfg_attr(not(any(test, feature = "std")), no_std)]

#[cfg(any(test, feature = "page"))]
extern crate alloc;

#[cfg(any(test, feature = "std"))]
pub mod file;
#[cfg(any(test, feature = "page"))]
pub mod page;
pub mod table;
#[cfg(test)]
mod test_log;
#[cfg(test)]
mod tests;
#[cfg(any(test, feature = "page"))]
mod tree;

use core::fmt;
use oxide_trace::Args;
use table::Row;

/// The predefined roots, `HKEY_CLASSES_ROOT` to `HKEY_DYN_DATA`.
const ROOTS: core::ops::RangeInclusive<u32> = 0x8000_0000..=0x8000_0006;
/// `REG_DWORD`: the one type the store holds.
const DWORD: u32 = 4;
/// A `REG_DWORD`'s size in bytes.
const SIZE: u32 = 4;
/// `REG_CREATED_NEW_KEY` and `REG_OPENED_EXISTING_KEY`.
const CREATED: u32 = 1;
const OPENED: u32 = 2;

/// Where one program's saved settings live: the seam between the calls
/// and a target. A key is named by its path under the row's root, a held
/// key by the handle the back gave it, never 0 and never a root.
pub trait Settings {
    /// The key `path` under the key `within` (`None` the root), made when
    /// `create` and not there: its handle, and whether it was made. `None`
    /// for a key not there, or, when `create`, one the back could not make
    /// or save.
    fn open(&self, within: Option<u32>, path: &[u8], create: bool) -> Option<(u32, bool)>;
    /// Whether `handle` is a key this back holds open.
    fn holds(&self, handle: u32) -> bool;
    /// The value `name` under the key `handle`, `None` for one not there.
    fn read(&self, handle: u32, name: &[u8]) -> Option<u32>;
    /// Keeps `value` as `name` under the key `handle` and saves it;
    /// `false` for a name the back cannot hold or a save that failed.
    fn write(&self, handle: u32, name: &[u8], value: u32) -> bool;
    /// Lets `handle` go; `false` for one not held.
    fn close(&self, handle: u32) -> bool;
    /// Writes one line around whatever the target must keep across it.
    fn quiet(&self, line: impl FnOnce()) {
        line();
    }
}

/// A borrowed back: each of a program's files keeps its own [`Front`] and
/// all share the one back.
impl<S: Settings> Settings for &S {
    fn open(&self, within: Option<u32>, path: &[u8], create: bool) -> Option<(u32, bool)> {
        (**self).open(within, path, create)
    }

    fn holds(&self, handle: u32) -> bool {
        (**self).holds(handle)
    }

    fn read(&self, handle: u32, name: &[u8]) -> Option<u32> {
        (**self).read(handle, name)
    }

    fn write(&self, handle: u32, name: &[u8], value: u32) -> bool {
        (**self).write(handle, name, value)
    }

    fn close(&self, handle: u32) -> bool {
        (**self).close(handle)
    }

    fn quiet(&self, line: impl FnOnce()) {
        (**self).quiet(line);
    }
}

/// `RegQueryValueExA`'s answer: its code, and the type and size it
/// writes to its pointers, `None` for a refusal that writes neither.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Queried {
    pub code: u32,
    pub wrote: Option<(u32, u32)>,
}

/// Bytes as the line shows them: a printable byte as its character, any
/// other `\xNN`.
pub struct Bytes<'a>(pub &'a [u8]);

impl fmt::Display for Bytes<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|byte| match byte {
            b' '..=b'~' => fmt::Write::write_char(f, char::from(*byte)),
            _ => write!(f, "\\x{byte:02x}"),
        })
    }
}

/// What a call wrote or kept, the line's `o`, its numbers comma-joined:
/// empty for a refusal.
enum Wrote {
    Nothing,
    /// The handle an open wrote.
    Handle(u32),
    /// The handle and disposition a create wrote.
    Made(u32, u32),
    /// The type and size a query wrote, and the value when it had a
    /// buffer.
    Read(u32, u32, Option<u32>),
    /// The value a set kept.
    Kept(u32),
}

impl fmt::Display for Wrote {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Nothing => Ok(()),
            Self::Handle(handle) | Self::Kept(handle) => {
                write!(f, "{}", oxide_trace::__shown!(handle))
            }
            Self::Made(handle, disposition) => write!(
                f,
                "{}",
                Args([
                    oxide_trace::__shown!(handle),
                    oxide_trace::__shown!(disposition)
                ])
            ),
            Self::Read(kind, size, None) => write!(
                f,
                "{}",
                Args([oxide_trace::__shown!(kind), oxide_trace::__shown!(size)])
            ),
            Self::Read(kind, size, Some(value)) => write!(
                f,
                "{}",
                Args([
                    oxide_trace::__shown!(kind),
                    oxide_trace::__shown!(size),
                    oxide_trace::__shown!(value)
                ])
            ),
        }
    }
}

/// One file's registry calls: its row, and its back.
pub struct Front<S> {
    file: &'static Row,
    settings: S,
}

impl<S: Settings> Front<S> {
    pub const fn new(file: &'static Row, settings: S) -> Self {
        Self { file, settings }
    }

    /// The key `within` names for an open or a create: `None` the row's
    /// root, else a held key's handle; another root answers 2, any other
    /// handle 6.
    fn parent(&self, within: u32) -> Result<Option<u32>, u32> {
        if within == self.file.root {
            Ok(None)
        } else if ROOTS.contains(&within) {
            Err(self.file.errors.not_found)
        } else if self.settings.holds(within) {
            Ok(Some(within))
        } else {
            Err(self.file.errors.invalid_handle)
        }
    }

    /// The held key a query or a set names; the row's root holds no
    /// values here, and answers `at_root`.
    fn held(&self, handle: u32, at_root: u32) -> Result<u32, u32> {
        self.parent(handle)?.ok_or(at_root)
    }

    /// `RegOpenKeyExA`: the key `path` under `within`, the row's root or a
    /// key held open; its handle, or the refusal's code. Options 0 alone;
    /// the access asked is logged and not checked: the store keeps no
    /// security.
    pub fn open_key(
        &self,
        within: u32,
        path: &[u8],
        options: u32,
        access: u32,
    ) -> Result<u32, u32> {
        let answer = if options == 0 {
            self.parent(within).and_then(|parent| {
                self.settings
                    .open(parent, path, false)
                    .map(|(handle, _)| handle)
                    .ok_or(self.file.errors.not_found)
            })
        } else {
            Err(self.file.errors.invalid_parameter)
        };
        let (code, wrote) = match answer {
            Ok(handle) => (0, Wrote::Handle(handle)),
            Err(code) => (code, Wrote::Nothing),
        };
        let arguments = [
            oxide_trace::__shown!(within),
            oxide_trace::__shown!(options),
            oxide_trace::__shown!(access),
        ];
        self.settings.quiet(|| {
            log::debug!(
                c = self.file.calls.open_key, a:% = Args(arguments), s:% = Bytes(path),
                o:% = wrote, v:% = oxide_trace::__shown!(code), by = "kernel-registry"; "imp.call"
            );
        });
        answer
    }

    /// `RegCreateKeyExA`: the key `path` under `within`, made when not
    /// there; its handle and disposition (1 made, 2 opened), or the
    /// refusal's code. Options 0 alone; the class and the security the
    /// file gives are not taken and the access is not checked: the store
    /// keeps none of them.
    pub fn create_key(
        &self,
        within: u32,
        path: &[u8],
        options: u32,
        access: u32,
    ) -> Result<(u32, u32), u32> {
        let answer = if options == 0 {
            self.parent(within).and_then(|parent| {
                self.settings
                    .open(parent, path, true)
                    .map(|(handle, made)| (handle, if made { CREATED } else { OPENED }))
                    .ok_or(self.file.errors.cannot_write)
            })
        } else {
            Err(self.file.errors.invalid_parameter)
        };
        let (code, wrote) = match answer {
            Ok((handle, disposition)) => (0, Wrote::Made(handle, disposition)),
            Err(code) => (code, Wrote::Nothing),
        };
        let arguments = [
            oxide_trace::__shown!(within),
            oxide_trace::__shown!(options),
            oxide_trace::__shown!(access),
        ];
        self.settings.quiet(|| {
            log::debug!(
                c = self.file.calls.create_key, a:% = Args(arguments), s:% = Bytes(path),
                o:% = wrote, v:% = oxide_trace::__shown!(code), by = "kernel-registry"; "imp.call"
            );
        });
        answer
    }

    /// `RegQueryValueExA`: the value `name` under the key `handle`, its 4
    /// bytes into `data`. `size` is the size the file gives (`None` for a
    /// null size pointer), `data` its buffer (`None` for a null data
    /// pointer, which answers the type and size alone). A buffer under 4
    /// bytes answers `ERROR_MORE_DATA` and the size 4.
    pub fn query_value(
        &self,
        handle: u32,
        name: &[u8],
        reserved: u32,
        size: Option<u32>,
        data: Option<&mut [u8]>,
    ) -> Queried {
        let errors = self.file.errors;
        // `short`: the value is there and the buffer under its 4 bytes, so
        // the refusal still writes the type and the size it needs.
        let (answer, short) = if reserved != 0 || (data.is_some() && size.is_none()) {
            (Err(errors.invalid_parameter), false)
        } else {
            match self
                .held(handle, errors.not_found)
                .map(|key| self.settings.read(key, name))
            {
                Err(code) => (Err(code), false),
                Ok(None) => (Err(errors.not_found), false),
                Ok(Some(value)) => match data {
                    None => (Ok(None), false),
                    Some(data) => match data.get_mut(..4) {
                        Some(slot) if size.unwrap_or(0) >= SIZE => {
                            slot.copy_from_slice(&value.to_le_bytes());
                            (Ok(Some(value)), false)
                        }
                        _ => (Err(errors.more_data), true),
                    },
                },
            }
        };
        let (queried, wrote) = match answer {
            Ok(read) => (
                Queried {
                    code: 0,
                    wrote: Some((DWORD, SIZE)),
                },
                Wrote::Read(DWORD, SIZE, read),
            ),
            Err(code) if short => (
                Queried {
                    code,
                    wrote: Some((DWORD, SIZE)),
                },
                Wrote::Read(DWORD, SIZE, None),
            ),
            Err(code) => (Queried { code, wrote: None }, Wrote::Nothing),
        };
        let size = size.unwrap_or(0);
        let arguments = [
            oxide_trace::__shown!(handle),
            oxide_trace::__shown!(reserved),
            oxide_trace::__shown!(size),
        ];
        let code = queried.code;
        self.settings.quiet(|| {
            log::debug!(
                c = self.file.calls.query_value, a:% = Args(arguments), s:% = Bytes(name),
                o:% = wrote, v:% = oxide_trace::__shown!(code), by = "kernel-registry"; "imp.call"
            );
        });
        queried
    }

    /// `RegSetValueExA`: `data` kept as the value `name` under the key
    /// `handle`, and saved; the code answered. A `REG_DWORD` of 4 bytes
    /// alone, the stand-in's own limit.
    pub fn set_value(&self, handle: u32, name: &[u8], kind: u32, data: &[u8]) -> u32 {
        let errors = self.file.errors;
        let value = <[u8; 4]>::try_from(data)
            .ok()
            .filter(|_| kind == DWORD)
            .map(u32::from_le_bytes);
        let answer = value.ok_or(errors.invalid_parameter).and_then(|value| {
            let key = self.held(handle, errors.invalid_parameter)?;
            if self.settings.write(key, name, value) {
                Ok(value)
            } else {
                Err(errors.cannot_write)
            }
        });
        let (code, wrote) = match answer {
            Ok(value) => (0, Wrote::Kept(value)),
            Err(code) => (code, Wrote::Nothing),
        };
        let size = u32::try_from(data.len()).unwrap_or(u32::MAX);
        let arguments = [
            oxide_trace::__shown!(handle),
            oxide_trace::__shown!(kind),
            oxide_trace::__shown!(size),
        ];
        self.settings.quiet(|| {
            log::debug!(
                c = self.file.calls.set_value, a:% = Args(arguments), s:% = Bytes(name),
                o:% = wrote, v:% = oxide_trace::__shown!(code), by = "kernel-registry"; "imp.call"
            );
        });
        code
    }

    /// `RegCloseKey`: `handle` let go. A predefined root answers 0, as
    /// Windows does; a handle not held 6.
    pub fn close_key(&self, handle: u32) -> u32 {
        let code = if ROOTS.contains(&handle) || self.settings.close(handle) {
            0
        } else {
            self.file.errors.invalid_handle
        };
        let arguments = [oxide_trace::__shown!(handle)];
        self.settings.quiet(|| {
            log::debug!(
                c = self.file.calls.close_key, a:% = Args(arguments),
                v:% = oxide_trace::__shown!(code), by = "kernel-registry"; "imp.call"
            );
        });
        code
    }
}
