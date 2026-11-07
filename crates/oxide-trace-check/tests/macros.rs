// SPDX-License-Identifier: MIT
//! Each of `oxide_trace`'s 14 macros against a table worked by hand, one test a macro and
//! named for it. Each says: it writes the table's one row, and the logbook knows that row;
//! it evaluates each wrapped expression once and in the order its doc gives; on a real page
//! it touches memory as often as the bare line it wraps, and at least once; and with
//! logging off all of that holds with no row, but for `enter!`, whose arguments sit inside
//! the log call and so are not evaluated at all.
//!
//! A red here names `oxide_trace`, not a program built on it.
#![cfg(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(windows, target_arch = "x86")
))]

use std::fmt::Debug;
use std::hint::black_box;
use std::sync::{Mutex, PoisonError};

use oxide_trace_check::{CELL, Guard, SOURCE, SOURCE_VALUE, Session, Ticks, book_fault};

/// What [`CELL`] holds before each site.
const START: u32 = 0x2a;

/// The calls of [`counting_copy`], in order, each one's three arguments. The site's counting
/// functions touch no memory, so a guarded source shows "the bytes copied are not read
/// again to log". One static per test: the tests run in parallel, and each empties its own
/// list outside the gate.
static COPY_CALLS: Mutex<Vec<[usize; 3]>> = Mutex::new(Vec::new());

/// The calls of [`counting_fill`], as [`COPY_CALLS`] holds [`counting_copy`]'s.
static FILL_CALLS: Mutex<Vec<[usize; 3]>> = Mutex::new(Vec::new());

fn cell() -> *mut u32 {
    CELL as *mut u32
}

/// A type with no `Value`.
struct Opaque;

/// What the worked table says of one site.
struct Table<'table, T> {
    row: &'table str,
    order: &'table [&'table str],
    value: T,
    after: u32,
}

/// Runs a site twice, with logging on and off, each from a fresh page.
fn both_ways<T: PartialEq + Debug>(
    session: &Session,
    guard: &Guard<'_>,
    faults: usize,
    wrapped: impl Fn(&Ticks) -> T,
    table: &Table<'_, T>,
) {
    for on in [true, false] {
        session.logging(on);
        guard.reset(START);
        let ticks = Ticks::default();
        let value = wrapped(&ticks);
        let touched = guard.faults();
        let rows = session.take();
        let texts: Vec<String> = rows.iter().map(oxide_trace_check::Row::text).collect();
        let expected_rows = if on { vec![table.row] } else { vec![] };
        assert_eq!(texts, expected_rows, "the rows, logging on: {on}");
        for row in &rows {
            assert_eq!(book_fault(row), None, "against the logbook");
        }
        assert_eq!(
            ticks.names(),
            table.order,
            "the evaluations, logging on: {on}"
        );
        assert_eq!(touched, faults, "the page's touches, logging on: {on}");
        assert_eq!(value, table.value, "the value, logging on: {on}");
        assert_eq!(guard.cell(), table.after, "the cell, logging on: {on}");
    }
}

/// A site on the page, judged against the same access written bare.
fn on_page<T: PartialEq + Debug>(
    bare: impl Fn() -> T,
    wrapped: impl Fn(&Ticks) -> T,
    table: &Table<'_, T>,
) {
    let session = Session::begin();
    let guard = session.guard();
    guard.reset(START);
    let value = bare();
    let faults = guard.faults();
    assert!(faults >= 1, "the bare site never touched the page");
    assert_eq!(value, table.value, "the bare site's value");
    assert_eq!(guard.cell(), table.after, "the bare site's cell");
    both_ways(&session, &guard, faults, wrapped, table);
}

/// A site that must not touch the page.
fn off_page<T: PartialEq + Debug>(wrapped: impl Fn(&Ticks) -> T, table: &Table<'_, T>) {
    let session = Session::begin();
    let guard = session.guard();
    both_ways(&session, &guard, 0, wrapped, table);
}

fn take_calls(calls: &Mutex<Vec<[usize; 3]>>) -> Vec<[usize; 3]> {
    std::mem::take(&mut *calls.lock().unwrap_or_else(PoisonError::into_inner))
}

