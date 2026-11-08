// SPDX-License-Identifier: MIT
//! The text fronts against Wine's own answers, on a row of no game. One file
//! with two callers, the test beside it and a page's probe, so a green on a
//! page reads the same rows.
//!
//! Each line of `fixtures/text-wine.txt` is one call Wine answered from a kept
//! error of 0: the words before its `v` ask, the rest answers. A row asks this
//! crate the same and prints both; a red is a value, never a panic, so every
//! row prints on every target. `fixtures/cp1252-units-wine.txt` is the walk
//! over all 65,536 units. `fixtures/text-standing.txt` holds each difference
//! that stands, with its got and its reason: told and not fixed.
//!
//! `Portable` calls no host, so the same arithmetic runs on every target: only
//! Wine's answers judge it, and the targets say the table built and read alike.
//!
//! `TX1`: every byte to a wide unit and every unit to a byte, as Wine converts.
//! `TX2`: two strings compared, with case and without, as Wine orders them.
//! `TX3`: each byte's upper case, lower case and character type, as Wine's.
//! `TX4`: the code page and locale answers, and each refusal's last error.

use super::kernel_text::table::{Calls, Errors, Row};
use super::kernel_text::{Front, Portable};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::fmt::Write as _;

const ANSWERS: &str = include_str!("../../fixtures/text-wine.txt");
const UNITS: &str = include_str!("../../fixtures/cp1252-units-wine.txt");
const HELD: &str = include_str!("../../fixtures/text-standing.txt");

/// Each kind of fixture line and the check it answers to.
const KINDS: &[(&str, &str)] = &[
    ("b0", "TX1"),
    ("b1", "TX1"),
    ("pagewide", "TX1"),
    ("pagebytes", "TX1"),
    ("pageused", "TX1"),
    ("given", "TX1"),
    ("cmpa", "TX2"),
    ("cmpw", "TX2"),
    ("upper", "TX3"),
    ("lower", "TX3"),
    ("type1", "TX3"),
    ("upperw", "TX3"),
    ("lowerw", "TX3"),
    ("typew", "TX3"),
    ("kinda", "TX3"),
    ("kindw", "TX3"),
    ("maploc", "TX3"),
    ("mapwloc", "TX3"),
    ("typeloc", "TX3"),
    ("mapflag", "TX3"),
    ("acp", "TX4"),
    ("cpinfo", "TX4"),
    ("wideflag", "TX4"),
    ("byteflag", "TX4"),
    ("empty", "TX4"),
    ("short", "TX4"),
    ("room0", "TX4"),
    ("alone", "TX4"),
    ("info", "TX4"),
    ("infow", "TX4"),
    ("infoalone", "TX4"),
    ("infoshort", "TX4"),
    ("after", "TX4"),
];

static ROW: Row = Row {
    file: "row",
    calls: Calls {
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
    },
    ansi: 1252,
    oem: 437,
    locale: 0x409,
    locale_info: &[(0x1004, "1252"), (0xb, "437"), (0x3, "ENU")],
    errors: Errors {
        invalid_parameter: 87,
        insufficient_buffer: 122,
        invalid_flags: 1004,
    },
};

thread_local! {
    static ERROR: Cell<u32> = const { Cell::new(0) };
    static WROTE: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn keep(code: u32) {
    ERROR.with(|error| error.set(code));
}

fn front() -> Front<Portable> {
    Front::new(&ROW, Portable::new(keep))
}

/// The call's answer and the error it left, from a kept 0.
fn fresh<T>(call: impl FnOnce(&Front<Portable>) -> T) -> (T, u32) {
    keep(0);
    let value = call(&front());
    (value, ERROR.with(Cell::get))
}

/// One row's outcome, printed by whichever caller runs it.
pub struct Line {
    pub kind: &'static str,
    pub check: &'static str,
    pub name: String,
    pub rule: String,
    pub got: String,
    pub want: String,
}

impl Line {
    pub fn holds(&self) -> bool {
        self.got == self.want
    }

    pub fn text(&self) -> String {
        format!(
            "{} {} {} rule={} = got={} want={}",
            self.kind, self.check, self.name, self.rule, self.got, self.want
        )
    }
}

fn hx(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return "-".into();
    }
    bytes.iter().fold(String::new(), |mut text, byte| {
        let _ = write!(text, "{byte:02x}");
        text
    })
}

