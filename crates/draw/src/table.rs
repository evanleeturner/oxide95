// SPDX-License-Identifier: MIT
//! The drawing rows: each call's name as the counter labels it, the device's
//! answers, and the route a file's pictures take. A file the seam serves is a
//! row, kept in its game's rows crate, and nothing else.

/// Each call's name, the line's `c`: the import as the counter spells it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Calls {
    pub create_palette: &'static str,
    pub select_palette: &'static str,
    pub set_palette_entries: &'static str,
    pub realize_palette: &'static str,
    pub set_dib_color_table: &'static str,
    pub get_system_palette_entries: &'static str,
    pub create_dib_section: &'static str,
    pub bit_blt: &'static str,
    pub set_dib_bits_to_device: &'static str,
    pub stretch_dib_bits: &'static str,
    pub stretch_blt: &'static str,
    pub create_compatible_dc: &'static str,
    pub delete_dc: &'static str,
    pub delete_object: &'static str,
    pub select_object: &'static str,
    pub get_device_caps: &'static str,
    pub get_stock_object: &'static str,
    pub get_dc: &'static str,
    pub release_dc: &'static str,
    pub update_window: &'static str,
    pub begin_paint: &'static str,
    pub end_paint: &'static str,
    pub validate_rect: &'static str,
}

/// How a file's pictures reach its window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    /// `BitBlt` from a buffer it writes through the bits pointer.
    Blit,
    /// `SetDIBitsToDevice` or `StretchDIBits` from its own bits.
    Bits,
    /// It draws no picture.
    None,
}

/// One file's drawing calls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    pub file: &'static str,
    pub calls: &'static Calls,
    /// `GetDeviceCaps`' answers: each index and its answer. An index not
    /// here answers 0, Windows' answer for one it does not know.
    pub caps: &'static [(u32, i32)],
    pub route: Route,
}
