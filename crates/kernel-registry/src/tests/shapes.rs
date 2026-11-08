//! RG1's rows: each shape of the five registry calls a file may hand,
//! answered by the crate in the answer form of its judge, a probe of
//! Wine 9.0's own `advapi32` (`probe-kernel-rg`), row for row in its
//! order. One file, three sides: `tests.rs` runs it natively (N) and
//! under Wine at 32 bits (W), and the page probe includes it by `#[path]`
//! (P), so no side can answer from a copy of its own.
//!
//! An answer prints the code, and the fields the call may write against
//! a sentinel each starts as: a handle `set` or `kept`, a disposition, a
//! kind, a room, and the buffer's 4 bytes. A handle's number is never
//! printed. The crate writes no field on a refusal: its glue owns them.

use super::{Calls, Errors, Front, Row, Settings};
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

/// Each field a call may write starts as this; `-` prints one left alone.
const S: u32 = 0xa5a5_a5a5;
/// `HKEY_CURRENT_USER`, the judge's root.
const HKCU: u32 = 0x8000_0001;
/// The access every open and create asks, the judge's.
const ACCESS: u32 = 0xf_003f;

/// A row of no game: the judge's root, its key without the run's pid,
/// no first values.
pub(crate) static ROW: Row = Row {
    file: "shapes",
    calls: Calls {
        open_key: "open",
        query_value: "query",
        close_key: "close",
        create_key: "create",
        set_value: "set",
    },
    root: HKCU,
    key: b"Software\\rg",
    first: &[],
    folder: "rg-shapes",
    errors: Errors {
        not_found: 2,
        invalid_handle: 6,
        invalid_parameter: 87,
        more_data: 234,
        cannot_write: 1013,
    },
};

/// Wine's 49 answers, harvested by script from the judge's first run
/// (2026-11-07 15:16, Wine 9.0, the judge's `.exe` sha256
/// `74319f2f56def10c`), in its order.
pub(crate) const JUDGE: [(&str, &str); 49] = [
    ("open.miss", "code 2 handle zero"),
    ("create.new", "code 0 handle set disposition 1"),
    ("create.again", "code 0 handle set disposition 2"),
    ("close.again", "code 0"),
    ("set", "code 0"),
    ("query.room4", "code 0 kind 4 room 4 buf 07000000"),
    ("query.room2", "code 234 kind 4 room 4 buf a5a5a5a5"),
    ("query.room0", "code 234 kind 4 room 4 buf a5a5a5a5"),
    ("query.size-only", "code 0 kind 4 room 4 buf a5a5a5a5"),
    ("query.no-size", "code 87 kind - room - buf a5a5a5a5"),
    (
        "query.no-size-no-buffer",
        "code 0 kind 4 room - buf a5a5a5a5",
    ),
    ("query.reserved", "code 87 kind - room 4 buf a5a5a5a5"),
    ("query.miss", "code 2 kind 0 room 4 buf a5a5a5a5"),
    ("query.default-before", "code 2 kind 0 room 4 buf a5a5a5a5"),
    ("set.default", "code 0"),
    ("query.default", "code 0 kind 4 room 4 buf 09000000"),
    ("set.kind3", "code 0"),
    ("set.dword-size2", "code 0"),
    ("set.tab", "code 0"),
    ("query.tab", "code 0 kind 4 room 4 buf 07000000"),
    ("set.case", "code 0"),
    ("query.case", "code 0 kind 4 room 4 buf 08000000"),
    ("open.options1", "code 0 handle set"),
    ("open.options8", "code 0 handle set"),
    ("create.options1", "code 0 handle set disposition 1"),
    ("create.options8", "code 0 handle set disposition 1"),
    ("set.root", "code 0"),
    ("query.root", "code 0 kind 4 room 4 buf 07000000"),
    ("create.sub", "code 0 handle set disposition 1"),
    ("close.sub", "code 0"),
    ("open.lead", "code 161 handle zero"),
    ("open.trail", "code 0 handle set"),
    ("open.double", "code 0 handle set"),
    ("open.empty", "code 0 handle set"),
    ("open.under-key", "code 0 handle set"),
    ("open.ascii-case", "code 0 handle set"),
    ("create.lead", "code 161 handle zero disposition -"),
    ("create.e-acute", "code 0 handle set disposition 1"),
    ("open.E-acute", "code 0 handle set"),
    ("close", "code 0"),
    ("close.twice", "code 6"),
    ("query.closed", "code 6 kind 0 room 4 buf a5a5a5a5"),
    ("close.root0", "code 0"),
    ("close.root1", "code 0"),
    ("close.root2", "code 0"),
    ("close.root3", "code 0"),
    ("close.root4", "code 0"),
    ("close.root5", "code 0"),
    ("close.root6", "code 0"),
];

