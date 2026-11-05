// SPDX-License-Identifier: MIT
//! The registry rows: each call's name as the counter labels it, the root
//! and key a program keeps its settings under, the values a new player
//! starts with, the settings file's folder, and the codes a refusal
//! answers, in the file's own spelling. A file the seam serves is a row,
//! kept in its game's rows crate, and nothing else.

/// Each call's name, the line's `c`: the import as the counter spells it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Calls {
    pub open_key: &'static str,
    pub query_value: &'static str,
    pub close_key: &'static str,
    pub create_key: &'static str,
    pub set_value: &'static str,
}

/// The codes a refusal answers, Windows' numbers in the file's spelling.
/// A registry call answers its error as its value and leaves no last
/// error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Errors {
    /// A key or value not there, or a predefined root the row does not
    /// name: Windows' answer for a key not there.
    pub not_found: u32,
    /// A handle neither a predefined root nor a key held open.
    pub invalid_handle: u32,
    /// Windows' own answer for a query's reserved word not 0, or a buffer
    /// with no size. The rest is the stand-in's own limit, not Windows'
    /// answer: Windows stores every value type and keeps values at a root
    /// and volatile keys; this store holds a `REG_DWORD` of 4 bytes under
    /// a key alone, and refuses a set of any other type or size, a set at
    /// a root, and options other than 0.
    pub invalid_parameter: u32,
    /// A query's buffer under the value's 4 bytes; the size it needs is
    /// written.
    pub more_data: u32,
    /// A key or value the back could not save, or a name the store cannot
    /// hold (a tab or a line break in it): the stand-in's own answer, where
    /// Windows saves later, out of the call's sight. A value whose save
    /// failed stays held; the next save takes it.
    pub cannot_write: u32,
}

/// One file's registry calls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    pub file: &'static str,
    pub calls: Calls,
    /// The one predefined root the calls serve, by its number; every other
    /// root answers [`Errors::not_found`].
    pub root: u32,
    /// The program's key under the root, as the file spells it.
    pub key: &'static [u8],
    /// The values under [`Row::key`] a new player starts with, before any
    /// save: each name and its `REG_DWORD`.
    pub first: &'static [(&'static [u8], u32)],
    /// The settings file's folder, under the host's settings folder.
    pub folder: &'static str,
    pub errors: Errors,
}
