// SPDX-License-Identifier: MIT
//! The key map: each key that types a character and its character, the US
//! layout's, never all 256. Its rows: `A` to `Z`, `0` to `9`, space,
//! Return, Escape, Backspace and Tab, each typing its own key number (a
//! letter its capital), as Windows' `MapVirtualKeyA` type 2 answers.
//!
//! The named gap: a shifted digit, the `VK_OEM` keys (the punctuation) and
//! the number pad type nothing here, nor does a key with Ctrl down type its
//! control character.
//!
//! [`scan`] answers each key's scan code. Its named gaps: Pause, Print Screen
//! and Num Lock, which the table gives more than one code, have no row; a
//! key number names no hand, so Shift, Control and Alt read as the left and
//! Return as the main one; the pad's keys with Num Lock off read as the keys
//! they stand for.

/// Set on [`scan`]'s answer for a key the table writes with `0xE0`: bit 24 of
/// the `l` the answer lands in.
const EXTENDED: u32 = 0x100;

/// The character `MapVirtualKeyA` type 2 answers for `key`; 0 for a key
/// with no row.
#[must_use]
pub const fn typed(key: u8) -> u8 {
    match key {
        b'A'..=b'Z' | b'0'..=b'9' | b' ' | b'\r' | 0x1b | 0x08 | b'\t' => key,
        _ => 0,
    }
}

/// The character `TranslateMessage` posts for `key`: a letter in lower
/// case unless `shift`; a shifted digit none, the named gap; 0 for none.
#[must_use]
pub const fn character(key: u8, shift: bool) -> u32 {
    match (typed(key), shift) {
        (letter @ b'A'..=b'Z', false) => letter.to_ascii_lowercase() as u32,
        (b'0'..=b'9', true) => 0,
        (typed, _) => typed as u32,
    }
}

/// The code a key message's `l` holds in bits 16 to 24 for `key`: its "Scan
/// 1 Make" in Microsoft's table of scan codes, bit 8 set where the table
/// writes it with `0xE0`; 0 for a key with no row.
#[must_use]
pub const fn scan(key: u8) -> u32 {
    match key {
        0x08 => 0x0e,  // Backspace
        b'\t' => 0x0f, // Tab
        b'\r' => 0x1c, // Return
        0x10 => 0x2a,  // Shift, the left
        0x11 => 0x1d,  // Control, the left
        0x12 => 0x38,  // Alt, the left
        0x14 => 0x3a,  // Caps Lock
        0x1b => 0x01,  // Escape
        b' ' => 0x39,
        0x21 => EXTENDED | 0x49, // Page Up
        0x22 => EXTENDED | 0x51, // Page Down
        0x23 => EXTENDED | 0x4f, // End
        0x24 => EXTENDED | 0x47, // Home
        0x25 => EXTENDED | 0x4b, // Left
        0x26 => EXTENDED | 0x48, // Up
        0x27 => EXTENDED | 0x4d, // Right
        0x28 => EXTENDED | 0x50, // Down
        0x2d => EXTENDED | 0x52, // Insert
        0x2e => EXTENDED | 0x53, // Delete
        b'1' => 0x02,
        b'2' => 0x03,
        b'3' => 0x04,
        b'4' => 0x05,
        b'5' => 0x06,
        b'6' => 0x07,
        b'7' => 0x08,
        b'8' => 0x09,
        b'9' => 0x0a,
        b'0' => 0x0b,
        b'A' => 0x1e,
        b'B' => 0x30,
        b'C' => 0x2e,
        b'D' => 0x20,
        b'E' => 0x12,
        b'F' => 0x21,
        b'G' => 0x22,
        b'H' => 0x23,
        b'I' => 0x17,
        b'J' => 0x24,
        b'K' => 0x25,
        b'L' => 0x26,
        b'M' => 0x32,
        b'N' => 0x31,
        b'O' => 0x18,
        b'P' => 0x19,
        b'Q' => 0x10,
        b'R' => 0x13,
        b'S' => 0x1f,
        b'T' => 0x14,
        b'U' => 0x16,
        b'V' => 0x2f,
        b'W' => 0x11,
        b'X' => 0x2d,
        b'Y' => 0x15,
        b'Z' => 0x2c,
        0x60 => 0x52,            // the pad's 0
        0x61 => 0x4f,            // the pad's 1
        0x62 => 0x50,            // the pad's 2
        0x63 => 0x51,            // the pad's 3
        0x64 => 0x4b,            // the pad's 4
        0x65 => 0x4c,            // the pad's 5
        0x66 => 0x4d,            // the pad's 6
        0x67 => 0x47,            // the pad's 7
        0x68 => 0x48,            // the pad's 8
        0x69 => 0x49,            // the pad's 9
        0x6a => 0x37,            // the pad's star
        0x6b => 0x4e,            // the pad's plus
        0x6d => 0x4a,            // the pad's dash
        0x6e => 0x53,            // the pad's period
        0x6f => EXTENDED | 0x35, // the pad's slash
        0x70 => 0x3b,            // F1
        0x71 => 0x3c,            // F2
        0x72 => 0x3d,            // F3
        0x73 => 0x3e,            // F4
        0x74 => 0x3f,            // F5
        0x75 => 0x40,            // F6
        0x76 => 0x41,            // F7
        0x77 => 0x42,            // F8
        0x78 => 0x43,            // F9
        0x79 => 0x44,            // F10
        0x7a => 0x57,            // F11
        0x7b => 0x58,            // F12
        0x91 => 0x46,            // Scroll Lock
        0xba => 0x27,            // semicolon
        0xbb => 0x0d,            // equals
        0xbc => 0x33,            // comma
        0xbd => 0x0c,            // dash
        0xbe => 0x34,            // period
        0xbf => 0x35,            // slash
        0xc0 => 0x29,            // grave accent
        0xdb => 0x1a,            // left brace
        0xdc => 0x2b,            // backslash
        0xdd => 0x1b,            // right brace
        0xde => 0x28,            // apostrophe
        _ => 0,
    }
}
