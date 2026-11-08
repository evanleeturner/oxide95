// SPDX-License-Identifier: MIT
//! The fronts on a row of no game: the table's own round trip, Wine's
//! answers for code page 1252 (`fixtures/cp1252-wine.txt`) and the
//! refusals. The census rows from real runs replay in each game's rows crate.

mod wine;
use super::*;
use core::cell::Cell;
use table::{Calls, Errors};

thread_local! {
    static ERROR: Cell<u32> = const { Cell::new(0) };
}

fn keep(code: u32) {
    ERROR.with(|error| error.set(code));
}

const CALLS: Calls = Calls {
    ansi_code_page: "acp",
    oem_code_page: "oemcp",
    code_page_info: "cpinfo",
    to_wide: "to_wide",
    to_bytes: "to_bytes",
    string_type: "type",
    string_type_w: "type_w",
    map_string: "map",
    map_string_w: "map_w",
    compare: "compare",
    compare_w: "compare_w",
    locale_info: "info",
    locale_info_w: "info_w",
};

static ROW: Row = Row {
    file: "test",
    calls: CALLS,
    ansi: 1252,
    oem: 437,
    locale: 0x409,
    locale_info: &[(0x1004, "1252")],
    errors: Errors {
        invalid_parameter: 87,
        insufficient_buffer: 122,
        invalid_flags: 1004,
    },
};

fn front() -> Front<Portable> {
    Front::new(&ROW, Portable::new(keep))
}

#[test]
fn every_byte_goes_there_and_back() {
    let bytes: [u8; 256] = core::array::from_fn(|byte| u8::try_from(byte).unwrap_or(0));
    let mut units = [0; 256];
    let mut back = [0; 256];
    assert_eq!(front().to_wide(0, 0, &bytes, Some(&mut units)), 256);
    assert_eq!(units[0x80], 0x20AC);
    assert_eq!(units[0xE9], 0x00E9);
    assert_eq!(
        front().to_bytes(0, 0, &units, Some(&mut back), None),
        (256, false)
    );
    assert_eq!(back, bytes);
}

#[test]
fn a_unit_the_page_lacks_answers_the_default() {
    let mut out = [0; 2];
    assert_eq!(
        front().to_bytes(1252, 0, &[0x0394, 0x41], Some(&mut out), None),
        (2, true)
    );
    assert_eq!(out, [b'?', b'A']);
}

/// The probe's answers under Wine: a `#` header, then one
/// line a call.
const WINE_1252: &str = include_str!("../fixtures/cp1252-wine.txt");

fn hex(word: &str) -> u32 {
    u32::from_str_radix(word, 16).expect("a fixture word is hexadecimal")
}

fn number(word: &str) -> i32 {
    word.parse().expect("a fixture word is a number")
}

#[test]
fn every_answer_equals_wines() {
    let (mut infos, mut bytes, mut units) = (0, 0, 0);
    let mut best_fit = Vec::new();
    for line in WINE_1252.lines().filter(|line| !line.starts_with('#')) {
        let words: Vec<&str> = line.split(' ').collect();
        match words[0] {
            "info" => {
                let page = u32::try_from(number(words[2])).expect("a page");
                let info = front().code_page_info(page);
                let lead: Vec<u8> = (0..12)
                    .map(|at| u8::try_from(hex(&words[10][at * 2..at * 2 + 2])).expect("a byte"))
                    .collect();
                let default = hex(words[8]).to_be_bytes();
                let expected = Info {
                    max_char_size: hex(words[6]),
                    default: [default[2], default[3]],
                    lead_bytes: lead.try_into().expect("12 lead bytes"),
                };
                assert_eq!((number(words[4]), info), (1, Some(expected)), "{line}");
                infos += 1;
            }
            "byte" => {
                let byte = u8::try_from(hex(words[1])).expect("a byte");
                let unit = u16::try_from(hex(words[7])).expect("a unit");
                let mut wide = [0; 1];
                let to_wide = front().to_wide(1252, 0, &[byte], Some(&mut wide));
                assert_eq!((to_wide, wide[0]), (number(words[3]), unit), "{line}");
                let mut back = [0; 1];
                let to_bytes = front().to_bytes(1252, 0, &[unit], Some(&mut back), None);
                let used = number(words[14]) != 0;
                let again = u8::try_from(hex(words[12])).expect("a byte");
                assert_eq!(
                    (to_bytes, back[0]),
                    ((number(words[10]), used), again),
                    "{line}"
                );
                bytes += 1;
            }
            "unit" => {
                let unit = u16::try_from(hex(words[1])).expect("a unit");
                if number(words[7]) == 0 {
                    // Wine's best-fit byte, the crate's named wall: the
                    // back answers the default.
                    best_fit.push(unit);
                } else {
                    let mut out = [0; 1];
                    let answer = front().to_bytes(1252, 0, &[unit], Some(&mut out), None);
                    let byte = u8::try_from(hex(words[5])).expect("a byte");
                    assert_eq!((answer, out[0]), ((number(words[3]), true), byte), "{line}");
                }
                units += 1;
            }
            other => panic!("an unknown line kind {other}: {line}"),
        }
    }
    assert_eq!((infos, bytes, units), (1, 256, 5));
    assert_eq!(best_fit, [0x0100]);
}

#[test]
fn refusals_leave_the_rows_errors() {
    keep(0);
    assert_eq!(front().to_wide(437, 0, b"a", None), 0);
    assert_eq!(ERROR.with(Cell::get), 87);
    assert_eq!(front().to_bytes(0, 0x400, &[0x41], None, None), (0, false));
    assert_eq!(ERROR.with(Cell::get), 1004);
    assert_eq!(front().to_wide(0, 0, b"abc", Some(&mut [0; 2])), 0);
    assert_eq!(ERROR.with(Cell::get), 122);
    assert_eq!(front().compare(0, 0x1000, b"a", b"a"), 0);
    assert_eq!(ERROR.with(Cell::get), 1004);
}

#[test]
fn the_info_the_cases_and_the_classes() {
    let info = front()
        .code_page_info(0)
        .map(|info| (info.max_char_size, info.default));
    assert_eq!(info, Some((1, [b'?', 0])));
    let mut out = [0; 4];
    assert_eq!(
        front().map_string(0, UPPERCASE, b"a\xe9\xff1", Some(&mut out)),
        4
    );
    assert_eq!(&out, b"A\xc9\x9f1");
    let mut classes = [0; 3];
    assert_eq!(
        front().string_type_w(1, &[0x41, 0x20, 0x37], &mut classes),
        1
    );
    assert_eq!(classes, [0x181, 0x048, 0x084]);
    let mut text = [0; 5];
    assert_eq!(front().locale_info(0, 0x8000_1004, Some(&mut text)), 5);
    assert_eq!(&text, b"1252\0");
}
