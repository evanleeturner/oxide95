// SPDX-License-Identifier: MIT
//! The time rows: each call's name as the counter labels it, and every
//! number the time calls pass or read back, in the file's own spelling. A
//! file the seam serves is a row, kept in its game's rows crate, and
//! nothing else.

/// A `SYSTEMTIME`'s size: 8 words.
pub const DATE_BYTES: usize = 16;

/// A `TIME_ZONE_INFORMATION`'s size.
pub const ZONE_BYTES: usize = 172;

/// Each call's name, the line's `c`: the import as the counter spells it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Calls {
    pub tick_count: &'static str,
    pub sleep: &'static str,
    pub counter: &'static str,
    pub system_time: &'static str,
    pub local_time: &'static str,
    pub zone: &'static str,
    pub to_local_file_time: &'static str,
    pub to_system_time: &'static str,
    /// The C library's `time`, a stand-in's name rather than an import's:
    /// it reads the same wall clock.
    pub time: &'static str,
}

/// One file's time calls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    pub file: &'static str,
    pub calls: Calls,
    /// The zone call's one answer, `TIME_ZONE_ID_UNKNOWN` in the file's
    /// spelling: no zone in force, on every target.
    pub zone_unknown: u32,
    /// The last error a refused argument leaves.
    pub bad_argument: u32,
}
