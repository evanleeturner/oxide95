// SPDX-License-Identifier: MIT
//! The logbook fence: every log call in a workspace's sources against the
//! `logbook.json` at its root. It finds an uncataloged event, a message that
//! is not a bare event id, an orphan entry or field (declared, never logged),
//! a level that differs from the entry's, an undeclared field, or a template
//! naming a field the entry does not declare.
//!
//! One function, so that every workspace holding a logbook calls the same
//! walk: a workspace's test is one line, `check(root).enforce()`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// One log call found in a source file.
struct Site {
    place: String,
    level: char,
    event: String,
    keys: Vec<String>,
}

const MACROS: [(&str, char); 5] = [
    ("error!(", 'E'),
    ("warn!(", 'W'),
    ("info!(", 'I'),
    ("debug!(", 'D'),
    ("trace!(", 'T'),
];

fn sources(dir: &Path, found: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .expect("invariant: the crates directory is readable")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() && path.file_name().is_some_and(|name| name != "target") {
            sources(&path, found);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            found.push(path);
        }
    }
}

/// Splits `text` at each top-level `separator`, outside strings and brackets.
fn split_top(text: &str, separator: char) -> Vec<&str> {
    let (mut parts, mut depth, mut start, mut in_string, mut escaped) =
        (Vec::new(), 0, 0, false, false);
    for (index, ch) in text.char_indices() {
        if in_string {
            match ch {
                _ if escaped => escaped = false,
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            _ if ch == separator && depth == 0 => {
                parts.push(&text[start..index]);
                start = index + ch.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(&text[start..]);
    parts
}

/// The text between a macro's `(` at `open` and its matching `)`.
fn inner(text: &str, open: usize) -> Option<&str> {
    let rest = &text[open..];
    let (mut depth, mut in_string, mut escaped) = (1, false, false);
    for (index, ch) in rest.char_indices() {
        if in_string {
            match ch {
                _ if escaped => escaped = false,
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&rest[..index]);
                }
            }
            _ => {}
        }
    }
    None
}

fn scan(root: &Path, path: &Path, sites: &mut Vec<Site>, problems: &mut Vec<String>) {
    let source = std::fs::read_to_string(path).expect("invariant: a source file is UTF-8");
    let text: String = source
        .lines()
        .map(|line| {
            if line.trim_start().starts_with("//") {
                ""
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let name = path
        .strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string();
    if text.contains("log!(") {
        problems.push(format!("{name}: a `log!` call: use the level macros"));
    }
    for (pattern, level) in MACROS {
        for (at, _) in text.match_indices(pattern) {
            if text[..at].ends_with(|ch: char| ch.is_alphanumeric() || ch == '_') {
                continue;
            }
            let place = format!("{name}:{}", text[..at].lines().count());
            let Some(args) = inner(&text, at + pattern.len()) else {
                problems.push(format!("{place}: an unclosed log call"));
                continue;
            };
            let parts = split_top(args, ';');
            let (keys, message) = match parts.as_slice() {
                [message] => ("", *message),
                [keys, message] => (*keys, *message),
                _ => {
                    problems.push(format!("{place}: a log call of more than one `;`"));
                    continue;
                }
            };
            let message = message.trim().trim_end_matches(',').trim();
            let event = message.strip_prefix('"').and_then(|m| m.strip_suffix('"'));
            let Some(event) = event.filter(|e| {
                e.split('.').count() >= 2
                    && e.split('.').all(|part| {
                        !part.is_empty() && part.chars().all(|ch| ch.is_ascii_lowercase())
                    })
            }) else {
                problems.push(format!(
                    "{place}: the message is not a bare event id: {message}"
                ));
                continue;
            };
            let keys = split_top(keys, ',')
                .into_iter()
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(|item| {
                    item.split([':', '=', ' '])
                        .next()
                        .unwrap_or(item)
                        .to_owned()
                })
                .collect();
            sites.push(Site {
                place,
                level,
                event: event.to_owned(),
                keys,
            });
        }
    }
}

/// What one walk of a workspace found.
#[derive(Debug)]
pub struct Report {
    /// The source files read.
    pub files: usize,
    /// The log calls found in them.
    pub sites: usize,
    /// The events the logbook declares.
    pub events: usize,
    /// Each rule broken, one line each; empty when the workspace holds.
    pub problems: Vec<String>,
}

impl Report {
    /// Prints the counts and panics on any problem or on a walk that found no
    /// log call, which would mean the walk had looked in the wrong place.
    pub fn enforce(&self) {
        eprintln!(
            "fence: {} files, {} log calls, {} events",
            self.files, self.sites, self.events
        );
        assert!(self.sites > 0, "the logbook fence found no log call");
        assert!(
            self.problems.is_empty(),
            "the logbook fence:\n{}",
            self.problems.join("\n")
        );
    }
}

/// Walks `root`'s `crates/` against `root`'s `logbook.json`.
#[must_use]
pub fn check(root: &Path) -> Report {
    let book: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("logbook.json"))
            .expect("invariant: logbook.json is at the workspace root"),
    )
    .expect("invariant: logbook.json is JSON");
    let mut problems = Vec::new();
    let mut entries: BTreeMap<String, (char, BTreeSet<String>)> = BTreeMap::new();
    for entry in book["events"]
        .as_array()
        .expect("invariant: the logbook has an events list")
    {
        let id = entry["id"].as_str().unwrap_or_default().to_owned();
        let level = entry["level"]
            .as_str()
            .and_then(|l| l.chars().next())
            .unwrap_or('?');
        let fields: BTreeSet<String> = entry["fields"]
            .as_object()
            .map(|f| f.keys().cloned().collect())
            .unwrap_or_default();
        let template = entry["template"].as_str().unwrap_or_default();
        for named in template
            .split('{')
            .skip(1)
            .filter_map(|part| part.split('}').next())
        {
            if !fields.contains(named) {
                problems.push(format!(
                    "logbook {id}: the template names an undeclared field `{named}`"
                ));
            }
        }
        if entries.insert(id.clone(), (level, fields)).is_some() {
            problems.push(format!("logbook {id}: declared twice"));
        }
    }

    let mut files = Vec::new();
    sources(&root.join("crates"), &mut files);
    // This file names the log macros as data, not as calls.
    files.retain(|file| !file.ends_with("oxide-logline/src/fence.rs"));
    let mut sites = Vec::new();
    for file in &files {
        scan(root, file, &mut sites, &mut problems);
    }

    let mut used: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for site in &sites {
        let Some((level, fields)) = entries.get(&site.event) else {
            problems.push(format!(
                "{}: uncataloged event `{}`",
                site.place, site.event
            ));
            continue;
        };
        if *level != site.level {
            problems.push(format!(
                "{}: `{}` logged at {}, cataloged at {level}",
                site.place, site.event, site.level
            ));
        }
        let keys = used.entry(&site.event).or_default();
        for key in &site.keys {
            keys.insert(key);
            if !fields.contains(key) {
                problems.push(format!(
                    "{}: `{}` carries the undeclared field `{key}`",
                    site.place, site.event
                ));
            }
        }
    }
    for (id, (_, fields)) in &entries {
        match used.get(id.as_str()) {
            None => problems.push(format!("logbook {id}: an orphan entry, logged nowhere")),
            Some(keys) => {
                for field in fields.iter().filter(|field| !keys.contains(field.as_str())) {
                    problems.push(format!(
                        "logbook {id}: the field `{field}` is logged nowhere"
                    ));
                }
            }
        }
    }
    Report {
        files: files.len(),
        sites: sites.len(),
        events: entries.len(),
        problems,
    }
}
