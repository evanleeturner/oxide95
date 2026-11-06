// SPDX-License-Identifier: MIT
//! The joystick rows: each call's name as the counter labels it. A file the
//! front serves is a row, kept in its game's rows crate, and nothing else.

/// Each call's name, the line's `c`: the import as the counter spells it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Calls {
    pub devices: &'static str,
    pub caps: &'static str,
    pub position: &'static str,
}

/// One file's joystick calls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    pub file: &'static str,
    pub calls: Calls,
}