fn hw(units: &[u16]) -> String {
    if units.is_empty() {
        return "-".into();
    }
    let words: Vec<_> = units.iter().map(|unit| format!("{unit:04x}")).collect();
    words.join(".")
}

fn hex(word: &str) -> u32 {
    u32::from_str_radix(word, 16).unwrap_or(u32::MAX)
}

fn unit_of(word: &str) -> u16 {
    u16::try_from(hex(word)).unwrap_or(u16::MAX)
}

fn run(word: &str) -> Vec<u8> {
    if word == "-" {
        return Vec::new();
    }
    let pairs = word.as_bytes().chunks(2);
    pairs
        .map(|pair| u8::try_from(hex(core::str::from_utf8(pair).unwrap_or("ff"))).unwrap_or(0xFF))
        .collect()
}

/// A count as answered, as a length within what was written.
fn len(count: i32) -> usize {
    usize::try_from(count).unwrap_or(0)
}

/// Wine's own table for the page, so a wide row's text owes nothing to the crate.
fn wines_units() -> BTreeMap<u8, u16> {
    let mut table = BTreeMap::new();
    for line in ANSWERS.lines() {
        let words: Vec<_> = line.split(' ').collect();
        if let ["b0", byte, "v", "1", "unit", unit, "e", "0"] = words[..] {
            table.insert(run(byte)[0], unit_of(unit));
        }
    }
    table
}

fn page_units(table: &BTreeMap<u8, u16>, text: &[u8]) -> Vec<u16> {
    let wide = |byte: &u8| table.get(byte).copied().unwrap_or(u16::MAX);
    text.iter().map(wide).collect()
}

fn sized(words: &[&str]) -> String {
    let (count, room) = match words[0] {
        "empty" => (0, 8),
        "short" => (5, 2),
        _ => (5, 0),
    };
    let (mut bytes, mut units) = ([0xAAu8; 8], [0xAAAAu16; 8]);
    let (text, wide) = (&b"abcde"[..count], &[0x61u16; 5][..count]);
    match words[1] {
        "wide" => {
            let (n, e) = fresh(|f| f.to_wide(1252, 0, text, Some(&mut units[..room])));
            format!("v {n} e {e} left {}", hw(&units))
        }
        "bytes" => {
            let ((n, _), e) = fresh(|f| f.to_bytes(1252, 0, wide, Some(&mut bytes[..room]), None));
            format!("v {n} e {e} left {}", hx(&bytes))
        }
        "map" => {
            let (n, e) = fresh(|f| f.map_string(0x409, 0x200, text, Some(&mut bytes[..room])));
            format!("v {n} e {e} left {}", hx(&bytes))
        }
        _ => {
            let (n, e) = fresh(|f| f.map_string_w(0x409, 0x200, wide, Some(&mut units[..room])));
            format!("v {n} e {e} left {}", hw(&units))
        }
    }
}

fn alone(what: &str) -> String {
    let (n, e) = fresh(|f| match what {
        "wide" => f.to_wide(1252, 0, b"abcde", None),
        "bytes" => f.to_bytes(1252, 0, &[0x61; 5], None, None).0,
        _ => f.map_string(0x409, 0x200, b"abcde", None),
    });
    format!("v {n} e {e}")
}

/// A refusal's error, then a success after it: does the success leave that alone?
fn after(what: &str) -> String {
    let (mut bytes, mut units) = ([0u8; 8], [0u16; 8]);
    let (refused, kept) = fresh(|f| match what {
        "cpinfo" => i32::from(f.code_page_info(99_999).is_some()),
        "wide" => f.to_wide(99_999, 0, b"abcde", Some(&mut units)),
        "compare" => f.compare(0x409, 0xFFFF_0000, b"a", b"a"),
        _ => f.locale_info(0x409, 0x7777, Some(&mut bytes)),
    });
    let ok = match what {
        "cpinfo" => i32::from(front().code_page_info(1252).is_some()),
        "wide" => front().to_wide(1252, 0, b"abcde", Some(&mut units)),
        "compare" => front().compare(0x409, 0, b"a", b"a"),
        _ => front().locale_info(0x409, 0x1004, Some(&mut bytes)),
    };
    let then = ERROR.with(Cell::get);
    format!("v {refused} e {kept} then v {ok} e {then}")
}

