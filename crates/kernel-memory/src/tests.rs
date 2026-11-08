// SPDX-License-Identifier: MIT
//! The rows, run natively: each printed, every one compared. The rows live
//! in their own file, as a page's probe runs the same ones.
use crate as kernel_memory;

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
        .map(|line| line.name)
        .collect();
    assert!(red.is_empty(), "red rows: {red:?}");
}

#[test]
#[ignore = "the crate keeps a game's rule where today's documents differ: told, not fixed"]
fn the_documents_rules_where_the_crate_differs() {
    let red: Vec<_> = rows::standing()
        .into_iter()
        .filter(|line| !line.holds())
        .map(|line| line.name)
        .collect();
    assert!(red.is_empty(), "standing: {red:?}");
}
