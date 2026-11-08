// SPDX-License-Identifier: MIT
//! The thin export `files-browser.mjs` runs: the page's one thread answering through the file
//! fronts over the memory folder, as text, and how many rows left the table.
//!
//! `cargo build -p kernel_files --example page_files --features memory --target wasm32-unknown-unknown`
#![allow(
    unsafe_code,
    reason = "a page reaches a function only by its exported name"
)]

use kernel_files::memory::MemoryFolder;
use kernel_files::table::{Calls, Errors, FIND_BYTES, Row};
use kernel_files::{Files, Folder, names};
use std::sync::OnceLock;

#[path = "../src/tests/walk.rs"]
mod walk;
#[path = "../src/tests/want.rs"]
mod want;

static ANSWERS: OnceLock<String> = OnceLock::new();

fn ask() -> String {
    walk::rows().join("\n")
}

/// The answers' first byte in the module's memory.
#[unsafe(no_mangle)]
pub extern "C" fn answers() -> *const u8 {
    ANSWERS.get_or_init(ask).as_ptr()
}

/// The answers' length in bytes.
#[unsafe(no_mangle)]
pub extern "C" fn answers_len() -> usize {
    ANSWERS.get_or_init(ask).len()
}

/// How many rows differ from the table and hold no told reason.
#[unsafe(no_mangle)]
pub extern "C" fn wrong() -> usize {
    walk::all_wrong().len()
}