fn cpinfo(page: u32) -> String {
    let (info, e) = fresh(|f| f.code_page_info(page));
    info.map_or_else(
        || format!("v 0 max 0 default 0000 lead {} e {e}", hx(&[0; 12])),
        |info| {
            format!(
                "v 1 max {} default {} lead {} e {e}",
                info.max_char_size,
                hx(&info.default),
                hx(&info.lead_bytes)
            )
        },
    )
}

/// The row's own pages, then both conversions by a page's number.
fn ask_page(words: &[&str]) -> String {
    let page = || words[1].parse().unwrap_or(u32::MAX);
    let (mut bytes, mut wide) = ([0u8; 8], [0u16; 4]);
    let units = [0x20ACu16, 0xE9, 0x100];
    match words[0] {
        "acp" => format!(
            "{} oemcp {}",
            front().ansi_code_page(),
            front().oem_code_page()
        ),
        "cpinfo" => cpinfo(page()),
        "pagewide" => {
            let (n, e) = fresh(|f| f.to_wide(page(), 0, b"\x80\xE9", Some(&mut wide)));
            format!("v {n} out {} e {e}", hw(&wide[..len(n)]))
        }
        "pagebytes" => {
            let ((n, _), e) = fresh(|f| f.to_bytes(page(), 0, &units, Some(&mut bytes), None));
            format!("v {n} out {} e {e}", hx(&bytes[..len(n)]))
        }
        _ => {
            let ((n, used), e) = fresh(|f| f.to_bytes(page(), 0, &units, Some(&mut bytes), None));
            format!("v {n} used {} e {e}", i32::from(used))
        }
    }
}

/// A flag on each conversion, and a default given or not.
fn ask_flags(words: &[&str]) -> String {
    let (mut bytes, mut wide) = ([0u8; 4], [0u16; 4]);
    match words[0] {
        "wideflag" => {
            let (n, e) = fresh(|f| f.to_wide(1252, hex(words[1]), b"a", Some(&mut wide)));
            format!("v {n} e {e}")
        }
        "byteflag" => {
            let flags = hex(words[1]);
            let ((n, _), e) = fresh(|f| f.to_bytes(1252, flags, &[0x61], Some(&mut bytes), None));
            format!("v {n} e {e}")
        }
        _ => {
            let text: Vec<_> = words[1].split('.').map(unit_of).collect();
            let default = (words[2] == "true").then_some(b'#');
            let ((n, used), e) = fresh(|f| f.to_bytes(1252, 0, &text, Some(&mut bytes), default));
            let out = hx(&bytes[..len(n)]);
            format!("v {n} out {out} used {} e {e}", i32::from(used))
        }
    }
}

/// A locale's answer, with room, wide, by count alone and in a short buffer.
fn ask_info(words: &[&str]) -> String {
    let (locale, kind) = (hex(words[1]), hex(words[2]));
    let (mut bytes, mut wide, mut sentinel) = ([0u8; 16], [0u16; 16], [0xAAu8; 8]);
    match words[0] {
        "info" => {
            let (n, e) = fresh(|f| f.locale_info(locale, kind, Some(&mut bytes)));
            format!("v {n} out {} e {e}", hx(&bytes[..len(n)]))
        }
        "infow" => {
            let (n, e) = fresh(|f| f.locale_info_w(locale, kind, Some(&mut wide)));
            format!("v {n} out {} e {e}", hw(&wide[..len(n)]))
        }
        "infoalone" => {
            let (n, e) = fresh(|f| f.locale_info(locale, kind, None));
            format!("v {n} e {e}")
        }
        _ => {
            let (n, e) = fresh(|f| f.locale_info(locale, kind, Some(&mut sentinel[..2])));
            format!("v {n} e {e} left {}", hx(&sentinel))
        }
    }
}

/// One pair, as bytes, or as the units Wine's own table gives those bytes.
fn ask_compare(words: &[&str], table: &BTreeMap<u8, u16>) -> String {
    let (locale, flags) = (hex(words[1]), hex(words[2]));
    let (left, right) = (run(words[3]), run(words[4]));
    let (n, e) = if words[0] == "cmpa" {
        fresh(|f| f.compare(locale, flags, &left, &right))
    } else {
        let (left, right) = (page_units(table, &left), page_units(table, &right));
        fresh(|f| f.compare_w(locale, flags, &left, &right))
    };
    format!("v {n} e {e}")
}

