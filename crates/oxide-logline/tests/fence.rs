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

/// A throwaway workspace holding `rows` and one script in a page folder the walk reads.
fn walk_page(name: &str, rows: &[Value], script: &str) -> Report {
    walk_page_in("crates/layer-check/page", name, rows, script)
}

/// [`walk_page`] with the script in `folder`.
fn walk_page_in(folder: &str, name: &str, rows: &[Value], script: &str) -> Report {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("fence-page-{name}"));
    let page = root.join(folder);
    fs::create_dir_all(&page).expect("the temporary tree is writable");
    fs::write(
        root.join("logbook.json"),
        json!({ "events": rows }).to_string(),
    )
    .expect("the temporary logbook is writable");
    fs::write(page.join("made.mjs"), script).expect("the temporary script is writable");
    let report = check(&root);
    eprintln!("{name}: {:?}", report.problems);
    report
}

/// The row each page test logs against.
fn page_row() -> Value {
    json!({
        "id": "made.page.result",
        "level": "I",
        "fields": { "check": "a check" },
        "template": "{check}",
    })
}

const GOOD_PAGE_CALL: &str = "logger.info(\"made.page.result\", { check: found.check });\n";

#[test]
fn every_log_call_is_cataloged_and_every_entry_is_used() {
    check(&home()).enforce();
}

#[test]
fn a_page_call_on_its_row_holds_and_a_commented_call_is_not_one() {
    let script = format!("// logger.info(\"a sentence\");\n{GOOD_PAGE_CALL}");
    let report = walk_page("holds", &[page_row()], &script);
    assert!(report.problems.is_empty(), "{:?}", report.problems);
    assert_eq!(report.sites, 1);
    assert_eq!(report.files, 1);
}

#[test]
fn a_page_row_logged_nowhere_is_an_orphan() {
    let report = walk_page("orphan", &[page_row()], "");
    fails_once(&report, "an orphan entry");
}

#[test]
fn a_page_event_with_no_row_fails() {
    let script = format!("{GOOD_PAGE_CALL}logger.info(\"made.page.other\");\n");
    let report = walk_page("no-row", &[page_row()], &script);
    fails_once(&report, "uncataloged event");
}

#[test]
fn a_page_sentence_fails() {
    let script = format!("{GOOD_PAGE_CALL}logger.info(\"check: a sentence\");\n");
    let report = walk_page("sentence", &[page_row()], &script);
    fails_once(&report, "not a bare event id");
}

#[test]
fn a_page_call_at_another_level_fails() {
    let script = "logger.debug(\"made.page.result\", { check: found.check });\n";
    let report = walk_page("level", &[page_row()], script);
    fails_once(&report, "logged at D, cataloged at I");
}

#[test]
fn a_page_field_logged_nowhere_fails() {
    let report = walk_page(
        "field",
        &[page_row()],
        "logger.info(\"made.page.result\");\n",
    );
    fails_once(&report, "the field `check` is logged nowhere");
}

#[test]
fn a_page_field_with_no_row_fails() {
    let script = "logger.info(\"made.page.result\", { check: found.check, extra: 1 });\n";
    let report = walk_page("extra", &[page_row()], script);
    fails_once(&report, "the undeclared field `extra`");
}

#[test]
fn a_page_value_outside_one_object_fails() {
    let script = format!("{GOOD_PAGE_CALL}logger.info(\"made.page.result\", found.check);\n");
    let report = walk_page("loose", &[page_row()], &script);
    fails_once(&report, "go in one object");
}

#[test]
fn the_sound_page_holds_on_its_row_and_its_old_sentences_fail() {
    const FOLDER: &str = "crates/sound-mix/page";
    let holds = walk_page_in(FOLDER, "sound-holds", &[page_row()], GOOD_PAGE_CALL);
    assert!(holds.problems.is_empty(), "{:?}", holds.problems);
    assert_eq!((holds.sites, holds.files), (1, 1));
    let plants = [
        (
            "sound-old-state",
            format!("{GOOD_PAGE_CALL}logger.info(\"sound: the context is\", context.state);\n"),
            "not a bare event id",
        ),
        (
            "sound-old-block",
            format!("{GOOD_PAGE_CALL}logger.debug(\"sound: block\", plan.when, plan.kept);\n"),
            "not a bare event id",
        ),
        (
            "sound-no-row",
            format!("{GOOD_PAGE_CALL}logger.info(\"made.page.other\");\n"),
            "uncataloged event",
        ),
        (
            "sound-loose",
            format!("{GOOD_PAGE_CALL}logger.info(\"made.page.result\", found.check);\n"),
            "go in one object",
        ),
        (
            "sound-level",
            "logger.debug(\"made.page.result\", { check: found.check });\n".to_owned(),
            "logged at D, cataloged at I",
        ),
        (
            "sound-field",
            "logger.info(\"made.page.result\");\n".to_owned(),
            "the field `check` is logged nowhere",
        ),
        (
            "sound-extra",
            "logger.info(\"made.page.result\", { check: found.check, extra: 1 });\n".to_owned(),
            "the undeclared field `extra`",
        ),
        ("sound-orphan", String::new(), "an orphan entry"),
    ];
    for (name, script, words) in plants {
        fails_once(&walk_page_in(FOLDER, name, &[page_row()], &script), words);
    }
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