/// The rows the crate answers otherwise by design: the row, the crate's
/// answer, the number of the expected difference it stands for, of the
/// ten the check named before its data (0 for one outside them), and
/// why.
pub(crate) const STANDING: &[(&str, &str, u8, &str)] = &[
    (
        "open.miss",
        "code 2 handle kept",
        0,
        "a refusal's handle: Wine writes 0, the crate writes none; its glue owns the field",
    ),
    (
        "create.new",
        "code 0 handle set disposition 2",
        0,
        "the row's own key is there from the start, so a first create opens it: 2, where Wine makes it: 1",
    ),
    (
        "query.miss",
        "code 2 kind - room 4 buf a5a5a5a5",
        0,
        "a miss's kind: Wine writes 0, the crate writes none; its glue owns the field",
    ),
    (
        "query.default-before",
        "code 2 kind - room 4 buf a5a5a5a5",
        0,
        "a miss's kind: Wine writes 0, the crate writes none; its glue owns the field",
    ),
    (
        "set.kind3",
        "code 87",
        2,
        "a REG_SZ: the store holds a REG_DWORD alone, and answers 87",
    ),
    (
        "set.dword-size2",
        "code 87",
        2,
        "a REG_DWORD of 2 bytes: the store takes 4 bytes alone, and answers 87",
    ),
    (
        "set.tab",
        "code 1013",
        7,
        "a tab in a name: the settings text cannot hold it, and answers 1013",
    ),
    (
        "query.tab",
        "code 2 kind - room 4 buf a5a5a5a5",
        7,
        "the name with a tab was refused, so it is not there",
    ),
    (
        "open.options1",
        "code 87 handle kept",
        4,
        "an option but 0 answers 87; Wine opens the key",
    ),
    (
        "open.options8",
        "code 87 handle kept",
        4,
        "an option but 0 answers 87; Wine opens the key",
    ),
    (
        "create.options1",
        "code 87 handle kept disposition -",
        4,
        "an option but 0 answers 87; Wine makes the key",
    ),
    (
        "create.options8",
        "code 87 handle kept disposition -",
        4,
        "an option but 0 answers 87; Wine makes the key",
    ),
    (
        "set.root",
        "code 87",
        3,
        "the row's root holds no values, and a set there answers 87",
    ),
    (
        "query.root",
        "code 2 kind - room 4 buf a5a5a5a5",
        3,
        "the row's root holds no values, and a query there answers 2",
    ),
    (
        "open.lead",
        "code 2 handle kept",
        5,
        "a leading backslash: Wine answers 161 and writes 0, the crate finds no key: 2",
    ),
    (
        "open.trail",
        "code 2 handle kept",
        5,
        "a trailing backslash: Wine opens the key, the crate answers 2",
    ),
    (
        "open.double",
        "code 2 handle kept",
        5,
        "a doubled backslash: Wine opens the key, the crate answers 2",
    ),
    (
        "create.lead",
        "code 0 handle set disposition 1",
        5,
        "a leading backslash: Wine answers 161, the crate makes the key",
    ),
    (
        "open.E-acute",
        "code 2 handle kept",
        6,
        "case past ASCII: Wine opens e-acute's key by E-acute, the crate answers 2",
    ),
    (
        "query.closed",
        "code 6 kind - room 4 buf a5a5a5a5",
        0,
        "a closed handle's kind: Wine writes 0, the crate writes none; its glue owns the field",
    ),
];

fn shown(value: u32) -> String {
    if value == S {
        "-".into()
    } else {
        format!("{value}")
    }
}

fn hex(bytes: [u8; 4]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The rows in the making, over one front.
struct Rows<'a, S> {
    front: &'a Front<S>,
    out: Vec<(String, String)>,
}

