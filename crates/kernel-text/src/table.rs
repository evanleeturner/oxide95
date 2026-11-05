// SPDX-License-Identifier: MIT
//! The text rows: each call's name as the counter labels it, the code
//! pages and the locale a file's calls resolve to, and the errors a
//! refusal leaves, in the file's own spelling. A file the seam serves is a
//! row, kept in its game's rows crate, and nothing else.

/// Each call's name, the line's `c`: the import as the counter spells it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Calls {
    pub ansi_code_page: &'static str,
    pub oem_code_page: &'static str,
    pub code_page_info: &'static str,
    pub to_wide: &'static str,
    pub to_bytes: &'static str,
    pub string_type: &'static str,
    pub string_type_w: &'static str,
    pub map_string: &'static str,
    pub map_string_w: &'static str,
    pub compare: &'static str,
    pub compare_w: &'static str,
    pub locale_info: &'static str,
    pub locale_info_w: &'static str,
}

/// The last errors a refusal leaves, Windows' numbers in the file's
/// spelling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Errors {
    /// A code page the back lacks, a locale the row does not serve, or a
    /// locale value it holds no text for.
    pub invalid_parameter: u32,
    /// An answer longer than the buffer it was given.
    pub insufficient_buffer: u32,
    /// A flag the back does not answer: each one a named wall.
    pub invalid_flags: u32,
}

/// One file's text calls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    pub file: &'static str,
    pub calls: Calls,
    /// `GetACP`'s answer; code page 0 (`CP_ACP`) and 3 (`CP_THREAD_ACP`)
    /// resolve to it.
    pub ansi: u32,
    /// `GetOEMCP`'s answer; code page 1 (`CP_OEMCP`) resolves to it.
    pub oem: u32,
    /// The one locale the calls serve: its number, which locale 0, the
    /// user's default (`0x400`) and the system's (`0x800`) resolve to.
    pub locale: u32,
    /// `GetLocaleInfo`'s texts: each value type and its answer, without
    /// the NUL the call adds.
    pub locale_info: &'static [(u32, &'static str)],
    pub errors: Errors,
}

/// A single-byte code page: each byte's UTF-16 unit and the default
/// character a unit with no byte answers. A code page with lead bytes is
/// a wall no row has measured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CodePage {
    pub number: u32,
    /// Bytes `0x80` to `0xFF`'s units; `0x00` to `0x7F` are ASCII's.
    pub high: [u16; 128],
    /// `GetCPInfo`'s `DefaultChar`: the byte a unit with no byte answers.
    pub default: u8,
}

impl CodePage {
    /// The UTF-16 unit `byte` answers.
    #[must_use]
    pub const fn unit(&self, byte: u8) -> u16 {
        if byte < 0x80 {
            byte as u16
        } else {
            self.high[(byte - 0x80) as usize]
        }
    }

    /// The byte `unit` answers, `None` for a unit the page lacks.
    #[must_use]
    pub fn byte(&self, unit: u16) -> Option<u8> {
        match u8::try_from(unit) {
            Ok(byte) if byte < 0x80 => Some(byte),
            _ => (0x80..=0xFF_u8).find(|byte| self.unit(*byte) == unit),
        }
    }
}
