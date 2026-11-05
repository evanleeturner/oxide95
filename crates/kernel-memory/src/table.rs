// SPDX-License-Identifier: MIT
//! The files' rows: where each file keeps its heap, its `_newmode` and
//! its new-handler caller, and the address and name of each of its memory
//! helpers. A file the helpers serve is a row, kept in its game's rows
//! crate, and nothing else.

/// One memory helper inside a file: its address, and the name its line
/// carries as `c`, the one `ORDER.tsv` gives it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Helper {
    pub address: u32,
    pub name: &'static str,
}

/// One file's C library, as its memory helpers see it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    /// The file, as its exports folder spells it.
    pub file: &'static str,
    /// The address of the file's heap handle, made once at its start.
    pub heap: u32,
    /// The address of `_newmode`: nonzero, a failed malloc calls the new
    /// handler as operator new does.
    pub new_mode: u32,
    /// The address of `__callnewh`, which calls the new handler under
    /// the C library's own lock.
    pub call_new_handler: u32,
    pub malloc: Helper,
    /// Absent where the file carries no copy: a C program has none.
    pub operator_new: Option<Helper>,
    pub free: Helper,
    /// Absent where the file carries no copy: a C program has none.
    pub operator_delete: Option<Helper>,
    /// Absent where the file carries no copy, or none the rules serve.
    pub calloc: Option<Helper>,
    /// Absent where the file carries no copy, or none the rules serve.
    pub realloc: Option<Helper>,
}

/// A debug C library's list of live blocks, as its make and free keep it:
/// the list's two ends, and the lock both take around it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct List {
    /// The address of the newest block's header, the list's first end.
    pub first: u32,
    /// The address of the oldest block's header, the list's last end.
    pub last: u32,
    /// The address of `__lock`, which takes the C library's lock `number`.
    pub lock: u32,
    /// The address of the call that gives the lock back.
    pub unlock: u32,
    /// The heap's lock among the C library's own, `_HEAP_LOCK`.
    pub number: u32,
}
