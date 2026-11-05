// SPDX-License-Identifier: MIT
//! The window rows: each call's name as the counter labels it, and the
//! metrics a file is answered beyond the screen's size. A file the seam
//! serves is a row, kept in its game's rows crate, and nothing else.

/// Each call's name, the line's `c`: the import as the counter spells it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Calls {
    pub register_class: &'static str,
    pub create_window: &'static str,
    pub destroy_window: &'static str,
    pub find_window: &'static str,
    pub load_icon: &'static str,
    pub show_window: &'static str,
    pub set_window_pos: &'static str,
    pub adjust_window_rect: &'static str,
    pub get_window_rect: &'static str,
    pub get_window_long: &'static str,
    pub set_window_long: &'static str,
    pub client_to_screen: &'static str,
    pub screen_to_client: &'static str,
    pub set_focus: &'static str,
    pub get_system_metrics: &'static str,
    pub def_window_proc: &'static str,
    pub peek_message: &'static str,
    pub wait_message: &'static str,
    pub translate_message: &'static str,
    pub dispatch_message: &'static str,
    pub send_message: &'static str,
    pub post_message: &'static str,
    pub post_quit_message: &'static str,
    pub get_message_time: &'static str,
}

/// One file's window calls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    pub file: &'static str,
    pub calls: &'static Calls,
    /// `GetSystemMetrics`' answers past indexes 0 and 1, the screen's
    /// size, which the back gives: each index and its answer. An index not
    /// here answers 0, Windows' answer for one it does not know.
    pub metrics: &'static [(i32, i32)],
}