impl<T: Settings> Rows<'_, T> {
    fn push(&mut self, id: &str, answer: String) {
        self.out.push((id.into(), answer));
    }

    /// An open: the handle, or 0 for a refusal.
    fn open(&mut self, id: &str, within: u32, path: &[u8], options: u32) -> u32 {
        let answer = self.front.open_key(within, path, options, ACCESS);
        let (code, handle) = match answer {
            Ok(_) => (0, "set"),
            Err(code) => (code, "kept"),
        };
        self.push(id, format!("code {code} handle {handle}"));
        answer.unwrap_or(0)
    }

    /// A create: the handle, or 0 for a refusal.
    fn create(&mut self, id: &str, within: u32, path: &[u8], options: u32) -> u32 {
        match self.front.create_key(within, path, options, ACCESS) {
            Ok((handle, disposition)) => {
                self.push(
                    id,
                    format!("code 0 handle set disposition {}", shown(disposition)),
                );
                handle
            }
            Err(code) => {
                self.push(id, format!("code {code} handle kept disposition -"));
                0
            }
        }
    }

    fn set(&mut self, id: &str, key: u32, name: &[u8], kind: u32, data: &[u8]) {
        let code = self.front.set_value(key, name, kind, data);
        self.push(id, format!("code {code}"));
    }

    /// A query: `room` `None` stands for a null size pointer, `buffer`
    /// false for a null data pointer, `reserved` for a pointer where the
    /// call wants null (any pointer not 0; it stands as 1).
    fn query(
        &mut self,
        id: &str,
        key: u32,
        name: &[u8],
        reserved: bool,
        room: Option<u32>,
        buffer: bool,
    ) {
        let mut buf = [0xa5_u8; 4];
        let queried = self.front.query_value(
            key,
            name,
            u32::from(reserved),
            room,
            buffer.then_some(&mut buf[..]),
        );
        let (kind, written) = queried.wrote.unwrap_or((S, room.unwrap_or(S)));
        let room = if room.is_some() {
            shown(written)
        } else {
            "-".into()
        };
        self.push(
            id,
            format!(
                "code {} kind {} room {room} buf {}",
                queried.code,
                shown(kind),
                hex(buf)
            ),
        );
    }

    fn close(&mut self, id: &str, key: u32) {
        let code = self.front.close_key(key);
        self.push(id, format!("code {code}"));
    }

    /// A key let go without a row, as the judge's `let_go`.
    fn let_go(&self, key: u32) {
        if key != 0 {
            self.front.close_key(key);
        }
    }
}