/// Stands where `core::ptr::copy_nonoverlapping` would, and touches nothing.
unsafe fn counting_copy(source: *const u8, destination: *mut u8, count: usize) {
    COPY_CALLS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push([source.addr(), destination.addr(), count]);
}

/// Stands where `core::ptr::write_bytes` would, and touches nothing.
unsafe fn counting_fill(destination: *mut u8, byte: u8, count: usize) {
    FILL_CALLS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push([destination.addr(), usize::from(byte), count]);
}

/// The instrument before anything is judged by it: 2 on 2 hand reads, 0 on none.
#[test]
fn counter() {
    let session = Session::begin();
    let guard = session.guard();
    guard.reset(START);
    assert_eq!(guard.faults(), 0, "no read yet");
    // SAFETY: the cell is mapped and aligned, and the guard lets each fault through.
    let first = unsafe { cell().read_volatile() };
    // SAFETY: as above.
    let second = unsafe { cell().read_volatile() };
    assert_eq!((first, second), (START, START), "the values read");
    assert_eq!(guard.faults(), 2, "2 hand reads");
}

#[test]
fn enter() {
    let session = Session::begin();
    let guard = session.guard();

    oxide_trace::enter!("401000");
    oxide_trace::enter!("401000", 0x2a_u32, -1_i8, Opaque);
    let rows = session.take();
    let texts: Vec<String> = rows.iter().map(oxide_trace_check::Row::text).collect();
    assert_eq!(texts, ["fn.in f=401000", "fn.in f=401000 a=2a,ff,?"]);
    for row in &rows {
        assert_eq!(book_fault(row), None, "against the logbook");
    }

    let ticks = Ticks::default();
    oxide_trace::enter!(
        "401000",
        *ticks.at("a", &0x2a_u32),
        *ticks.at("b", &-1_i8),
        *ticks.at("c", &Opaque)
    );
    assert_eq!(session.take().len(), 1, "one row");
    assert_eq!(ticks.names(), ["a", "b", "c"], "once each, in order");

    // A pointer to the page prints its address, and printing it touches nothing.
    guard.reset(START);
    let pointer = cell();
    oxide_trace::enter!("401000", pointer);
    let texts: Vec<String> = session
        .take()
        .iter()
        .map(oxide_trace_check::Row::text)
        .collect();
    assert_eq!(texts, ["fn.in f=401000 a=20000010"]);
    assert_eq!(guard.faults(), 0, "the address is not a read");

    // Logging off: no row, and the arguments, inside the log call, are not evaluated.
    session.logging(false);
    let ticks = Ticks::default();
    oxide_trace::enter!("401000");
    oxide_trace::enter!("401000", *ticks.at("a", &0x2a_u32));
    assert_eq!(session.take().len(), 0, "no row");
    assert_eq!(ticks.names(), [""; 0], "0 evaluations, by the worked table");
}

#[test]
fn rt() {
    off_page(
        |ticks| oxide_trace::rt!("401000", ticks.hit("value", 7_u16)),
        &Table {
            row: "fn.ret f=401000 v=7",
            order: &["value"],
            value: 7,
            after: START,
        },
    );
    on_page(
        // SAFETY: the cell is mapped and aligned, and the guard lets each fault through.
        || unsafe { cell().read() },
        // SAFETY: as above.
        |ticks| oxide_trace::rt!("401000", ticks.hit("value", unsafe { cell().read() })),
        &Table {
            row: "fn.ret f=401000 v=2a",
            order: &["value"],
            value: START,
            after: START,
        },
    );
}

#[test]
fn rd() {
    on_page(
        // SAFETY: the cell is mapped and aligned, and the guard lets each fault through.
        || unsafe { cell().read_unaligned() },
        // SAFETY: as above.
        |ticks| unsafe { oxide_trace::rd!("401000", ticks.hit("p", cell()), read_unaligned) },
        &Table {
            row: "mem.rd f=401000 a=20000010 v=2a",
            order: &["p"],
            value: START,
            after: START,
        },
    );
}