/// The wide sample, each unit alone.
fn ask_wide(words: &[&str]) -> String {
    let unit = [unit_of(words[1])];
    if words[0] == "typew" {
        let mut kind = [0u16; 2];
        let (n, e) = fresh(|f| f.string_type_w(1, &unit, &mut kind));
        return format!("v {n} bits {:04x} e {e}", kind[0]);
    }
    let flags = if words[0] == "upperw" { 0x200 } else { 0x100 };
    let mut wide = [0u16; 4];
    let (n, e) = fresh(|f| f.map_string_w(0x409, flags, &unit, Some(&mut wide)));
    format!("v {n} out {} e {e}", hw(&wide[..len(n)]))
}

/// A kind, a locale or a flag the row may refuse, on both sides.
fn ask_refused(words: &[&str]) -> String {
    let asked = hex(words[1]);
    let (mut bytes, mut wide, mut kind) = ([0u8; 16], [0u16; 4], [0u16; 2]);
    match words[0] {
        "kinda" => {
            let (n, e) = fresh(|f| f.string_type(0x409, asked, b"a", &mut kind));
            format!("v {n} e {e}")
        }
        "kindw" => {
            let (n, e) = fresh(|f| f.string_type_w(asked, &[0x61], &mut kind));
            format!("v {n} e {e}")
        }
        "maploc" => {
            let (n, e) = fresh(|f| f.map_string(asked, 0x200, b"a", Some(&mut bytes[..2])));
            format!("v {n} out {} e {e}", hx(&bytes[..len(n)]))
        }
        "mapwloc" => {
            let (n, e) = fresh(|f| f.map_string_w(asked, 0x200, &[0x61], Some(&mut wide[..2])));
            format!("v {n} e {e}")
        }
        "typeloc" => {
            let (n, e) = fresh(|f| f.string_type(asked, 1, b"a", &mut kind));
            format!("v {n} e {e}")
        }
        _ => {
            let (n, e) = fresh(|f| f.map_string(0x409, asked, b"a", Some(&mut bytes)));
            format!("v {n} e {e}")
        }
    }
}

/// One byte through the page, a case map or its class.
fn ask_byte(words: &[&str]) -> String {
    let byte = run(words[1]);
    let (mut bytes, mut wide, mut kind) = ([0u8; 4], [0u16; 4], [0u16; 2]);
    match words[0] {
        "b0" | "b1" => {
            let flags = u32::from(words[0] == "b1");
            let (n, e) = fresh(|f| f.to_wide(1252, flags, &byte, Some(&mut wide)));
            format!("v {n} unit {} e {e}", hw(&wide[..len(n)]))
        }
        "upper" | "lower" => {
            let flags = if words[0] == "upper" { 0x200 } else { 0x100 };
            let (n, e) = fresh(|f| f.map_string(0x409, flags, &byte, Some(&mut bytes)));
            format!("v {n} out {} e {e}", hx(&bytes[..len(n)]))
        }
        _ => {
            let (n, e) = fresh(|f| f.string_type(0x409, 1, &byte, &mut kind));
            format!("v {n} bits {:04x} e {e}", kind[0])
        }
    }
}

/// What the crate answers to one line's asking words, in the fixture's own form.
fn ask(words: &[&str], table: &BTreeMap<u8, u16>) -> String {
    match words[0] {
        "acp" | "cpinfo" | "pagewide" | "pagebytes" | "pageused" => ask_page(words),
        "wideflag" | "byteflag" | "given" => ask_flags(words),
        "empty" | "short" | "room0" => sized(words),
        "alone" => alone(words[1]),
        "after" => after(words[1]),
        "info" | "infow" | "infoalone" | "infoshort" => ask_info(words),
        "cmpa" | "cmpw" => ask_compare(words, table),
        "upperw" | "lowerw" | "typew" => ask_wide(words),
        "kinda" | "kindw" | "maploc" | "mapwloc" | "typeloc" | "mapflag" => ask_refused(words),
        _ => ask_byte(words),
    }
}

