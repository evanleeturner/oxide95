// SPDX-License-Identifier: MIT
//! The check's promises, on a synthetic list of 3 names, never a game's:
//! a complete pick passes; a removed, emptied, case-changed or moved file
//! each gives its own finding and its own sentence naming the file; a file
//! the list marks empty at source passes empty.
use alloc::string::ToString;
use alloc::vec::Vec;

use super::test_log::debug_lines;
use crate::{Expected, Finding, Picked, check};

/// An expected file the disc holds with bytes, as all 3 of the list are.
const fn file(path: &str) -> Expected<'_> {
    Expected {
        path,
        empty_at_source: false,
    }
}

const EXPECTED: [Expected<'static>; 3] = [
    file("README.TXT"),
    file("DATA/LEVEL.DAT"),
    file("DATA/SOUND.RAW"),
];

/// The whole synthetic tree, each file readable and non-empty.
fn complete() -> Vec<Picked<'static>> {
    EXPECTED
        .iter()
        .map(|file| Picked {
            path: file.path,
            size: Some(100),
        })
        .collect()
}

#[test]
fn a_complete_pick_has_no_finding() {
    assert_eq!(check(&EXPECTED, &complete()), []);
}

#[test]
fn a_file_the_list_does_not_name_is_no_finding() {
    let mut picked = complete();
    picked.push(Picked {
        path: "EXTRA.BIN",
        size: Some(0),
    });
    assert_eq!(check(&EXPECTED, &picked), []);
}

#[test]
fn a_removed_file_is_missing_by_its_name() {
    let picked: Vec<_> = complete()
        .into_iter()
        .filter(|f| f.path != "DATA/LEVEL.DAT")
        .collect();
    assert_eq!(
        check(&EXPECTED, &picked),
        [Finding::Missing {
            expected: "DATA/LEVEL.DAT"
        }]
    );
}

#[test]
fn an_emptied_file_is_empty() {
    let mut picked = complete();
    picked[2].size = Some(0);
    assert_eq!(
        check(&EXPECTED, &picked),
        [Finding::Empty {
            path: "DATA/SOUND.RAW"
        }]
    );
}

/// The mark changes the empty answer alone: a marked file picked empty, or
/// picked with bytes, is no finding, while the same empty pick unmarked is
/// `Empty`, and a marked file moved after the pick is still `Unreadable`.
#[test]
fn a_file_empty_at_source_passes_empty() {
    let mut expected = EXPECTED;
    expected[1].empty_at_source = true;
    let mut picked = complete();
    assert_eq!(check(&expected, &picked), []);
    picked[1].size = Some(0);
    assert_eq!(check(&expected, &picked), []);
    assert_eq!(
        check(&EXPECTED, &picked),
        [Finding::Empty {
            path: "DATA/LEVEL.DAT"
        }]
    );
    picked[1].size = None;
    assert_eq!(
        check(&expected, &picked),
        [Finding::Unreadable {
            path: "DATA/LEVEL.DAT"
        }]
    );
}

#[test]
fn a_changed_case_is_named_with_what_was_found() {
    let mut picked = complete();
    picked[0].path = "readme.txt";
    assert_eq!(
        check(&EXPECTED, &picked),
        [Finding::CaseChanged {
            expected: "README.TXT",
            found: "readme.txt"
        }]
    );
}

#[test]
fn a_file_moved_after_the_pick_is_unreadable() {
    let mut picked = complete();
    picked[1].size = None;
    assert_eq!(
        check(&EXPECTED, &picked),
        [Finding::Unreadable {
            path: "DATA/LEVEL.DAT"
        }]
    );
}

#[test]
fn an_exact_path_wins_over_a_case_changed_one() {
    let mut picked = complete();
    picked.insert(
        0,
        Picked {
            path: "readme.txt",
            size: Some(0),
        },
    );
    assert_eq!(check(&EXPECTED, &picked), []);
}

#[test]
fn findings_keep_the_list_order_and_each_sentence_names_its_file() {
    let picked = [
        Picked {
            path: "DATA/SOUND.RAW",
            size: None,
        },
        Picked {
            path: "data/level.dat",
            size: Some(100),
        },
    ];
    let findings = check(&EXPECTED, &picked);
    let kinds: Vec<_> = findings.iter().map(Finding::kind).collect();
    assert_eq!(kinds, ["missing", "case", "unreadable"]);
    let sentences: Vec<_> = findings.iter().map(ToString::to_string).collect();
    for (finding, sentence) in findings.iter().zip(&sentences) {
        assert!(sentence.contains(finding.expected()), "{sentence}");
        assert!(sentence.ends_with('.'), "{sentence}");
    }
    let mut distinct = sentences.clone();
    distinct.sort();
    distinct.dedup();
    assert_eq!(distinct.len(), 3);
}

/// The check writes one `disc.finding` line a finding, in the list's
/// order, its kind and its expected name, then one `disc.check` line: the
/// list's count, the pick's and the findings'. A complete pick writes the
/// count line alone.
#[test]
fn the_check_writes_a_line_a_finding_then_its_counts() {
    let mut picked: Vec<_> = complete()
        .into_iter()
        .filter(|f| f.path != "README.TXT")
        .collect();
    picked[0].size = Some(0);
    let lines = debug_lines(|| assert_eq!(check(&EXPECTED, &picked).len(), 2));
    assert_eq!(
        lines,
        [
            "disc.finding k=missing p=README.TXT",
            "disc.finding k=empty p=DATA/LEVEL.DAT",
            "disc.check n=3 m=2 f=2",
        ]
    );
    let lines = debug_lines(|| assert_eq!(check(&EXPECTED, &complete()), []));
    assert_eq!(lines, ["disc.check n=3 m=3 f=0"]);
}
