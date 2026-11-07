// SPDX-License-Identifier: MIT
//! The link's first question: does a 32-bit program, with no crate of ours, catch a panic
//! under Wine? Prints `caught=true pointer_bytes=4` when the link is right. One file, built
//! by `wine.sh link`; no Cargo target, so nothing else in the workspace can be its fault.
fn main() {
    let caught = std::panic::catch_unwind(|| panic!("planted"));
    println!(
        "caught={} pointer_bytes={}",
        caught.is_err(),
        size_of::<usize>()
    );
}
