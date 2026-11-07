// SPDX-License-Identifier: MIT
//! The thin export `threads-browser.mjs` runs: the page's one thread answering
//! through the fronts, as text. It asks and answers; the script judges.
//!
//! `cargo build -p kernel_threads --example page_threads --features page
//! --target wasm32-unknown-unknown`
#![allow(
    unsafe_code,
    reason = "a page reaches a function only by its exported name"
)]
use core::fmt::Write as _;
use core::sync::atomic::{AtomicI32, Ordering};
use kernel_threads::page::Page;
use kernel_threads::table::{Calls, NO_SLOT, Row};
use kernel_threads::{ThreadCalls, Threads};
use std::sync::OnceLock;

const LOCK: u32 = 7;
const BAD_INDEX: u32 = 100_000;
const MANY: u32 = 1000;

static ROW: Row = Row {
    file: "page",
    calls: Calls {
        current_thread_id: "id",
        initialize: "initialize",
        enter: "enter",
        leave: "leave",
        delete: "delete",
        slot_alloc: "slot_alloc",
        slot_free: "slot_free",
        slot_get: "slot_get",
        slot_set: "slot_set",
        increment: Some("increment"),
        decrement: Some("decrement"),
    },
    bad_argument: 87,
};

static ANSWERS: OnceLock<String> = OnceLock::new();

/// The answer and the last error, the error cleared first.
fn both(calls: &ThreadCalls<Page>, call: impl FnOnce(&ThreadCalls<Page>) -> u32) -> String {
    calls.threads().set_last_error(0);
    let answer = call(calls);
    format!("{answer} error {}", calls.threads().last_error())
}

fn ask() -> String {
    let mut out = String::new();
    let mut row = |name: &str, answer: &dyn core::fmt::Display| {
        writeln!(out, "{name} = {answer}").ok();
    };
    let calls = ThreadCalls::new(&ROW, Page::new());
    row("id", &calls.current_thread_id(0));

    calls.initialize(0, LOCK);
    calls.enter(0, LOCK);
    calls.enter(0, LOCK);
    row("lock.twice", &calls.threads().takes(LOCK));
    calls.leave(0, LOCK);
    row("lock.one_left", &calls.threads().takes(LOCK));
    calls.leave(0, LOCK);
    row("lock.both_left", &calls.threads().takes(LOCK));
    calls.leave(0, LOCK);
    row("lock.over_left", &calls.threads().takes(LOCK));
    for _ in 0..MANY {
        calls.enter(0, LOCK);
    }
    row("lock.many", &calls.threads().takes(LOCK));
    for _ in 0..MANY {
        calls.leave(0, LOCK);
    }
    row("lock.many_left", &calls.threads().takes(LOCK));
    calls.delete(0, LOCK);
    row("lock.deleted", &calls.threads().takes(LOCK));

    let slot = calls.slot_alloc(0);
    row("slot.unset", &both(&calls, |c| c.slot_get(0, slot)));
    row("slot.set", &both(&calls, |c| c.slot_set(0, slot, 42)));
    row("slot.get", &both(&calls, |c| c.slot_get(0, slot)));
    row("slot.free", &both(&calls, |c| c.slot_free(0, slot)));
    row("slot.freed.get", &both(&calls, |c| c.slot_get(0, slot)));
    row("slot.freed.set", &both(&calls, |c| c.slot_set(0, slot, 42)));
    row("slot.freed.free", &both(&calls, |c| c.slot_free(0, slot)));
    row("slot.bad.get", &both(&calls, |c| c.slot_get(0, BAD_INDEX)));
    row(
        "slot.bad.set",
        &both(&calls, |c| c.slot_set(0, BAD_INDEX, 42)),
    );
    row(
        "slot.bad.free",
        &both(&calls, |c| c.slot_free(0, BAD_INDEX)),
    );

    let mut count = 0;
    let mut last = NO_SLOT;
    let mut answer = calls.slot_alloc(0);
    while answer != NO_SLOT && count <= kernel_threads::table::SLOTS {
        count += 1;
        last = answer;
        answer = calls.slot_alloc(0);
    }
    row("slot.count", &count);
    row("slot.out", &answer);
    let _ = calls.slot_set(0, last, 42);
    let _ = calls.slot_free(0, last);
    let again = calls.slot_alloc(0);
    row(
        "slot.again.index",
        &if again == last { "same" } else { "another" },
    );
    row("slot.again.unset", &both(&calls, |c| c.slot_get(0, again)));

    for (name, start, up) in [
        ("counter.inc.5", 5, true),
        ("counter.dec.6", 6, false),
        ("counter.inc.max", i32::MAX, true),
        ("counter.dec.min", i32::MIN, false),
    ] {
        let counter = AtomicI32::new(start);
        let answer = if up {
            calls.increment(0, &counter)
        } else {
            calls.decrement(0, &counter)
        };
        row(
            name,
            &format!("{answer} held {}", counter.load(Ordering::SeqCst)),
        );
    }
    out
}

fn answered() -> &'static str {
    ANSWERS.get_or_init(ask)
}

/// Where the answers' text starts in the page's memory.
#[unsafe(no_mangle)]
pub extern "C" fn answers() -> *const u8 {
    answered().as_ptr()
}

/// How many bytes the answers' text holds.
#[unsafe(no_mangle)]
pub extern "C" fn answers_len() -> usize {
    answered().len()
}
