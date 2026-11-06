// SPDX-License-Identifier: MIT
//! This workspace's logbook against its own log calls; and `check_sharing`'s
//! rules, each on a throwaway workspace of one logbook and one source file.
//!
//! The real walk reads this file too, so a planted log call is built at run time
//! from its level's name: spelled out here, it would be a call of this workspace.

use std::fs;
use std::path::{Path, PathBuf};

use oxide_logline::fence::{Report, check, check_sharing};
use serde_json::{Value, json};

fn home() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A row of this workspace's own logbook.
fn home_row(id: &str) -> Value {
    let book = fs::read_to_string(home().join("logbook.json")).expect("the logbook is readable");
    let book: Value = serde_json::from_str(&book).expect("the logbook is JSON");
    book["events"]
        .as_array()
        .expect("the logbook has an events list")
        .iter()
        .find(|row| row["id"] == id)
        .expect("the row is in the logbook")
        .clone()
}

/// A source file logging event `id` at `level`, carrying `keys`.
fn logging(level: &str, keys: &str, id: &str) -> String {
    format!("fn f() {{ {level}!({keys}; \"{id}\"); }}\n")
}

/// Walks a throwaway workspace holding `rows` and `source`, printing what it found.
fn walk(name: &str, rows: &[Value], source: &str, shared: &[&str]) -> Report {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("fence-{name}"));
    fs::create_dir_all(root.join("crates/here/src")).expect("the temporary tree is writable");
    fs::write(
        root.join("logbook.json"),
        json!({ "events": rows }).to_string(),
    )
    .expect("the temporary logbook is writable");
    fs::write(root.join("crates/here/src/lib.rs"), source)
        .expect("the temporary source is writable");
    let report = check_sharing(&root, shared);
    eprintln!("{name}: {:?}", report.problems);
    report
}

/// Fails unless the walk found exactly one problem, and it holds `words`.
fn fails_once(report: &Report, words: &str) {
    assert_eq!(report.problems.len(), 1, "{:?}", report.problems);
    assert!(report.problems[0].contains(words), "{:?}", report.problems);
}

#[test]
fn every_log_call_is_cataloged_and_every_entry_is_used() {
    check(&home()).enforce();
}

#[test]
fn a_shared_row_equal_to_the_home_row_may_go_unlogged() {
    let rows = [home_row("fn.in")];
    let report = walk("shared-unlogged", &rows, "", &["fn.in"]);
    assert!(report.problems.is_empty(), "{:?}", report.problems);
    assert_eq!(report.shared, 1);
}

#[test]
fn the_same_row_not_named_as_shared_is_an_orphan() {
    let rows = [home_row("fn.in")];
    let report = walk("not-named", &rows, "", &[]);
    fails_once(&report, "an orphan entry");
}

#[test]
fn a_named_row_absent_from_this_workspace_fails() {
    let report = walk("absent-here", &[], "", &["fn.in"]);
    fails_once(&report, "absent from this workspace's logbook");
}

#[test]
fn a_named_row_absent_from_the_dependency_fails() {
    let row = json!({ "id": "only.here", "level": "DEBUG", "fields": {}, "template": "" });
    let report = walk("absent-there", &[row], "", &["only.here"]);
    fails_once(&report, "absent from the dependency's logbook");
}

#[test]
fn a_named_row_unequal_to_the_dependency_fails() {
    let mut row = home_row("fn.in");
    row["fields"]["added"] = json!("a field the dependency's row does not hold");
    let report = walk("unequal", &[row], "", &["fn.in"]);
    fails_once(&report, "unequal to the dependency's row");
}

/// The drift this closes: nobody named this row, and it is logged, so no other
/// rule sees it differ from the row a dependency logs through.
#[test]
fn a_row_both_hold_unnamed_and_unequal_fails() {
    let mut row = home_row("fn.in");
    row["fields"]["added"] = json!("a field the dependency's row does not hold");
    let source = logging("debug", "f = 1, a = 2, added = 3", "fn.in");
    let report = walk("drift", &[row], &source, &[]);
    fails_once(
        &report,
        "held by both logbooks, unequal to the dependency's row",
    );
}

#[test]
fn an_undeclared_field_on_a_shared_row_fails() {
    let source = logging("debug", "undeclared = 1", "fn.in");
    let report = walk(
        "undeclared-field",
        &[home_row("fn.in")],
        &source,
        &["fn.in"],
    );
    fails_once(&report, "carries the undeclared field `undeclared`");
}

#[test]
fn a_wrong_level_on_a_shared_row_fails() {
    let source = logging("warn", "", "fn.in");
    let report = walk("wrong-level", &[home_row("fn.in")], &source, &["fn.in"]);
    fails_once(&report, "`fn.in` logged at W, cataloged at D");
}
