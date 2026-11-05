// SPDX-License-Identifier: MIT
//! The fronts on the page's one thread, and the host's lock across two
//! real threads. A test row stands in for a game's: the crate names none.
use crate::host::Host;
use crate::page::{PAGE_THREAD, Page};
use crate::table::{Calls, NO_SLOT, Row, SLOTS};
use crate::{ThreadCalls, Threads};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

const ROW: Row = Row {
    file: "test",
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
    bad_argument: 0x57,
};

#[test]
fn a_page_lock_counts_its_takes() {
    let calls = ThreadCalls::new(&ROW, Page::new());
    calls.initialize(0, 0x100);
    calls.enter(0, 0x100);
    calls.enter(0, 0x100);
    assert_eq!(calls.threads().takes(0x100), 2);
    calls.leave(0, 0x100);
    calls.leave(0, 0x100);
    calls.leave(0, 0x100);
    assert_eq!(calls.threads().takes(0x100), 0);
    assert_eq!(calls.current_thread_id(0), PAGE_THREAD);
}

#[test]
fn a_page_slot_keeps_its_value_until_freed() {
    let calls = ThreadCalls::new(&ROW, Page::new());
    let slot = calls.slot_alloc(0);
    assert_eq!(calls.slot_get(0, slot), 0);
    assert_eq!(calls.slot_set(0, slot, 7), 1);
    assert_eq!(calls.slot_get(0, slot), 7);
    assert_eq!(calls.threads().last_error(), 0);
    assert_eq!(calls.slot_free(0, slot), 1);
    assert_eq!(calls.slot_get(0, slot), 0);
    assert_eq!(calls.threads().last_error(), 0x57);
    assert_eq!(calls.slot_set(0, slot, 7), 0);
}

#[test]
fn every_slot_taken_answers_no_slot() {
    let calls = ThreadCalls::new(&ROW, Page::new());
    for slot in 0..SLOTS {
        assert_eq!(calls.slot_alloc(0), slot);
    }
    assert_eq!(calls.slot_alloc(0), NO_SLOT);
}

#[test]
fn a_counter_answers_its_new_value() {
    let calls = ThreadCalls::new(&ROW, Page::new());
    let counter = AtomicI32::new(0);
    assert_eq!(calls.increment(0, &counter), 1);
    assert_eq!(calls.increment(0, &counter), 2);
    assert_eq!(calls.decrement(0, &counter), 1);
    let low = AtomicI32::new(i32::MIN);
    assert_eq!(calls.decrement(0, &low), i32::MAX);
}

#[test]
fn a_host_lock_reenters_on_its_thread_and_holds_off_another() {
    let host = Arc::new(Host::new());
    host.enter(0x200);
    host.enter(0x200);
    let taken = Arc::new(AtomicBool::new(false));
    let started = Arc::new(Barrier::new(2));
    let other = {
        let (host, taken, started) = (Arc::clone(&host), Arc::clone(&taken), Arc::clone(&started));
        thread::spawn(move || {
            started.wait();
            host.enter(0x200);
            taken.store(true, Ordering::SeqCst);
            host.leave(0x200);
            host.id()
        })
    };
    started.wait();
    thread::sleep(Duration::from_millis(50));
    host.leave(0x200);
    thread::sleep(Duration::from_millis(50));
    assert!(!taken.load(Ordering::SeqCst), "one take still held");
    host.leave(0x200);
    let other_id = other.join().expect("the other thread ends");
    assert!(taken.load(Ordering::SeqCst));
    assert_ne!(other_id, host.id());
}

#[test]
fn a_host_slot_holds_a_value_a_thread() {
    let host = Arc::new(Host::new());
    let slot = host.slot_alloc().expect("a free slot");
    assert!(host.slot_set(slot, 5));
    let seen = {
        let host = Arc::clone(&host);
        thread::spawn(move || {
            let first = host.slot_get(slot);
            host.slot_set(slot, 9);
            (first, host.slot_get(slot))
        })
        .join()
        .expect("the other thread ends")
    };
    assert_eq!(seen, (Some(0), Some(9)));
    assert_eq!(host.slot_get(slot), Some(5));
    assert!(host.slot_free(slot));
    let again = host.slot_alloc().expect("the freed slot");
    assert_eq!((again, host.slot_get(again)), (slot, Some(0)));
}