#[test]
fn rdf() {
    on_page(
        // SAFETY: the cell is mapped and aligned, and the guard lets each fault through.
        || unsafe { core::ptr::read_unaligned(cell()) },
        // SAFETY: as above.
        |ticks| unsafe {
            oxide_trace::rdf!("401000", core::ptr::read_unaligned, ticks.hit("p", cell()))
        },
        &Table {
            row: "mem.rd f=401000 a=20000010 v=2a",
            order: &["p"],
            value: START,
            after: START,
        },
    );
}

#[test]
fn dr() {
    on_page(
        // SAFETY: the cell is mapped and aligned, and the guard lets each fault through.
        || unsafe { *cell() },
        // SAFETY: as above.
        |ticks| unsafe { oxide_trace::dr!("401000", ticks.hit("p", cell())) },
        &Table {
            row: "mem.rd f=401000 a=20000010 v=2a",
            order: &["p"],
            value: START,
            after: START,
        },
    );

    // A `&mut` is reborrowed, never moved: the use after the macro is the check.
    let session = Session::begin();
    let mut local = START;
    let borrowed = &mut local;
    // SAFETY: `borrowed` is a live `&mut`.
    let read = unsafe { oxide_trace::dr!("401000", borrowed) };
    *borrowed += 1;
    assert_eq!((read, local), (START, START + 1));
    assert_eq!(session.take().len(), 1, "one row");
}

#[test]
fn wr() {
    on_page(
        // SAFETY: the cell is mapped and aligned, and the guard lets each fault through.
        || unsafe { cell().write_unaligned(0x55_u32) },
        // SAFETY: as above.
        |ticks| unsafe {
            oxide_trace::wr!(
                "401000",
                ticks.hit("p", cell()),
                write_unaligned,
                ticks.hit("v", 0x55_u32)
            );
        },
        &Table {
            row: "mem.wr f=401000 a=20000010 v=55",
            order: &["p", "v"],
            value: (),
            after: 0x55,
        },
    );
}

#[test]
fn dw() {
    on_page(
        // SAFETY: the cell is mapped and aligned, and the guard lets each fault through.
        || unsafe { *cell() = 0x66_u32 },
        // SAFETY: as above.
        |ticks| unsafe {
            oxide_trace::dw!("401000", ticks.hit("p", cell()), ticks.hit("v", 0x66_u32));
        },
        &Table {
            row: "mem.wr f=401000 a=20000010 v=66",
            order: &["v", "p"],
            value: (),
            after: 0x66,
        },
    );
}

#[test]
fn du() {
    on_page(
        // SAFETY: the cell is mapped and aligned, and the guard lets each fault through.
        || unsafe { *cell() += black_box(1_u32) },
        // SAFETY: as above.
        |ticks| unsafe {
            oxide_trace::du!("401000", ticks.hit("p", cell()), +=, ticks.hit("v", 1_u32));
        },
        &Table {
            row: "mem.upd f=401000 a=20000010 op=+= v=1",
            order: &["v", "p"],
            value: (),
            after: 0x2b,
        },
    );
}

#[test]
fn gr() {
    on_page(
        // SAFETY: the cell is mapped and aligned, and the guard lets each fault through.
        || unsafe { *cell() },
        // SAFETY: as above.
        |ticks| unsafe { oxide_trace::gr!("401000", "4a0000", *ticks.hit("p", cell())) },
        &Table {
            row: "glob.rd f=401000 g=4a0000 v=2a",
            order: &["p"],
            value: START,
            after: START,
        },
    );
}

#[test]
fn gw() {
    on_page(
        // SAFETY: the cell is mapped and aligned, and the guard lets each fault through.
        || unsafe { *cell() = 0x55_u32 },
        // SAFETY: as above.
        |ticks| unsafe {
            oxide_trace::gw!(
                "401000",
                "4a0000",
                *ticks.hit("p", cell()),
                ticks.hit("v", 0x55_u32)
            );
        },
        &Table {
            row: "glob.wr f=401000 g=4a0000 v=55",
            order: &["v", "p"],
            value: (),
            after: 0x55,
        },
    );
}