/// Every row, in the judge's order, each with its answer. The judge's
/// cleans have no row: a store made for the run holds nothing after it.
pub(crate) fn rows<T: Settings>(front: &Front<T>) -> Vec<(String, String)> {
    let mut r = Rows {
        front,
        out: Vec::new(),
    };
    let kb: &[u8] = b"Software\\rg";
    let under = |tail: &[u8]| [kb, b"\\", tail].concat();
    let seven = 7_u32.to_le_bytes();
    r.open("open.miss", HKCU, &[kb, b"-none"].concat(), 0);
    let key = r.create("create.new", HKCU, kb, 0);
    let again = r.create("create.again", HKCU, kb, 0);
    r.close("close.again", again);
    r.set("set", key, b"Volume", 4, &seven);
    r.query("query.room4", key, b"Volume", false, Some(4), true);
    r.query("query.room2", key, b"Volume", false, Some(2), true);
    r.query("query.room0", key, b"Volume", false, Some(0), true);
    r.query("query.size-only", key, b"Volume", false, Some(0), false);
    r.query("query.no-size", key, b"Volume", false, None, true);
    r.query(
        "query.no-size-no-buffer",
        key,
        b"Volume",
        false,
        None,
        false,
    );
    r.query("query.reserved", key, b"Volume", true, Some(4), true);
    r.query("query.miss", key, b"none", false, Some(4), true);
    r.query("query.default-before", key, b"", false, Some(4), true);
    r.set("set.default", key, b"", 4, &9_u32.to_le_bytes());
    r.query("query.default", key, b"", false, Some(4), true);
    r.set("set.kind3", key, b"Text", 3, b"abcdef");
    r.set("set.dword-size2", key, b"Short", 4, &[1, 2]);
    r.set("set.tab", key, b"a\tb", 4, &seven);
    r.query("query.tab", key, b"a\tb", false, Some(4), true);
    r.set("set.case", key, b"VOLUME", 4, &8_u32.to_le_bytes());
    r.query("query.case", key, b"volume", false, Some(4), true);
    let held = r.open("open.options1", HKCU, kb, 1);
    r.let_go(held);
    let held = r.open("open.options8", HKCU, kb, 8);
    r.let_go(held);
    let held = r.create("create.options1", HKCU, &under(b"Vol"), 1);
    r.let_go(held);
    let held = r.create("create.options8", HKCU, &under(b"Link"), 8);
    r.let_go(held);
    r.set("set.root", HKCU, b"rg", 4, &seven);
    r.query("query.root", HKCU, b"rg", false, Some(4), true);
    let sub = r.create("create.sub", HKCU, &under(b"Sub"), 0);
    r.close("close.sub", sub);
    for (id, within, path) in [
        ("open.lead", HKCU, [b"\\", kb, b"\\Sub"].concat()),
        ("open.trail", HKCU, [kb, b"\\Sub\\"].concat()),
        ("open.double", HKCU, [kb, b"\\\\Sub"].concat()),
        ("open.empty", key, Vec::new()),
        ("open.under-key", key, b"Sub".to_vec()),
        ("open.ascii-case", HKCU, under(b"SUB")),
    ] {
        let held = r.open(id, within, &path, 0);
        r.let_go(held);
    }
    let held = r.create("create.lead", HKCU, &[b"\\", kb, b"\\Lead"].concat(), 0);
    r.let_go(held);
    let held = r.create("create.e-acute", HKCU, &under(b"\xe9"), 0);
    r.let_go(held);
    let held = r.open("open.E-acute", HKCU, &under(b"\xc9"), 0);
    r.let_go(held);
    r.close("close", key);
    r.close("close.twice", key);
    r.query("query.closed", key, b"Volume", false, Some(4), true);
    for n in 0..7 {
        r.close(&format!("close.root{n}"), 0x8000_0000 + n);
    }
    r.out
}

/// The side this build answers for.
pub(crate) fn side() -> &'static str {
    if cfg!(target_family = "wasm") {
        "P"
    } else if cfg!(windows) {
        "W"
    } else {
        "N"
    }
}

/// Each row's line, `CHECK RG1 <side> <row> = PASS|FAIL <answer>`, and
/// the count failed. A row passes on the judge's answer, or on its
/// standing difference's own.
pub(crate) fn score(rows: &[(String, String)]) -> (Vec<String>, usize) {
    let side = side();
    let mut lines = Vec::new();
    let mut failed = 0;
    if rows.len() != JUDGE.len() {
        lines.push(format!(
            "CHECK RG1 {side} count = FAIL {} rows, {} owed",
            rows.len(),
            JUDGE.len()
        ));
        failed += 1;
    }
    for (index, (id, wine)) in JUDGE.iter().enumerate() {
        let standing = STANDING.iter().find(|row| row.0 == *id);
        let owed = standing.map_or(*wine, |row| row.1);
        let line = match rows.get(index) {
            Some((got, answer)) if got == id && answer == owed => match standing {
                Some(row) => format!(
                    "CHECK RG1 {side} {id} = PASS {answer} (standing {}, Wine: {wine})",
                    row.2
                ),
                None => format!("CHECK RG1 {side} {id} = PASS {answer}"),
            },
            Some((got, answer)) if got == id => {
                format!("CHECK RG1 {side} {id} = FAIL {answer} (owed: {owed})")
            }
            Some((got, _)) => format!("CHECK RG1 {side} {id} = FAIL row {got} in its place"),
            None => format!("CHECK RG1 {side} {id} = FAIL missing"),
        };
        if line.contains(" = FAIL ") {
            failed += 1;
        }
        lines.push(line);
    }
    (lines, failed)
}

/// The page's save, which RG1 never reads back: the text is let go.
#[cfg(test)]
fn forget(_: &[u8]) {}

#[cfg(test)]
#[test]
fn each_row_answers_as_wine_or_its_standing_difference() {
    let page =
        crate::page::Page::new(&ROW, b"", forget).expect("an empty text is in the settings form");
    let front = Front::new(&ROW, page);
    let (lines, failed) = score(&rows(&front));
    for line in &lines {
        std::println!("{line}");
    }
    assert_eq!(failed, 0, "{failed} rows of RG1 failed");
}
