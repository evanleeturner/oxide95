// SPDX-License-Identifier: MIT
//! The walk the host tests and the page's export both run: the fronts over the memory folder,
//! answering in the same words a target's own file calls answer in. It asks and judges by
//! `want.rs`, which is written from the target's answers, never from this crate's.

use super::want::{FILES_WANT, FINDS_WANT, KNOWN, MATCHES_WANT};
use super::{Calls, Errors, FIND_BYTES, Files, Folder, MemoryFolder, Row, names};
use std::ffi::{CStr, CString};

/// The target's numbers, each as its own calls printed or passed them.
pub const ROW: Row = Row {
    file: "KERNEL32.DLL",
    calls: Calls {
        open: "CreateFileA",
        read: "ReadFile",
        write: "WriteFile",
        seek: "SetFilePointer",
        kind: "GetFileType",
        close: "CloseHandle",
        delete: "DeleteFileA",
        rename: "MoveFileA",
        find_first: "FindFirstFileA",
        find_next: "FindNextFileA",
        find_close: "FindClose",
    },
    read: 0x8000_0000,
    write: 0x4000_0000,
    share: 1,
    plain: 0x80,
    creates: [1, 2, 3, 4, 5],
    methods: [0, 1, 2],
    kinds: [1, 2, 3],
    unknown: 0,
    invalid: 0xffff_ffff,
    errors: Errors {
        not_found: 2,
        no_path: 3,
        denied: 5,
        bad_handle: 6,
        no_more: 18,
        exists: 80,
        busy: 32,
        taken: 183,
        bad_seek: 131,
        bad_argument: 87,
    },
    name_at: 44,
    name_bytes: 260,
};

const READ: u32 = ROW.read;
const WRITE: u32 = ROW.write;
const BAD: u32 = ROW.invalid;
const CREATE_NEW: u32 = ROW.creates[0];
const CREATE_ALWAYS: u32 = ROW.creates[1];
const OPEN_EXISTING: u32 = ROW.creates[2];
const NO_HANDLE: u32 = 0x1234;

/// The names the find table runs over, beside one folder, `sub`, holding `inner.txt`.
const NAMES: [&str; 6] = ["k1.txt", "abc", "ab", "x", "data.tar.gz", "MIXED.TXT"];
const PATTERNS: [&str; 34] = [
    "k1.txt",
    "K1.TXT",
    "mixed.txt",
    "MIXED.TXT",
    "abc",
    "ABC",
    "data.tar.gz",
    "DATA.TAR.GZ",
    "*",
    "*.*",
    "*.",
    "?",
    "??",
    "k1.txt?",
    "abc?",
    "x?",
    "abc.",
    "*.TXT",
    "*.txt",
    "a*c",
    "*a*",
    "data.*",
    "*.gz",
    "z*.none",
    "K?.TXT",
    "sub",
    "sub\\*",
    "sub\\*.*",
    "sub\\inner.txt",
    "sub\\INNER.TXT",
    "sub/inner.txt",
    "sub\\*.none",
    "nosuch\\*",
    "nosuch\\x.txt",
];

type Walked = Files<MemoryFolder>;

/// The answer and the error it left, the error cleared first.
fn both<T>(files: &Walked, call: impl FnOnce(&Walked) -> T) -> (T, u32) {
    files.folder().fail(0);
    let value = call(files);
    (value, files.folder().last_error())
}

