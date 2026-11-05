// SPDX-License-Identifier: MIT
//! The crate's two grep fences, read from its own `src/` and `Cargo.toml`:
//! no back loads a host library (the name table is the stand-in's own),
//! and no line holds a game's address (a game's facts live in its rows
//! crate).
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

/// A host library load: `dlopen`, `libloading`, or a `LoadLibrary` call.
fn loads_a_library(line: &str) -> bool {
    const CALLS: [&str; 6] = ["(", "A(", "W(", "Ex(", "ExA(", "ExW("];
    line.contains("dlopen")
        || line.contains("libloading")
        || line.match_indices("LoadLibrary").any(|(at, name)| {
            CALLS
                .iter()
                .any(|call| line[at + name.len()..].starts_with(call))
        })
}

/// A game's address: a run of 8 or more hex digits starting `004` or `10`,
/// its `_` separators dropped.
fn names_a_game(line: &str) -> bool {
    let line = line.to_ascii_lowercase().replace('_', "");
    line.split(|c: char| !c.is_ascii_hexdigit())
        .any(|run| run.len() >= 8 && (run.starts_with("004") || run.starts_with("10")))
}

#[test]
fn no_back_loads_a_host_library() {
    let hits = hits(|line| !is_comment(line) && loads_a_library(line));
    assert!(hits.is_empty(), "host library loads:\n{}", hits.join("\n"));
}

#[test]
fn no_line_names_a_game() {
    let hits = hits(names_a_game);
    assert!(hits.is_empty(), "a game's names:\n{}", hits.join("\n"));
}
