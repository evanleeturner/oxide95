// SPDX-License-Identifier: MIT
//! The files' rows: each call's name as the counter labels it, and every
//! number the shell's file calls pass or read back, in the shell's own
//! spelling. A file the seam serves is a row, kept in its game's rows
//! crate, and nothing else.

/// The find data's size: the buffer a find call fills.
pub const FIND_BYTES: usize = 320;

/// Each call's name, the line's `c`: the import as the counter spells it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Calls {
    pub open: &'static str,
    pub read: &'static str,
    pub write: &'static str,
    pub seek: &'static str,
    pub kind: &'static str,
    pub close: &'static str,
    pub delete: &'static str,
    pub rename: &'static str,
    pub find_first: &'static str,
    pub find_next: &'static str,
    pub find_close: &'static str,
}

/// The last error each [`crate::Error`] leaves, as the shell reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Errors {
    pub not_found: u32,
    pub no_path: u32,
    pub denied: u32,
    pub bad_handle: u32,
    pub no_more: u32,
    pub exists: u32,
    pub busy: u32,
    pub taken: u32,
    pub bad_seek: u32,
    pub bad_argument: u32,
}

/// One file's file calls, as the seam sees them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    /// The file, as its exports folder spells it.
    pub file: &'static str,
    pub calls: Calls,
    /// The access bit an open to read passes.
    pub read: u32,
    /// The access bit an open to write passes.
    pub write: u32,
    /// The sharing every open of the shell's passes (a1: 48 of 48).
    pub share: u32,
    /// The attributes every open of the shell's passes (a1: 48 of 48).
    pub plain: u32,
    /// The dispositions, in [`crate::Create`]'s order.
    pub creates: [u32; 5],
    /// The seek methods, in [`crate::Origin`]'s order.
    pub methods: [u32; 3],
    /// The file types, in [`crate::Kind`]'s order.
    pub kinds: [u32; 3],
    /// The type a failed ask answers.
    pub unknown: u32,
    /// A failed open's or find's handle, and a failed seek's position.
    pub invalid: u32,
    pub errors: Errors,
    /// Where the found name sits in the find data, and its room, its NUL
    /// included.
    pub name_at: usize,
    pub name_bytes: usize,
}