/// The 22 file answers, call for call in the target's order.
#[allow(
    clippy::too_many_lines,
    reason = "one straight walk, read against the target's own"
)]
pub fn file_lines() -> Vec<String> {
    let files = Files::new(&ROW, MemoryFolder::default());
    let open = |name: &CStr, access: u32, share: u32, disposition: u32| {
        both(&files, |front| {
            front.open(name, access, share, disposition, ROW.plain)
        })
    };
    let mut lines = Vec::new();
    let (file, error) = open(c"no-such.txt", READ, 0, OPEN_EXISTING);
    lines.push(format!(
        "files CreateFileA miss = bad {} error {error}",
        file == BAD
    ));
    let (file, error) = open(c"no-such-folder\\x.txt", READ, 0, OPEN_EXISTING);
    lines.push(format!(
        "files CreateFileA no folder = bad {} error {error}",
        file == BAD
    ));
    let (file, error) = open(c"k1.txt", READ | WRITE, 1, CREATE_ALWAYS);
    lines.push(format!(
        "files CreateFileA create = bad {} error {error}",
        file == BAD
    ));
    let (again, error) = open(c"k1.txt", READ, 1 | 2, CREATE_NEW);
    lines.push(format!(
        "files CreateFileA CREATE_NEW on one = bad {} error {error}",
        again == BAD
    ));
    let mut wrote = 0u32;
    let ok = files.write(file, b"hello", Some(&mut wrote));
    lines.push(format!("files WriteFile 5 = ok {ok} wrote {wrote}"));
    lines.push(format!("files GetFileType = {}", files.kind(file)));
    let (at, error) = both(&files, |front| front.seek(file, 100, 0));
    lines.push(format!(
        "files SetFilePointer 100 past the end = {at} error {error}"
    ));
    let mut buffer = [0u8; 8];
    let mut read = 99u32;
    let ok = files.read(file, &mut buffer, Some(&mut read));
    lines.push(format!("files ReadFile past the end = ok {ok} read {read}"));
    let (at, error) = both(&files, |front| front.seek(file, -1, 0));
    lines.push(format!(
        "files SetFilePointer before the start = {at:#x} error {error}"
    ));
    let (at, error) = both(&files, |front| front.seek(file, 0, 9));
    lines.push(format!(
        "files SetFilePointer bad method = {at:#x} error {error}"
    ));
    let (ok, error) = both(&files, |front| front.delete(c"k1.txt"));
    lines.push(format!(
        "files DeleteFileA while open = ok {ok} error {error}"
    ));
    files.close(file);
    let (reader, _) = open(c"k1.txt", READ, 1, OPEN_EXISTING);
    let (ok, error) = both(&files, |front| front.write(reader, b"x", Some(&mut wrote)));
    lines.push(format!(
        "files WriteFile on a read handle = ok {ok} error {error}"
    ));
    files.close(reader);
    let (writer, _) = open(c"k2.txt", WRITE, 0, CREATE_ALWAYS);
    let (ok, error) = both(&files, |front| {
        front.read(writer, &mut buffer[..1], Some(&mut read))
    });
    lines.push(format!(
        "files ReadFile on a write handle = ok {ok} error {error}"
    ));
    let (busy, error) = open(c"k2.txt", READ, 3, OPEN_EXISTING);
    lines.push(format!(
        "files CreateFileA on one held share 0 = bad {} error {error}",
        busy == BAD
    ));
    files.close(writer);
    let (ok, error) = both(&files, |front| front.rename(c"k1.txt", c"k2.txt"));
    lines.push(format!("files MoveFileA onto one = ok {ok} error {error}"));
    let (ok, error) = both(&files, |front| front.rename(c"none.txt", c"k3.txt"));
    lines.push(format!("files MoveFileA miss = ok {ok} error {error}"));
    let mut data = [0u8; FIND_BYTES];
    let (find, error) = both(&files, |front| front.find_first(c"z*.none", &mut data));
    lines.push(format!(
        "files FindFirstFileA miss = bad {} error {error}",
        find == BAD
    ));
    let (find, _) = both(&files, |front| front.find_first(c"K?.TXT", &mut data));
    let mut count = i32::from(find != BAD);
    loop {
        let (ok, error) = both(&files, |front| front.find_next(find, &mut data));
        if ok == 0 {
            lines.push(format!(
                "files FindNextFileA end after {count} = error {error}"
            ));
            break;
        }
        count += 1;
    }
    lines.push(format!("files FindClose = {}", files.find_close(find)));
    let (ok, error) = both(&files, |front| front.close(NO_HANDLE));
    lines.push(format!("files CloseHandle bad = ok {ok} error {error}"));
    let (ok, error) = both(&files, |front| {
        front.read(NO_HANDLE, &mut buffer[..1], Some(&mut read))
    });
    lines.push(format!("files ReadFile bad handle = ok {ok} error {error}"));
    let (ok, error) = both(&files, |front| front.delete(c"none.txt"));
    lines.push(format!("files DeleteFileA miss = ok {ok} error {error}"));
    lines
}

/// One file made through the fronts, as the target's table is made.
fn make(files: &Walked, name: &str) {
    let text = CString::new(name).expect("a name holds no NUL");
    let file = files.open(&text, ROW.write, 0, CREATE_ALWAYS, ROW.plain);
    let mut wrote = 0u32;
    files.write(file, b"hello", Some(&mut wrote));
    files.close(file);
}

/// The table's folder: `NAMES` and `sub\inner.txt`, each holding `hello`.
fn table() -> Walked {
    let files = Files::new(&ROW, MemoryFolder::default().give_folder("sub"));
    for name in NAMES {
        make(&files, name);
    }
    make(&files, "sub\\inner.txt");
    files
}

/// The name a find left in its data, to its NUL.
fn found_name(data: &[u8]) -> String {
    let name = &data[ROW.name_at..ROW.name_at + ROW.name_bytes];
    let end = name
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(name.len());
    String::from_utf8_lossy(&name[..end]).into_owned()
}

/// One pattern's find: the names as answered, and its two errors; `None` when it found nothing.
fn find(files: &Walked, pattern: &str) -> (Option<Vec<String>>, u32, u32) {
    let text = CString::new(pattern).expect("a pattern holds no NUL");
    let mut data = [0u8; FIND_BYTES];
    let (first, error) = both(files, |front| front.find_first(&text, &mut data));
    if first == BAD {
        return (None, error, 0);
    }
    let mut order = vec![found_name(&data)];
    let end = loop {
        let (ok, end) = both(files, |front| front.find_next(first, &mut data));
        if ok == 0 {
            break end;
        }
        order.push(found_name(&data));
    };
    files.find_close(first);
    (Some(order), error, end)
}