#[test]
fn gu() {
    on_page(
        // SAFETY: the cell is mapped and aligned, and the guard lets each fault through.
        || unsafe { *cell() += black_box(1_u32) },
        // SAFETY: as above.
        |ticks| unsafe {
            oxide_trace::gu!(
                "401000",
                "4a0000",
                *ticks.hit("p", cell()),
                +=,
                ticks.hit("v", 1_u32)
            );
        },
        &Table {
            row: "glob.upd f=401000 g=4a0000 op=+= v=1",
            order: &["v", "p"],
            value: (),
            after: 0x2b,
        },
    );
}

#[test]
fn cr() {
    off_page(
        |ticks| oxide_trace::cr!("401000", "ind", ticks.hit("call", 9_u32)),
        &Table {
            row: "call.ret f=401000 c=ind v=9",
            order: &["call"],
            value: 9,
            after: START,
        },
    );
    on_page(
        // SAFETY: the cell is mapped and aligned, and the guard lets each fault through.
        || unsafe { cell().read() },
        // SAFETY: as above.
        |ticks| oxide_trace::cr!("401000", "ind", ticks.hit("call", unsafe { cell().read() })),
        &Table {
            row: "call.ret f=401000 c=ind v=2a",
            order: &["call"],
            value: START,
            after: START,
        },
    );
}

#[test]
fn cp() {
    take_calls(&COPY_CALLS);
    off_page(
        // SAFETY: `counting_copy` touches nothing through its arguments.
        |ticks| unsafe {
            oxide_trace::cp!(
                "401000",
                counting_copy,
                ticks.hit("s", 0x1000 as *const u8),
                ticks.hit("d", 0x2000 as *mut u8),
                ticks.hit("n", 4_usize)
            );
        },
        &Table {
            row: "mem.copy f=401000 s=1000 d=2000 n=4",
            order: &["s", "d", "n"],
            value: (),
            after: START,
        },
    );
    assert_eq!(
        take_calls(&COPY_CALLS),
        [[0x1000, 0x2000, 4]; 2],
        "1 call each way"
    );

    // The source is on the guarded page: logging the copy does not read its bytes.
    off_page(
        // SAFETY: `counting_copy` touches nothing through its arguments.
        |ticks| unsafe {
            oxide_trace::cp!(
                "401000",
                counting_copy,
                ticks.hit("s", SOURCE as *const u8),
                ticks.hit("d", CELL as *mut u8),
                ticks.hit("n", 4_usize)
            );
        },
        &Table {
            row: "mem.copy f=401000 s=20000020 d=20000010 n=4",
            order: &["s", "d", "n"],
            value: (),
            after: START,
        },
    );
    assert_eq!(
        take_calls(&COPY_CALLS),
        [[SOURCE, CELL, 4]; 2],
        "1 call each way"
    );

    on_page(
        // SAFETY: both cells are mapped, aligned and apart, and the guard lets each fault
        // through.
        || unsafe {
            core::ptr::copy_nonoverlapping(
                SOURCE as *const u8,
                CELL as *mut u8,
                black_box(4_usize),
            );
        },
        // SAFETY: as above.
        |ticks| unsafe {
            oxide_trace::cp!(
                "401000",
                core::ptr::copy_nonoverlapping,
                ticks.hit("s", SOURCE as *const u8),
                ticks.hit("d", CELL as *mut u8),
                ticks.hit("n", 4_usize)
            );
        },
        &Table {
            row: "mem.copy f=401000 s=20000020 d=20000010 n=4",
            order: &["s", "d", "n"],
            value: (),
            after: SOURCE_VALUE,
        },
    );
}

