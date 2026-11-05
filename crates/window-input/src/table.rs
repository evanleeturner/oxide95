// SPDX-License-Identifier: MIT
//! The input rows: each call's name as the counter labels it. A file the
//! seam serves is a row, kept in its game's rows crate, and nothing else.

/// Each call's name, the line's `c`: the import as the counter spells it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Calls {
    pub get_cursor_pos: &'static str,
    pub set_cursor_pos: &'static str,
    pub show_cursor: &'static str,
    pub load_cursor: &'static str,
    pub clip_cursor: &'static str,
    pub get_async_key_state: &'static str,
    pub get_key_state: &'static str,
    pub map_virtual_key: &'static str,
}

/// One file's input calls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    pub file: &'static str,
    pub calls: &'static Calls,
}
