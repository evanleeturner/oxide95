// SPDX-License-Identifier: MIT
//! The thread rows: each call's name as the counter labels it, and every
//! number the thread calls pass or read back, in the file's own spelling.
//! A file the seam serves is a row, kept in its game's rows crate, and
//! nothing else.

/// Windows' slot count, `TLS_MINIMUM_AVAILABLE`: Win95 keeps no more.
pub const SLOTS: u32 = 64;

/// `TlsAlloc`'s answer when every slot is taken, `TLS_OUT_OF_INDEXES`:
/// the one value its callers test.
pub const NO_SLOT: u32 = 0xFFFF_FFFF;

/// Each call's name, the line's `c`: the import as the counter spells it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Calls {
    pub current_thread_id: &'static str,
    pub initialize: &'static str,
    pub enter: &'static str,
    pub leave: &'static str,
    pub delete: &'static str,
    pub slot_alloc: &'static str,
    pub slot_free: &'static str,
    pub slot_get: &'static str,
    pub slot_set: &'static str,
    /// `None` for a file that imports no counter.
    pub increment: Option<&'static str>,
    pub decrement: Option<&'static str>,
}

/// One file's thread calls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    pub file: &'static str,
    pub calls: Calls,
    /// The last error a refused argument leaves.
    pub bad_argument: u32,
}
