// SPDX-License-Identifier: MIT
//! The dialog rows: each call's name as the counter labels it. A file the seam
//! serves is a row, kept in its game's rows crate, and nothing else.

/// Each call's name, the line's `c`: the import as the counter spells it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Calls {
    pub check_dlg_button: &'static str,
    pub check_menu_item: &'static str,
    pub dialog_box_param: &'static str,
    pub draw_menu_bar: &'static str,
    pub enable_menu_item: &'static str,
    pub end_dialog: &'static str,
    pub get_dlg_item: &'static str,
    pub is_dlg_button_checked: &'static str,
    pub load_menu: &'static str,
    pub set_menu: &'static str,
}

/// One file's dialog calls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    pub file: &'static str,
    pub calls: &'static Calls,
}