/// The differences that stand: a row's name to its reason and its held got.
fn held() -> BTreeMap<&'static str, (&'static str, &'static str)> {
    let lines = HELD.lines().filter(|line| !line.starts_with('#'));
    let rows = lines.map(|line| {
        let mut parts = line.splitn(3, '|');
        let name = parts.next().unwrap_or("");
        let why = parts.next().unwrap_or("");
        (name, (why, parts.next().unwrap_or("")))
    });
    rows.collect()
}

/// One fixture line asked: the crate's got beside Wine's want.
struct Asked {
    check: &'static str,
    name: String,
    kind: &'static str,
    got: String,
    want: String,
}

/// Every fixture line asked.
fn replayed() -> Vec<Asked> {
    let table = wines_units();
    let mut out = Vec::new();
    for line in ANSWERS.lines() {
        let words: Vec<_> = line.split(' ').collect();
        let Some(&(kind, check)) = KINDS.iter().find(|(kind, _)| *kind == words[0]) else {
            continue;
        };
        let ends = words.iter().position(|word| *word == "v").unwrap_or(1);
        out.push(Asked {
            check,
            name: words[..ends].join("_"),
            kind,
            got: ask(&words, &table),
            want: words[ends..].join(" "),
        });
    }
    out
}

/// The sparse list: unit to its byte, its used flag and whether it is best fit.
fn listed() -> (BTreeMap<u16, (u8, bool, bool)>, String) {
    let mut list = BTreeMap::new();
    let mut head = String::new();
    for line in UNITS.lines().filter(|line| !line.starts_with('#')) {
        let words: Vec<_> = line.split(' ').collect();
        if let ["counts", ..] = words[..] {
            head = words[1..].join(" ");
        } else if let [unit, byte, used, pass] = words[..] {
            let entry = (run(byte)[0], used == "1", pass == "best");
            list.insert(unit_of(unit), entry);
        }
    }
    (list, head)
}

/// The walk over all 65,536 units, each alone: per class, how many were asked
/// and how many answered otherwise, with the first of those. The crate holds
/// no best fit, so at flags 0 it answers what Wine answers under 0x400, which
/// it refuses itself: the wall is exactly the list's best-fit units.
fn walked(list: &BTreeMap<u16, (u8, bool, bool)>, out: &mut Vec<Line>) {
    let mut classes = BTreeMap::new();
    for unit in 0..=u16::MAX {
        let mut byte = [0u8; 4];
        let ((n, used), e) = fresh(|f| f.to_bytes(1252, 0, &[unit], Some(&mut byte), None));
        let listed = list.get(&unit);
        let (class, want) = match listed {
            Some(&(byte, used, false)) => ("page", (1, byte, used, 0)),
            Some(_) => ("wall", (1, b'?', true, 0)),
            None => ("default", (1, b'?', true, 0)),
        };
        let entry = classes.entry(class).or_insert((0u32, 0u32, None));
        entry.0 += 1;
        if (n, byte[0], used, e) != want {
            entry.1 += 1;
            entry.2.get_or_insert(unit);
        }
    }
    for (class, (asked, red, first)) in classes {
        let first = first.map_or_else(|| "-".to_owned(), |unit| format!("{unit:04x}"));
        out.push(Line {
            kind: "CHECK",
            check: "TX1",
            name: format!("units_{class}"),
            rule: "units".into(),
            got: format!("asked {asked} differ {red} first {first}"),
            want: format!("asked {asked} differ 0 first -"),
        });
    }
}

fn units_rows(out: &mut Vec<Line>) {
    let (list, head) = listed();
    let count = |best: bool| list.values().filter(|row| row.2 == best).count();
    let (page, best) = (count(false), count(true));
    out.push(Line {
        kind: "CHECK",
        check: "TX1",
        name: "units_counts".into(),
        rule: "units".into(),
        got: format!("page {page} best {best} default {}", 65_536 - page - best),
        want: head,
    });
    let wall: Vec<_> = list.iter().filter(|(_, row)| row.2).collect();
    let first = wall.first().map_or(0, |(unit, _)| **unit);
    out.push(Line {
        kind: "WALL",
        check: "TX1",
        name: "best_fit".into(),
        rule: "best-fit-units-answer-the-default".into(),
        got: format!("{} units first {first:04x}", wall.len()),
        want: format!("{best} units first {first:04x}"),
    });
    walked(&list, out);
}