/// The read back, then each pattern's set and errors, in the target's words.
pub fn find_lines() -> Vec<String> {
    let files = table();
    let reader = files.open(c"k1.txt", READ, 1, OPEN_EXISTING, ROW.plain);
    let mut buffer = [0u8; 16];
    let mut read = 0u32;
    let ok = files.read(reader, &mut buffer, Some(&mut read));
    let bytes = String::from_utf8_lossy(&buffer[..read as usize]).into_owned();
    files.close(reader);
    let mut lines = vec![format!("back k1.txt = ok {ok} read {read} {bytes}")];
    for pattern in PATTERNS {
        lines.push(match find(&files, pattern) {
            (None, error, _) => format!("find {pattern} = bad true error {error}"),
            (Some(mut set), error, end) => {
                set.sort();
                format!(
                    "find {pattern} = set {} error {error} end {end}",
                    set.join(" ")
                )
            }
        });
    }
    lines
}

/// `names::matches` itself, each pattern with no folder in it over the table's names.
pub fn match_lines() -> Vec<String> {
    PATTERNS
        .iter()
        .filter(|pattern| !pattern.contains(['\\', '/']))
        .map(|pattern| {
            let mut set: Vec<&str> = NAMES
                .into_iter()
                .filter(|name| names::matches(pattern.as_bytes(), name.as_bytes()))
                .collect();
            set.sort_unstable();
            let set = if set.is_empty() {
                "none".to_string()
            } else {
                set.join(" ")
            };
            format!("match {pattern} = {set}")
        })
        .collect()
}

/// Each pattern's names in the order a find answered them.
pub fn order_lines() -> Vec<String> {
    let files = table();
    PATTERNS
        .iter()
        .map(|pattern| {
            let order = find(&files, pattern)
                .0
                .map_or("none".to_string(), |names| names.join(" "));
            format!("order {pattern} = {order}")
        })
        .collect()
}

/// A file's bytes in the folder, past the fronts.
pub fn contents_line() -> String {
    let bytes = table().folder().contents("k1.txt").unwrap_or_default();
    format!("contents k1.txt = {}", String::from_utf8_lossy(&bytes))
}

/// Every field of the find data, one line, named once and never a row each.
pub fn fields_line() -> String {
    let files = table();
    let mut data = [0u8; FIND_BYTES];
    let find = files.find_first(c"k1.txt", &mut data);
    files.find_close(find);
    let word = |at: usize| u32::from_le_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]);
    let time = |at: usize| {
        if data[at..at + 8].iter().all(|&byte| byte == 0) {
            "zero"
        } else {
            "set"
        }
    };
    let alternate = data[304..318].iter().take_while(|&&byte| byte != 0).count();
    format!(
        "fields k1.txt = attributes {:#x} created {} accessed {} written {} size-high {} size-low {} reserved0 {:#x} reserved1 {:#x} alternate-bytes {alternate} tail {} {}",
        word(0),
        time(4),
        time(12),
        time(20),
        word(28),
        word(32),
        word(36),
        word(40),
        data[318],
        data[319]
    )
}

/// Every row the host tests and the page print.
pub fn rows() -> Vec<String> {
    let mut rows = file_lines();
    rows.extend(find_lines());
    rows.extend(match_lines());
    rows.extend(order_lines());
    rows.push(contents_line());
    rows.push(fields_line());
    rows
}

/// A row's words before its answer: the name `KNOWN` holds it by.
pub fn key(line: &str) -> &str {
    line.split(" = ").next().unwrap_or(line)
}

/// Why a row is told apart, if it is.
pub fn reason(line: &str) -> Option<&'static str> {
    KNOWN
        .iter()
        .find(|(row, _)| *row == key(line))
        .map(|(_, reason)| *reason)
}

/// The rows that differ from their want and hold no reason, a count mismatch among them.
pub fn wrong(got: &[String], want: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    if got.len() != want.len() {
        out.push(format!(
            "{} rows where the table holds {}",
            got.len(),
            want.len()
        ));
    }
    for (got, want) in got.iter().zip(want) {
        if got != want && reason(want).is_none() {
            out.push(format!("got `{got}` want `{want}`"));
        }
    }
    out
}

/// The order rows whose names are not in the crate's promised key order.
pub fn out_of_order(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter(|line| {
            let names: Vec<&str> = line
                .split(" = ")
                .nth(1)
                .unwrap_or("")
                .split(' ')
                .filter(|name| *name != "none")
                .collect();
            let folded = |name: &str| name.bytes().map(names::fold).collect::<Vec<u8>>();
            !names.windows(2).all(|two| folded(two[0]) <= folded(two[1]))
        })
        .cloned()
        .collect()
}

/// All that the page's exit stands on: each table's wrong rows, and the order's.
#[allow(dead_code, reason = "the page's export alone reads this")]
pub fn all_wrong() -> Vec<String> {
    let mut out = wrong(&file_lines(), &FILES_WANT);
    out.extend(wrong(&find_lines(), &FINDS_WANT));
    out.extend(wrong(&match_lines(), &MATCHES_WANT));
    out.extend(out_of_order(&order_lines()));
    if contents_line() != "contents k1.txt = hello" {
        out.push(contents_line());
    }
    out
}