#[test]
fn fill() {
    take_calls(&FILL_CALLS);
    off_page(
        // SAFETY: `counting_fill` touches nothing through its arguments.
        |ticks| unsafe {
            oxide_trace::fill!(
                "401000",
                counting_fill,
                ticks.hit("d", 0x2000 as *mut u8),
                ticks.hit("b", 0xab_u8),
                ticks.hit("n", 4_usize)
            );
        },
        &Table {
            row: "mem.fill f=401000 d=2000 b=ab n=4",
            order: &["d", "b", "n"],
            value: (),
            after: START,
        },
    );
    assert_eq!(
        take_calls(&FILL_CALLS),
        [[0x2000, 0xab, 4]; 2],
        "1 call each way"
    );

    on_page(
        // SAFETY: the cell is mapped and aligned, and the guard lets each fault through.
        || unsafe { core::ptr::write_bytes(CELL as *mut u8, 0xab_u8, black_box(4_usize)) },
        // SAFETY: as above.
        |ticks| unsafe {
            oxide_trace::fill!(
                "401000",
                core::ptr::write_bytes,
                ticks.hit("d", CELL as *mut u8),
                ticks.hit("b", 0xab_u8),
                ticks.hit("n", 4_usize)
            );
        },
        &Table {
            row: "mem.fill f=401000 d=20000010 b=ab n=4",
            order: &["d", "b", "n"],
            value: (),
            after: 0xabab_abab,
        },
    );
}

/// Stands where `core::ptr::write_bytes` would, for [`real_forms`] alone, and touches nothing.
unsafe fn silent_fill(_destination: *mut u8, _byte: u8, _count: usize) {}

/// A global of [`real_forms`] alone, declared as a port's are and named bare.
static mut GLOBAL: u32 = 0;

/// The forms a port's real calls take that the sites above do not: arguments that are plain
/// locals, a global named bare, a computed pointer, `read_volatile` as the reader, and integer
/// literals with no type of their own, left to inference. Each row is worked by hand.
#[test]
fn real_forms() {
    let first = 0x2a_u32;
    let second = -1_i8;
    off_page(
        |_| {
            oxide_trace::enter!("401000", first, second);
        },
        &Table {
            row: "fn.in f=401000 a=2a,ff",
            order: &[],
            value: (),
            after: START,
        },
    );

    on_page(
        // SAFETY: the cell is mapped and aligned, and the guard lets each fault through.
        || unsafe { core::ptr::read_volatile(CELL.wrapping_add(0) as *const u16) },
        // SAFETY: as above.
        |_| unsafe {
            oxide_trace::rdf!(
                "401000",
                core::ptr::read_volatile,
                CELL.wrapping_add(0) as *const u16
            )
        },
        &Table {
            row: "mem.rd f=401000 a=20000010 v=2a",
            order: &[],
            value: 0x2a_u16,
            after: START,
        },
    );

    on_page(
        // SAFETY: the cell is mapped and aligned, and the guard lets each fault through.
        || unsafe {
            *cell() |= black_box(1_u32);
        },
        // SAFETY: as above.
        |_| unsafe {
            oxide_trace::du!("401000", cell().add(0), |=, 1);
        },
        &Table {
            row: "mem.upd f=401000 a=20000010 op=|= v=1",
            order: &[],
            value: (),
            after: 0x2b,
        },
    );

    off_page(
        // SAFETY: only this test names `GLOBAL`, and it runs one site at a time.
        |_| unsafe {
            GLOBAL = 0x2a;
            oxide_trace::gr!("401000", "4a0000", GLOBAL)
        },
        &Table {
            row: "glob.rd f=401000 g=4a0000 v=2a",
            order: &[],
            value: 0x2a_u32,
            after: START,
        },
    );

    off_page(
        // SAFETY: as above.
        |_| unsafe {
            oxide_trace::gw!("401000", "4a0000", GLOBAL, 0xffff_ffff);
            GLOBAL
        },
        &Table {
            row: "glob.wr f=401000 g=4a0000 v=ffffffff",
            order: &[],
            value: 0xffff_ffff_u32,
            after: START,
        },
    );

    off_page(
        // SAFETY: `silent_fill` touches nothing through its arguments.
        |_| unsafe {
            oxide_trace::fill!("401000", silent_fill, 0x2000 as *mut u8, 0xab, 0x2066);
        },
        &Table {
            row: "mem.fill f=401000 d=2000 b=ab n=2066",
            order: &[],
            value: (),
            after: START,
        },
    );
}
