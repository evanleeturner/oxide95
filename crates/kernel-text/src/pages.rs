// SPDX-License-Identifier: MIT
//! The code pages the portable back holds, written in Rust; no file's
//! text is copied. Each page's measured answers under Wine check it, and
//! where the two differ Wine's answer wins.

use crate::table::CodePage;

/// Windows-1252's bytes `0x80` to `0x9F`, from Microsoft's Windows-1252
/// chart, cross-read against Unicode's `MICSFT/WINDOWS/CP1252.TXT`
/// (version 2.01, 1998-04-15). The chart leaves `0x81`, `0x8D`, `0x8F`,
/// `0x90` and `0x9D` unassigned; each is the unit of its own number, as
/// Wine answers them (`fixtures/cp1252-wine.txt`, all 256 bytes).
const CP1252_C1: [u16; 32] = [
    0x20AC, 0x0081, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, //
    0x02C6, 0x2030, 0x0160, 0x2039, 0x0152, 0x008D, 0x017D, 0x008F, //
    0x0090, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, //
    0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0x009D, 0x017E, 0x0178, //
];

/// Bytes `0x80` to `0xFF`: [`CP1252_C1`], then `0xA0` to `0xFF` as the
/// units of their own numbers, Latin-1's.
#[allow(clippy::cast_possible_truncation, reason = "the index stays below 128")]
const fn cp1252_high() -> [u16; 128] {
    let mut high = [0; 128];
    let mut index = 0;
    while index < 128 {
        high[index] = if index < 32 {
            CP1252_C1[index]
        } else {
            0x80 + index as u16
        };
        index += 1;
    }
    high
}

/// Windows-1252; its default character `?`, `GetCPInfo`'s record under
/// Wine (`fixtures/cp1252-wine.txt`).
pub const CP1252: CodePage = CodePage {
    number: 1252,
    high: cp1252_high(),
    default: b'?',
};

/// Every code page the portable back holds.
pub static ALL: [CodePage; 1] = [CP1252];
