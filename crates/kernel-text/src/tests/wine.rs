// SPDX-License-Identifier: MIT
//! The rows against Wine's answers, run natively: each printed, every one
//! compared. The rows live in their own file, as a page's probe runs the same
//! ones.
use crate as kernel_text;

#[path = "rows.rs"]
mod rows;

#[test]
fn every_row_holds() {
    let lines = rows::all();
    for line in &lines {
        println!("{}", line.text());
    }
    for line in rows::standing() {
        println!("{}", line.text());
    }
    let red: Vec<_> = lines
        .iter()
        .filter(|line| !line.holds())
        .map(|line| line.name.as_str())
        .collect();
    assert!(red.is_empty(), "red rows: {red:?}");
}

#[test]
#[ignore = "the crate differs from Wine here and each stands with its reason: told, not fixed"]
fn wines_answers_where_the_crate_differs() {
    let lines = rows::standing();
    let red: Vec<_> = lines
        .iter()
        .filter(|line| !line.holds())
        .map(|line| line.name.as_str())
        .collect();
    assert!(red.is_empty(), "standing: {red:?}");
}