struct Counting;

impl log::Log for Counting {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &log::Record<'_>) {
        if record.args().to_string() == "imp.call" {
            let name = record.key_values().get("c".into());
            let name = name.map_or_else(String::new, |value| value.to_string());
            WROTE.with(|wrote| wrote.borrow_mut().push(name));
        }
    }

    fn flush(&self) {}
}

static COUNTING: Counting = Counting;

/// One of the row's calls, made on a fresh front.
type Call<'a> = &'a mut dyn FnMut(&Front<Portable>);

/// Each name in the row writes its one line with its own `c`.
fn names(out: &mut Vec<Line>) {
    let _ = log::set_logger(&COUNTING);
    log::set_max_level(log::LevelFilter::Debug);
    let (mut bytes, mut units, mut kind) = ([0u8; 8], [0u16; 8], [0u16; 2]);
    let calls: [(&str, Call<'_>); 13] = [
        ("acp", &mut |f| {
            let _ = f.ansi_code_page();
        }),
        ("oemcp", &mut |f| {
            let _ = f.oem_code_page();
        }),
        ("cpinfo", &mut |f| {
            let _ = f.code_page_info(1252);
        }),
        ("to_wide", &mut |f| {
            let _ = f.to_wide(1252, 0, b"a", None);
        }),
        ("to_bytes", &mut |f| {
            let _ = f.to_bytes(1252, 0, &[0x61], None, None);
        }),
        ("type", &mut |f| {
            let _ = f.string_type(0x409, 1, b"a", &mut kind);
        }),
        ("type_w", &mut |f| {
            let mut mine = [0; 2];
            let _ = f.string_type_w(1, &[0x61], &mut mine);
        }),
        ("map", &mut |f| {
            let _ = f.map_string(0x409, 0x200, b"a", Some(&mut bytes));
        }),
        ("map_w", &mut |f| {
            let _ = f.map_string_w(0x409, 0x200, &[0x61], Some(&mut units));
        }),
        ("compare", &mut |f| {
            let _ = f.compare(0x409, 0, b"a", b"b");
        }),
        ("compare_w", &mut |f| {
            let _ = f.compare_w(0x409, 0, &[0x61], &[0x62]);
        }),
        ("info", &mut |f| {
            let _ = f.locale_info(0x409, 0x1004, None);
        }),
        ("info_w", &mut |f| {
            let _ = f.locale_info_w(0x409, 0x1004, None);
        }),
    ];
    for (name, call) in calls {
        WROTE.with(|wrote| wrote.borrow_mut().clear());
        call(&front());
        let lines = WROTE.with(|wrote| wrote.borrow().clone());
        let own = lines.iter().filter(|line| line.as_str() == name).count();
        out.push(Line {
            kind: "CHECK",
            check: "TX4",
            name: format!("name_{name}"),
            rule: "names".into(),
            got: format!("lines {} own {own}", lines.len()),
            want: "lines 1 own 1".into(),
        });
    }
    log::set_max_level(log::LevelFilter::Off);
}

/// Every row: each compared with Wine, or where a difference stands, with its
/// held got, so a fix moves it and is seen.
pub fn all() -> Vec<Line> {
    let held = held();
    let mut out = Vec::new();
    for row in replayed() {
        let (mark, want) = held
            .get(row.name.as_str())
            .map_or(("CHECK", row.want), |(_, kept)| {
                ("HELD", (*kept).to_owned())
            });
        out.push(Line {
            kind: mark,
            check: row.check,
            name: row.name,
            rule: row.kind.to_owned(),
            got: row.got,
            want,
        });
    }
    units_rows(&mut out);
    names(&mut out);
    out
}

/// Where the crate differs from Wine and that stands: want is Wine's, got the
/// crate's, the rule the reason.
pub fn standing() -> Vec<Line> {
    let held = held();
    let mut out = Vec::new();
    for row in replayed() {
        if let Some((why, _)) = held.get(row.name.as_str()) {
            out.push(Line {
                kind: "STANDING",
                check: row.check,
                name: row.name,
                rule: (*why).to_owned(),
                got: row.got,
                want: row.want,
            });
        }
    }
    out
}
