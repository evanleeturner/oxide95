// SPDX-License-Identifier: MIT
//! No tracked file holds a program's address: the rule over every text file
//! `git ls-files` lists, so a file counts from the moment it is staged. The
//! test asks git, so it fails, and says so, outside a git tree.

use std::fs;
use std::path::Path;
use std::process::Command;
use std::str;

/// This file: its own table and its own cases would trip the rule.
const THIS_FILE: &str = "crates/oxide-logline/tests/no_program_address.rs";

/// The runs the rule takes that are no program's address: the exact path, the
/// exact value, the lines holding it, and what the value is. Anything else is
/// a failure, the same value on a new path or a new line included.
const KNOWN: [(&str, &str, usize, &str); 6] = [
    (
        "crates/kernel-time/src/lib.rs",
        "10000000",
        1,
        "file-time ticks a second",
    ),
    (
        "crates/kernel-time/src/tests.rs",
        "10000000",
        2,
        "file-time ticks a second",
    ),
    (
        "crates/kernel-time/src/host/tests.rs",
        "10000000",
        1,
        "file-time ticks a second",
    ),
    ("crates/window/src/front.rs", "10000000", 1, "WS_VISIBLE"),
    ("crates/window/src/front.rs", "00400000", 1, "WS_DLGFRAME"),
    (
        "crates/window/src/tests.rs",
        "00401000",
        1,
        "a test's procedure",
    ),
];

/// A program's address: a run of 8 or more hex digits starting `004` or `10`,
/// its `_` separators dropped.
fn addresses(line: &str) -> Vec<String> {
    let line = line.to_ascii_lowercase().replace('_', "");
    line.split(|c: char| !c.is_ascii_hexdigit())
        .filter(|run| run.len() >= 8 && (run.starts_with("004") || run.starts_with("10")))
        .map(str::to_owned)
        .collect()
}

/// Every tracked path, relative to the workspace's root.
fn tracked(root: &Path) -> Vec<String> {
    let listed = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "-z"])
        .output()
        .expect("git runs");
    assert!(listed.status.success(), "this test needs a git tree");
    str::from_utf8(&listed.stdout)
        .expect("tracked paths are text")
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(str::to_owned)
        .collect()
}

#[test]
fn the_rule_takes_an_address_and_leaves_a_small_number() {
    assert_eq!(
        addresses("call 0x10000001 at 0x0040_0001"),
        ["10000001", "00400001"]
    );
    assert!(addresses("0x0012_3456 and 0x1000 and 1000").is_empty());
}

#[test]
fn no_tracked_file_holds_a_programs_address() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (mut files, mut seen, mut hits) = (0_u32, [0_usize; KNOWN.len()], Vec::new());
    for path in tracked(&root) {
        // Not UTF-8 is not text; a path git lists and the disk lacks is a staged removal.
        let Ok(text) = fs::read_to_string(root.join(&path)) else {
            continue;
        };
        if path == THIS_FILE {
            continue;
        }
        files += 1;
        for (at, line) in (1_u32..).zip(text.lines()) {
            if path == "Cargo.lock" && line.starts_with("checksum = ") {
                continue;
            }
            for run in addresses(line) {
                let known = KNOWN.iter().position(|row| path == row.0 && run == row.1);
                match known {
                    Some(row) => seen[row] += 1,
                    None => hits.push(format!("{path}:{at}: {run}")),
                }
            }
        }
    }
    println!(
        "{files} tracked text files, {} known lines",
        seen.iter().sum::<usize>()
    );
    assert!(files > 0, "no tracked files: stage before testing");
    assert!(hits.is_empty(), "program addresses:\n{}", hits.join("\n"));
    for (row, count) in KNOWN.iter().zip(seen) {
        assert_eq!(count, row.2, "known run on a changed line count: {row:?}");
    }
}
