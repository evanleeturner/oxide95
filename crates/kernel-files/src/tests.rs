// SPDX-License-Identifier: MIT
//! The file fronts and the name matching, held to a target's own answers. `want.rs` is written
//! from those answers by a script. A difference already told stays out of the plain tests and has
//! an ignored test of its own, so a later fix shows as that test turning green.

use crate::memory::MemoryFolder;
use crate::table::{Calls, Errors, FIND_BYTES, Row};
use crate::{Files, Folder, names};

mod walk;
mod want;

use walk::{
    contents_line, file_lines, find_lines, match_lines, order_lines, out_of_order, reason, rows,
    wrong,
};
use want::{FILES_WANT, FINDS_WANT, MATCHES_WANT};

const NONE: Vec<String> = Vec::new();

/// Every row once, on a line of its own, for what reads the run.
#[test]
fn prints_every_row() {
    let host = if cfg!(windows) { 'w' } else { 'n' };
    println!();
    for row in rows() {
        println!("CHECK {host} {row}");
    }
}

#[test]
fn file_answers_are_the_targets() {
    assert_eq!(wrong(&file_lines(), &FILES_WANT), NONE);
}

#[test]
fn find_answers_are_the_targets() {
    assert_eq!(wrong(&find_lines(), &FINDS_WANT), NONE);
}

#[test]
fn name_matches_are_the_targets() {
    assert_eq!(wrong(&match_lines(), &MATCHES_WANT), NONE);
}

#[test]
fn a_find_answers_in_key_order() {
    assert_eq!(out_of_order(&order_lines()), NONE);
}

#[test]
fn a_write_reaches_the_folder() {
    assert_eq!(contents_line(), "contents k1.txt = hello");
}

/// The rows told apart under `why` that still differ from the target.
fn told(why: &str) -> Vec<String> {
    let table = [
        (file_lines(), &FILES_WANT[..]),
        (find_lines(), &FINDS_WANT[..]),
        (match_lines(), &MATCHES_WANT[..]),
    ];
    let mut out = Vec::new();
    for (got, want) in &table {
        for (got, want) in got.iter().zip(*want) {
            if got != want && reason(want) == Some(why) {
                out.push(format!("got `{got}` want `{want}`"));
            }
        }
    }
    out
}

#[test]
#[ignore = "a share reaches no folder, so a file held share 0 opens again"]
fn a_held_file_refuses_a_second_open() {
    assert_eq!(
        told("a share reaches no folder, so a file held share 0 opens again"),
        NONE
    );
}

#[test]
#[ignore = "a pattern's `*.*` wants a dot, where the target takes a name with none"]
fn star_dot_star_takes_a_name_with_no_dot() {
    assert_eq!(
        told("a pattern's `*.*` wants a dot, where the target takes a name with none"),
        NONE
    );
}

#[test]
#[ignore = "a pattern's `*.` wants a last dot, where the target takes a name with no dot"]
fn star_dot_takes_a_name_with_no_dot() {
    assert_eq!(
        told("a pattern's `*.` wants a last dot, where the target takes a name with no dot"),
        NONE
    );
}

#[test]
#[ignore = "a pattern's last dot stays, where the target drops it"]
fn a_last_dot_is_dropped() {
    assert_eq!(
        told("a pattern's last dot stays, where the target drops it"),
        NONE
    );
}

#[test]
#[ignore = "a find answers files alone, where the target adds folders and the dot names"]
fn a_find_answers_folders_and_the_dot_names() {
    assert_eq!(
        told("a find answers files alone, where the target adds folders and the dot names"),
        NONE
    );
}
