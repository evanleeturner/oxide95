// SPDX-License-Identifier: MIT
//! The key map: each key that types a character and its character, the US
//! layout's, never all 256. Its rows: `A` to `Z`, `0` to `9`, space,
//! Return, Escape, Backspace and Tab, each typing its own key number (a
//! letter its capital), as Windows' `MapVirtualKeyA` type 2 answers.
//!
//! The named gap: a shifted digit, the `VK_OEM` keys (the punctuation) and
//! the number pad type nothing here, nor does a key with Ctrl down type its
//! control character.

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
