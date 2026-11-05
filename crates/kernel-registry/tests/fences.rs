// SPDX-License-Identifier: MIT
//! The crate's two grep fences, read from its own `src/` and `Cargo.toml`:
//! no back calls the host's registry or OS (the
//! file back is `std` alone), and no line holds a game's address (a
//! game's facts live in its rows crate).
use std::fs;
use std::path::{Path, PathBuf};

/// Every file the fences read: `Cargo.toml` and each file under `src/`.
fn files() -> Vec<PathBuf> {
    fn walk(folder: &Path, found: &mut Vec<PathBuf>) {
        let mut entries: Vec<PathBuf> = fs::read_dir(folder)
            .expect("src/ reads")
            .map(|entry| entry.expect("an entry reads").path())
            .collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                walk(&path, found);
            } else {
                found.push(path);
            }
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut found = vec![root.join("Cargo.toml")];
    walk(&root.join("src"), &mut found);
    found
}

/// Each line of every file, with its place, for a fence to judge.
fn hits(judge: impl Fn(&str) -> bool) -> Vec<String> {
    let mut hits = Vec::new();
    for path in files() {
        let text = fs::read_to_string(&path).expect("a file reads");
        for (index, line) in text.lines().enumerate() {
            if judge(line) {
                hits.push(format!("{}:{}: {line}", path.display(), index + 1));
            }
        }
    }
    hits
}

/// A comment line, Rust's or TOML's: an attribute's `#[` and `#!` are code.
fn is_comment(line: &str) -> bool {
    let line = line.trim_start();
    line.starts_with("//")
        || (line.starts_with('#') && !line.starts_with("#[") && !line.starts_with("#!"))
}

/// A host call: a Windows or C binding crate, a foreign block, or a
/// registry call by its Win32 name.
fn calls_the_host(line: &str) -> bool {
    [
        "windows::",
        "windows_sys",
        "windows-sys",
        "winapi",
        "winreg",
        "libc",
        "extern \"",
        "#[link",
        "RegOpenKey",
        "RegQueryValue",
        "RegCloseKey",
        "RegCreateKey",
        "RegSetValue",
    ]
    .iter()
    .any(|name| line.contains(name))
}

/// A game's address: a run of 8 or more hex digits starting `004` or `10`,
/// its `_` separators dropped.
fn names_a_game(line: &str) -> bool {
    let line = line.to_ascii_lowercase().replace('_', "");
    line.split(|c: char| !c.is_ascii_hexdigit())
        .any(|run| run.len() >= 8 && (run.starts_with("004") || run.starts_with("10")))
}

#[test]
fn no_back_calls_the_host() {
    let hits = hits(|line| !is_comment(line) && calls_the_host(line));
    assert!(hits.is_empty(), "host calls:\n{}", hits.join("\n"));
}

#[test]
fn the_crate_depends_on_the_trace_and_log_alone() {
    let manifest = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"))
        .expect("Cargo.toml reads");
    let section = manifest
        .split("\n[dependencies]\n")
        .nth(1)
        .expect("a dependencies section")
        .split("\n[")
        .next()
        .unwrap_or_default();
    let names: Vec<&str> = section
        .lines()
        .filter(|line| !line.trim().is_empty() && !is_comment(line))
        .filter_map(|line| line.split_whitespace().next())
        .collect();
    assert_eq!(names, ["oxide_trace", "log"]);
}

#[test]
fn no_line_names_a_game() {
    let hits = hits(names_a_game);
    assert!(hits.is_empty(), "a game's names:\n{}", hits.join("\n"));
}
