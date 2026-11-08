// SPDX-License-Identifier: MIT
//! The logbook fence: every log call in a workspace's sources against the
//! `logbook.json` at its root. It finds an uncataloged event, a message that
//! is not a bare event id, an orphan entry or field (declared, never logged),
//! a level that differs from the entry's, an undeclared field, or a template
//! naming a field the entry does not declare.
//!
//! It also reads the page folders in [`PAGES_ON_IDS`], whose scripts log the same
//! events by `logger.info("id", { key: value })`, under the same rules.
//!
//! One walk, so that every workspace holding a logbook calls the same one: a
//! workspace's test is one line, `check(root).enforce()`. A workspace that takes
//! these crates and shares log rows with them calls
//! `check_sharing(root, &[names]).enforce()` instead.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The logbook of this crate's own workspace: what a shared row must equal.
/// A file outside the crate, so `cargo package` would refuse it; this crate
/// does not publish.
const HOME_BOOK: &str = include_str!("../../../logbook.json");

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

/// The page folders the walk reads. An older page logs sentences and takes no
/// row, so it stays out of this list until it moves to event ids.
const PAGES_ON_IDS: [&str; 3] = [
    "crates/layer-check/page",
    "crates/sound-mix/page",
    "crates/web-shell/page",
];

const PAGE_CALLS: [(&str, char); 4] = [
    ("logger.error(", 'E'),
    ("logger.warn(", 'W'),
    ("logger.info(", 'I'),
    ("logger.debug(", 'D'),
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

/// `source` with each whole-line `//` comment blanked, so line numbers still count.
fn without_comment_lines(source: &str) -> String {
    source
        .lines()
        .map(|line| {
            if line.trim_start().starts_with("//") {
                ""
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The event id a quoted `message` holds: two or more dotted parts of lowercase
/// letters. `None` for anything else, a sentence included.
fn bare_event(message: &str) -> Option<&str> {
    message
        .strip_prefix('"')
        .and_then(|m| m.strip_suffix('"'))
        .filter(|e| {
            e.split('.').count() >= 2
                && e.split('.')
                    .all(|part| !part.is_empty() && part.chars().all(|ch| ch.is_ascii_lowercase()))
        })
}

/// The field names in a comma list of `name`, `name = value` or `name: value`.
fn key_names(text: &str) -> Vec<String> {
    split_top(text, ',')
        .into_iter()
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(|item| {
            item.split([':', '=', ' '])
                .next()
                .unwrap_or(item)
                .to_owned()
        })
        .collect()
}

/// The field names of a page call's arguments after its id: none, or one object.
/// `None` when the values are anything else, as a row could not name them.
fn page_keys(values: &[&str]) -> Option<Vec<String>> {
    let given: Vec<&str> = values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .collect();
    match given.as_slice() {
        [] => Some(Vec::new()),
        [object] => {
            let fields = object.strip_prefix('{')?.strip_suffix('}')?;
            Some(key_names(fields))
        }
        _ => None,
    }
}

/// The scripts of every folder in [`PAGES_ON_IDS`] that `root` holds. A workspace
/// without the folder has no such page; this workspace's own fence would call a
/// moved folder's rows orphans.
fn page_scripts(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for page in PAGES_ON_IDS {
        let Ok(entries) = std::fs::read_dir(root.join(page)) else {
            continue;
        };
        found.extend(
            entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| path.extension().is_some_and(|ext| ext == "mjs")),
        );
    }
    found.sort();
    found
}

/// Reads one page script's log calls into `sites`.
fn scan_page(root: &Path, path: &Path, sites: &mut Vec<Site>, problems: &mut Vec<String>) {
    let source = std::fs::read_to_string(path).expect("invariant: a page script is UTF-8");
    let text = without_comment_lines(&source);
    let name = path
        .strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string();
    for (pattern, level) in PAGE_CALLS {
        for (at, _) in text.match_indices(pattern) {
            let place = format!("{name}:{}", text[..at].lines().count());
            let Some(args) = inner(&text, at + pattern.len()) else {
                problems.push(format!("{place}: an unclosed log call"));
                continue;
            };
            let parts = split_top(args, ',');
            let message = parts[0].trim();
            let Some(event) = bare_event(message) else {
                problems.push(format!(
                    "{place}: the message is not a bare event id: {message}"
                ));
                continue;
            };
            let Some(keys) = page_keys(&parts[1..]) else {
                problems.push(format!(
                    "{place}: a page's values go in one object, each by its field's name"
                ));
                continue;
            };
            sites.push(Site {
                place,
                level,
                event: event.to_owned(),
                keys,
            });
        }
    }
}

fn scan(root: &Path, path: &Path, sites: &mut Vec<Site>, problems: &mut Vec<String>) {
    let source = std::fs::read_to_string(path).expect("invariant: a source file is UTF-8");
    let text = without_comment_lines(&source);
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
            let Some(event) = bare_event(message) else {
                problems.push(format!(
                    "{place}: the message is not a bare event id: {message}"
                ));
                continue;
            };
            sites.push(Site {
                place,
                level,
                event: event.to_owned(),
                keys: key_names(keys),
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
    /// The rows the caller named as shared, which skip the two orphan rules.
    pub shared: usize,
    /// Each rule broken, one line each; empty when the workspace holds.
    pub problems: Vec<String>,
}

impl Report {
    /// Prints the counts and panics on any problem or on a walk that found no
    /// log call, which would mean the walk had looked in the wrong place.
    pub fn enforce(&self) {
        eprintln!(
            "fence: {} files, {} log calls, {} events, {} shared",
            self.files, self.sites, self.events, self.shared
        );
        assert!(self.sites > 0, "the logbook fence found no log call");
        assert!(
            self.problems.is_empty(),
            "the logbook fence:\n{}",
            self.problems.join("\n")
        );
    }
}

/// The row `id` of a parsed logbook.
fn row<'book>(book: &'book serde_json::Value, id: &str) -> Option<&'book serde_json::Value> {
    book["events"]
        .as_array()?
        .iter()
        .find(|entry| entry["id"] == id)
}

/// Each named row must be in both logbooks; and every row both hold, named or
/// not, must be equal between them, as JSON values: key order and white space do
/// not count. A row both hold is one meaning read from two places, so a drift in
/// either is a problem whichever workspace walks it.
fn shared_problems(book: &serde_json::Value, shared: &[&str], problems: &mut Vec<String>) {
    let home: serde_json::Value =
        serde_json::from_str(HOME_BOOK).expect("invariant: this crate's logbook is JSON");
    for &id in shared {
        match (row(book, id), row(&home, id)) {
            (None, _) => problems.push(format!(
                "logbook {id}: named as shared, absent from this workspace's logbook"
            )),
            (_, None) => problems.push(format!(
                "logbook {id}: named as shared, absent from the dependency's logbook"
            )),
            _ => {}
        }
    }
    for here in book["events"].as_array().into_iter().flatten() {
        let id = here["id"].as_str().unwrap_or_default();
        if row(&home, id).is_some_and(|there| there != here) {
            problems.push(format!(
                "logbook {id}: held by both logbooks, unequal to the dependency's row"
            ));
        }
    }
}

/// Walks `root`'s `crates/` and its page folders against `root`'s `logbook.json`.
#[must_use]
pub fn check(root: &Path) -> Report {
    check_sharing(root, &[])
}

/// [`check`] for a workspace logging the rows in `shared` partly through crates
/// it takes from here, whose files its own walk cannot see.
///
/// For those rows alone the orphan-entry and logged-nowhere-field rules are off;
/// every other rule stays on. The exemption is earned: a named row absent from
/// either logbook is a problem, and so is any row both hold, named or not, that is
/// unequal between them, so the fence of this crate's own workspace, which keeps
/// every rule on that same row, answers for its fields. A field only the caller
/// logs therefore has no place in a shared row: this crate's own fence would fail
/// it.
#[must_use]
pub fn check_sharing(root: &Path, shared: &[&str]) -> Report {
    let book: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("logbook.json"))
            .expect("invariant: logbook.json is at the workspace root"),
    )
    .expect("invariant: logbook.json is JSON");
    let mut problems = Vec::new();
    shared_problems(&book, shared, &mut problems);
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
    let pages = page_scripts(root);
    for page in &pages {
        scan_page(root, page, &mut sites, &mut problems);
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
        if shared.contains(&id.as_str()) {
            continue;
        }
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
        files: files.len() + pages.len(),
        sites: sites.len(),
        events: entries.len(),
        shared: shared.len(),
        problems,
    }
}
