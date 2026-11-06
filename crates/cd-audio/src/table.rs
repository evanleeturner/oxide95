// SPDX-License-Identifier: MIT
//! The CD-audio rows: each call's name as the counter labels it, and what a
//! file's calls read back. A file the front serves is a row, kept in its
//! game's rows crate, and nothing else.

/// Each call's name, the line's `c`: the import as the counter spells it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Calls {
    pub send_command: &'static str,
    pub aux_devices: &'static str,
    pub error_string: &'static str,
    pub aux_caps: &'static str,
    pub aux_set_volume: &'static str,
    pub aux_get_volume: &'static str,
}

/// One file's CD-audio calls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    pub file: &'static str,
    pub calls: Calls,
    /// How many CD-audio devices the file is told there are.
    pub letters: u32,
    /// How many tracks the disc is said to hold.
    pub tracks: u32,
    /// The name the device goes by; the file hands it back to open it.
    pub device_name: &'static str,
    /// The number an open hands out, and the only one a later call may name.
    pub device_id: u32,
}
