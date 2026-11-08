// SPDX-License-Identifier: MIT
//! The memory helpers' rows, on a `Memory` of their own and a row of no game.
//! One file with two callers, the test beside it and a page's probe, so a
//! green on a page reads the same rows.
//!
//! A row runs one small scene and compares what the spy saw with the rule. A
//! red is a value, never a panic, so every row prints on every target. The
//! spy keeps words in a map and invents small pointers it never reads
//! through, so these run at any width; `real_bytes` alone touches memory, at
//! 32 bits, as the debug rules cut a pointer to a 32-bit word. The handler's
//! yeses are a counted queue: calloc and realloc retry a size above the
//! largest without asking the heap, so a yes forever would never end.
//!
//! `ME1`: malloc, calloc, realloc and free keep the C library's documented rules.
//! `ME2`: a failed allocation asks the new handler, and tries again on its yes.
//! `ME3`: the debug heap's block header, its two guards and its list of blocks.
#![allow(
    unsafe_code,
    reason = "the seam's own contract is unsafe, and a row has to call it"
)]

use super::kernel_memory::debug::{DebugHeap, Words};
use super::kernel_memory::table::{Helper, List, Row};
use super::kernel_memory::{Heap, Memory};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, VecDeque};
use std::fmt::Debug;

const LARGEST: u32 = 0xffff_ffe0;
const GUARD: u32 = 0xfdfd_fdfd;
const B1: u32 = 0x1000;
const B2: u32 = 0x2000;
const B3: u32 = 0x3000;
const USER: u32 = 0x20;

const fn helper(address: u32, name: &'static str) -> Helper {
    Helper { address, name }
}

static ROW: Row = Row {
    file: "row",
    heap: 0x10,
    new_mode: 0x14,
    call_new_handler: 0x18,
    malloc: helper(0x20, "row.malloc"),
    operator_new: Some(helper(0x24, "row.new")),
    free: helper(0x28, "row.free"),
    operator_delete: Some(helper(0x2c, "row.delete")),
    calloc: Some(helper(0x30, "row.calloc")),
    realloc: Some(helper(0x34, "row.realloc")),
};

static LIST: List = List {
    first: 0x40,
    last: 0x44,
    lock: 0x48,
    unlock: 0x4c,
    number: 4,
};

/// One row's outcome, printed by whichever caller runs it.
pub struct Line {
    pub kind: &'static str,
    pub check: &'static str,
    pub name: &'static str,
    pub rule: &'static str,
    pub got: String,
    pub want: String,
}

impl Line {
    pub fn holds(&self) -> bool {
        self.got == self.want
    }

    pub fn text(&self) -> String {
        format!(
            "{} {} {} rule={} = got={} want={}",
            self.kind, self.check, self.name, self.rule, self.got, self.want
        )
    }
}

fn row(
    out: &mut Vec<Line>,
    check: &'static str,
    name: &'static str,
    rule: &'static str,
    got: &dyn Debug,
    want: &dyn Debug,
) {
    out.push(Line {
        kind: "CHECK",
        check,
        name,
        rule,
        got: format!("{got:?}"),
        want: format!("{want:?}"),
    });
}

/// A `Memory` that records every call and answers each from a queue.
#[derive(Default)]
struct Spy {
    answers: RefCell<VecDeque<u32>>,
    moves: RefCell<VecDeque<u32>>,
    asks: RefCell<Vec<(u32, bool, i32)>>,
    resizes: RefCell<Vec<(u32, u32)>>,
    releases: RefCell<Vec<(u32, i32)>>,
    mode: Cell<bool>,
    mode_reads: Cell<u32>,
    yeses: RefCell<VecDeque<bool>>,
    flips: RefCell<VecDeque<bool>>,
    handled: RefCell<Vec<(u32, i32)>>,
    quiets: Cell<u32>,
    words: RefCell<BTreeMap<u32, u32>>,
    held: Cell<i32>,
    deepest: Cell<i32>,
    locks: Cell<u32>,
    unlocked_touches: Cell<u32>,
}

impl Spy {
    fn new() -> Self {
        Self::default()
    }

    /// The blocks to answer, in order; a 0 or an empty queue is a refusal.
    fn blocks(self, blocks: &[u32]) -> Self {
        self.answers.borrow_mut().extend(blocks);
        self
    }

    fn moved(self, blocks: &[u32]) -> Self {
        self.moves.borrow_mut().extend(blocks);
        self
    }

    fn moded(self) -> Self {
        self.mode.set(true);
        self
    }

    /// The handler's answers, in order; an empty queue is a no.
    fn says(self, yeses: &[bool]) -> Self {
        self.yeses.borrow_mut().extend(yeses);
        self
    }

    /// What the handler leaves `_newmode` at, call by call.
    fn flips(self, modes: &[bool]) -> Self {
        self.flips.borrow_mut().extend(modes);
        self
    }

    fn asks(&self) -> Vec<(u32, bool, i32)> {
        self.asks.borrow().clone()
    }

    fn sizes_handled(&self) -> Vec<u32> {
        self.handled.borrow().iter().map(|seen| seen.0).collect()
    }

    fn word(&self, address: u32) -> u32 {
        self.words.borrow().get(&address).copied().unwrap_or(0)
    }

    /// The list from its first end by next, and from its last by previous.
    fn walks(&self) -> (Vec<u32>, Vec<u32>) {
        let walk = |start: u32, link: u32| {
            let mut seen = Vec::new();
            let mut at = self.word(start);
            while at != 0 && seen.len() < 8 {
                seen.push(at);
                at = self.word(at + link);
            }
            seen
        };
        (walk(LIST.first, 0), walk(LIST.last, 4))
    }

    /// A header's 8 words and the guard after its `size` user bytes.
    fn header(&self, block: u32, size: u32) -> Vec<u32> {
        let mut words: Vec<u32> = (0..8).map(|index| self.word(block + index * 4)).collect();
        words.push(self.word(block + USER + size));
        words
    }
}

impl Memory for Spy {
    fn heap(&self) -> usize {
        0x50
    }

    fn alloc(&self, bytes: u32, zeroed: bool) -> *mut u8 {
        self.asks
            .borrow_mut()
            .push((bytes, zeroed, self.held.get()));
        pointer(self.answers.borrow_mut().pop_front().unwrap_or(0))
    }

    unsafe fn resize(&self, block: *mut u8, bytes: u32) -> *mut u8 {
        self.resizes.borrow_mut().push((at(block), bytes));
        pointer(self.moves.borrow_mut().pop_front().unwrap_or(0))
    }

    unsafe fn release(&self, block: *mut u8) {
        self.releases
            .borrow_mut()
            .push((at(block), self.held.get()));
    }

    fn new_mode(&self) -> bool {
        self.mode_reads.set(self.mode_reads.get() + 1);
        self.mode.get()
    }

    fn new_handler(&self, bytes: u32) -> bool {
        self.handled.borrow_mut().push((bytes, self.held.get()));
        if let Some(mode) = self.flips.borrow_mut().pop_front() {
            self.mode.set(mode);
        }
        self.yeses.borrow_mut().pop_front().unwrap_or(false)
    }

    fn quiet(&self, line: impl FnOnce()) {
        self.quiets.set(self.quiets.get() + 1);
        line();
    }
}

impl Words for Spy {
    fn lock(&self) {
        let held = self.held.get() + 1;
        self.held.set(held);
        self.deepest.set(self.deepest.get().max(held));
        self.locks.set(self.locks.get() + 1);
    }

    fn unlock(&self) {
        self.held.set(self.held.get() - 1);
    }

    unsafe fn read(&self, address: u32) -> u32 {
        if self.held.get() == 0 {
            self.unlocked_touches.set(self.unlocked_touches.get() + 1);
        }
        self.word(address)
    }

    unsafe fn write(&self, address: u32, value: u32) {
        if self.held.get() == 0 {
            self.unlocked_touches.set(self.unlocked_touches.get() + 1);
        }
        self.words.borrow_mut().insert(address, value);
    }
}

/// A word as a pointer; the spy never reads through it.
fn pointer(address: u32) -> *mut u8 {
    core::ptr::without_provenance_mut(address as usize)
}

/// A pointer as a word: cut, as the debug rules cut it, so a plant's wild
/// pointer is a value here and not a panic.
#[allow(
    clippy::cast_possible_truncation,
    reason = "the spy's own blocks fit; a wild pointer only has to differ from them"
)]
fn at(block: *mut u8) -> u32 {
    block.addr() as u32
}

fn plain(spy: Spy) -> Heap<Spy> {
    Heap::new(&ROW, spy)
}

fn debug(spy: Spy) -> DebugHeap<Spy> {
    DebugHeap::new(&ROW, &LIST, spy)
}

fn free(heap: &Heap<Spy>, block: u32) {
    // SAFETY: the spy reads through no pointer, so any word is safe to hand it.
    unsafe { heap.free(pointer(block)) }
}

fn delete(heap: &Heap<Spy>, block: u32) {
    // SAFETY: as `free`.
    unsafe { heap.operator_delete(pointer(block)) }
}

fn realloc(heap: &Heap<Spy>, block: u32, bytes: u32) -> u32 {
    // SAFETY: as `free`.
    at(unsafe { heap.realloc(pointer(block), bytes) })
}

fn debug_free(heap: &DebugHeap<Spy>, block: u32) {
    // SAFETY: as `free`: the words are the spy's map, never memory.
    unsafe { heap.free(pointer(block)) }
}

/// A size of 0 is made 1; the largest size is asked; past it nothing is.
fn me1_sizes(out: &mut Vec<Line>) {
    let h = plain(Spy::new().blocks(&[B1]));
    let v = at(h.malloc(0));
    let got = (v, h.memory().asks());
    row(
        out,
        "ME1",
        "malloc-0-asks-1",
        "malloc-zero",
        &got,
        &(B1, vec![(1, false, 0)]),
    );
    let h = plain(Spy::new().blocks(&[B1]));
    let v = at(h.operator_new(0));
    let got = (v, h.memory().asks());
    row(
        out,
        "ME1",
        "new-0-asks-1",
        "malloc-zero",
        &got,
        &(B1, vec![(1, false, 0)]),
    );
    let h = plain(Spy::new().blocks(&[B1]));
    let v = at(h.malloc(LARGEST));
    let got = (v, h.memory().asks());
    row(
        out,
        "ME1",
        "malloc-largest-asks-it",
        "largest",
        &got,
        &(B1, vec![(LARGEST, false, 0)]),
    );
    let h = plain(Spy::new().blocks(&[B1]));
    let v = at(h.malloc(LARGEST + 1));
    let got = (v, h.memory().asks());
    let none: Vec<(u32, bool, i32)> = Vec::new();
    row(
        out,
        "ME1",
        "malloc-past-largest-asks-nothing",
        "largest",
        &got,
        &(0, none.clone()),
    );
    for (name, count, size, bytes) in [
        ("calloc-0-by-n-asks-1", 0, 7, 1),
        ("calloc-n-by-0-asks-1", 7, 0, 1),
        ("calloc-4-by-4-asks-16", 4, 4, 16),
        ("calloc-largest-asks-it", 1, LARGEST, LARGEST),
    ] {
        let h = plain(Spy::new().blocks(&[B1]));
        let v = at(h.calloc(count, size));
        let got = (v, h.memory().asks());
        row(
            out,
            "ME1",
            name,
            "calloc-zeroed",
            &got,
            &(B1, vec![(bytes, true, 0)]),
        );
    }
    let h = plain(Spy::new().blocks(&[B1]));
    let v = at(h.calloc(1, LARGEST + 1));
    let got = (v, h.memory().asks());
    row(
        out,
        "ME1",
        "calloc-past-largest-asks-nothing",
        "largest",
        &got,
        &(0, none),
    );
}

/// realloc of null is malloc's; of size 0 it frees; a refusal keeps the block.
fn me1_realloc(out: &mut Vec<Line>) {
    let none: Vec<(u32, bool, i32)> = Vec::new();
    let no_resize: Vec<(u32, u32)> = Vec::new();
    let no_release: Vec<(u32, i32)> = Vec::new();
    let h = plain(Spy::new().blocks(&[B1]));
    let v = realloc(&h, 0, 8);
    let got = (v, h.memory().asks(), h.memory().resizes.borrow().clone());
    let want = (B1, vec![(8, false, 0)], no_resize.clone());
    row(
        out,
        "ME1",
        "realloc-null-n-is-malloc",
        "realloc-null",
        &got,
        &want,
    );
    let h = plain(Spy::new().blocks(&[B1]));
    let v = realloc(&h, 0, 0);
    let got = (v, h.memory().asks(), h.memory().releases.borrow().clone());
    let want = (B1, vec![(1, false, 0)], no_release.clone());
    row(
        out,
        "ME1",
        "realloc-null-0-asks-1",
        "realloc-null",
        &got,
        &want,
    );
    let h = plain(Spy::new().moved(&[B2]));
    let v = realloc(&h, B1, 0);
    let got = (
        v,
        h.memory().releases.borrow().clone(),
        h.memory().resizes.borrow().clone(),
    );
    let want = (0, vec![(B1, 0)], no_resize.clone());
    row(
        out,
        "ME1",
        "realloc-p-0-releases-no-resize",
        "realloc-zero",
        &got,
        &want,
    );
    let h = plain(Spy::new().moved(&[B2]));
    let v = realloc(&h, B1, 9);
    let got = (
        v,
        h.memory().resizes.borrow().clone(),
        h.memory().releases.borrow().clone(),
    );
    let want = (B2, vec![(B1, 9)], no_release.clone());
    row(
        out,
        "ME1",
        "realloc-p-n-resizes",
        "realloc-refused",
        &got,
        &want,
    );
    let h = plain(Spy::new());
    let v = realloc(&h, B1, 9);
    let got = (v, h.memory().releases.borrow().clone());
    row(
        out,
        "ME1",
        "realloc-refused-keeps-the-block",
        "realloc-refused",
        &got,
        &(0, no_release),
    );
    let h = plain(Spy::new().moved(&[B2]));
    let v = realloc(&h, B1, LARGEST);
    let got = (v, h.memory().resizes.borrow().clone());
    row(
        out,
        "ME1",
        "realloc-largest-asks-it",
        "largest",
        &got,
        &(B2, vec![(B1, LARGEST)]),
    );
    let h = plain(Spy::new().moved(&[B2]));
    let v = realloc(&h, B1, LARGEST + 1);
    let got = (v, h.memory().resizes.borrow().clone(), h.memory().asks());
    row(
        out,
        "ME1",
        "realloc-past-largest-asks-nothing",
        "largest",
        &got,
        &(0, no_resize, none),
    );
}

/// free and operator delete of null release nothing; each door writes one line.
fn me1_free(out: &mut Vec<Line>) {
    let no_release: Vec<(u32, i32)> = Vec::new();
    let h = plain(Spy::new());
    free(&h, 0);
    let got = h.memory().releases.borrow().clone();
    row(
        out,
        "ME1",
        "free-null-releases-nothing",
        "free-null",
        &got,
        &no_release,
    );
    let h = plain(Spy::new());
    delete(&h, 0);
    let got = h.memory().releases.borrow().clone();
    row(
        out,
        "ME1",
        "delete-null-releases-nothing",
        "free-null",
        &got,
        &no_release,
    );
    let h = plain(Spy::new());
    free(&h, B1);
    let got = h.memory().releases.borrow().clone();
    row(
        out,
        "ME1",
        "free-p-releases-it",
        "free-null",
        &got,
        &vec![(B1, 0)],
    );
    let h = plain(Spy::new());
    delete(&h, B1);
    let got = h.memory().releases.borrow().clone();
    row(
        out,
        "ME1",
        "delete-p-releases-it",
        "free-null",
        &got,
        &vec![(B1, 0)],
    );
    let quiets = |run: &dyn Fn(&Heap<Spy>)| {
        let h = plain(Spy::new().blocks(&[B1]).moved(&[B2]));
        run(&h);
        h.memory().quiets.get()
    };
    let got = [
        quiets(&|h| {
            let _ = h.malloc(8);
        }),
        quiets(&|h| {
            let _ = h.operator_new(8);
        }),
        quiets(&|h| free(h, B1)),
        quiets(&|h| delete(h, B1)),
        quiets(&|h| free(h, 0)),
        quiets(&|h| {
            let _ = h.calloc(4, 4);
        }),
        quiets(&|h| {
            let _ = realloc(h, B1, 9);
        }),
    ];
    row(out, "ME1", "quiet-once-a-call", "quiet-once", &got, &[1; 7]);
}

/// Yes retries, no fails, each door by its own reading of `_newmode`.
#[allow(
    clippy::too_many_lines,
    reason = "one list of rows over one shared setup"
)]
fn me2_doors(out: &mut Vec<Line>) {
    let retries = [0, 0, B1];
    let yes2 = [true, true];
    let h = plain(Spy::new().moded().blocks(&retries).says(&yes2));
    let v = at(h.malloc(8));
    let got = (v, h.memory().asks().len(), h.memory().sizes_handled());
    row(
        out,
        "ME2",
        "malloc-yes-twice-then-a-block",
        "retry",
        &got,
        &(B1, 3, vec![8, 8]),
    );
    let h = plain(Spy::new().moded());
    let v = at(h.malloc(8));
    let got = (v, h.memory().asks().len(), h.memory().sizes_handled());
    row(
        out,
        "ME2",
        "malloc-no-fails",
        "retry",
        &got,
        &(0, 1, vec![8]),
    );
    let h = plain(Spy::new().says(&yes2));
    let v = at(h.malloc(8));
    let none: Vec<u32> = Vec::new();
    let got = (v, h.memory().asks().len(), h.memory().sizes_handled());
    row(
        out,
        "ME2",
        "malloc-mode-off-no-handler",
        "mode-default",
        &got,
        &(0, 1, none.clone()),
    );
    let h = plain(Spy::new().blocks(&retries).says(&yes2));
    let v = at(h.operator_new(8));
    let got = (v, h.memory().asks().len(), h.memory().sizes_handled());
    row(
        out,
        "ME2",
        "new-mode-off-keeps-its-handler",
        "new-always",
        &got,
        &(B1, 3, vec![8, 8]),
    );
    let h = plain(Spy::new());
    let v = at(h.operator_new(8));
    let got = (v, h.memory().asks().len(), h.memory().sizes_handled());
    row(out, "ME2", "new-no-fails", "retry", &got, &(0, 1, vec![8]));
    let h = plain(Spy::new().moded().blocks(&retries).says(&yes2));
    let v = at(h.calloc(4, 4));
    let got = (v, h.memory().asks().len(), h.memory().sizes_handled());
    row(
        out,
        "ME2",
        "calloc-yes-twice-then-a-block",
        "retry",
        &got,
        &(B1, 3, vec![16, 16]),
    );
    let h = plain(Spy::new().says(&yes2));
    let v = at(h.calloc(4, 4));
    let got = (v, h.memory().asks().len(), h.memory().sizes_handled());
    row(
        out,
        "ME2",
        "calloc-mode-off-no-handler",
        "mode-default",
        &got,
        &(0, 1, none.clone()),
    );
    let h = plain(Spy::new().moded().moved(&retries).says(&yes2));
    let v = realloc(&h, B2, 9);
    let got = (
        v,
        h.memory().resizes.borrow().len(),
        h.memory().sizes_handled(),
    );
    row(
        out,
        "ME2",
        "realloc-yes-twice-then-a-block",
        "retry",
        &got,
        &(B1, 3, vec![9, 9]),
    );
    let h = plain(Spy::new().says(&yes2));
    let v = realloc(&h, B2, 9);
    let got = (
        v,
        h.memory().resizes.borrow().len(),
        h.memory().sizes_handled(),
    );
    row(
        out,
        "ME2",
        "realloc-mode-off-no-handler",
        "mode-default",
        &got,
        &(0, 1, none),
    );
    // One size above the largest, five doors: told side by side.
    let door = |run: &dyn Fn(&Heap<Spy>)| {
        let h = plain(Spy::new().moded());
        run(&h);
        h.memory().sizes_handled().len()
    };
    let past = LARGEST + 1;
    let got = [
        door(&|h| {
            let _ = h.malloc(past);
        }),
        door(&|h| {
            let _ = h.operator_new(past);
        }),
        door(&|h| {
            let _ = h.calloc(1, past);
        }),
        door(&|h| {
            let _ = realloc(h, B1, past);
        }),
        door(&|h| {
            let _ = realloc(h, 0, past);
        }),
    ];
    row(
        out,
        "ME2",
        "past-largest-handler-by-door",
        "doors",
        &got,
        &[0, 0, 1, 1, 0],
    );
}

/// The size a handler sees, and when `_newmode` is read.
#[allow(
    clippy::too_many_lines,
    reason = "one list of rows over one shared setup"
)]
fn me2_mode(out: &mut Vec<Line>) {
    let seen = |run: &dyn Fn(&Heap<Spy>)| {
        let h = plain(Spy::new().moded());
        run(&h);
        h.memory().sizes_handled()
    };
    let got = [
        seen(&|h| {
            let _ = h.malloc(0);
        }),
        seen(&|h| {
            let _ = h.operator_new(0);
        }),
        seen(&|h| {
            let _ = h.calloc(0, 4);
        }),
        seen(&|h| {
            let _ = h.calloc(4, 4);
        }),
        seen(&|h| {
            let _ = realloc(h, B1, 9);
        }),
    ];
    let want = [vec![1], vec![1], vec![1], vec![16], vec![9]];
    row(
        out,
        "ME2",
        "the-size-the-handler-sees",
        "handler-size",
        &got,
        &want,
    );
    let h = debug(Spy::new().moded());
    let _ = h.malloc(0);
    let _ = h.malloc(8);
    let got = h.memory().sizes_handled();
    row(
        out,
        "ME2",
        "debug-handler-sees-the-users-size",
        "handler-size",
        &got,
        &vec![0, 8],
    );
    // The handler turns `_newmode` off between tries.
    let scene = || {
        Spy::new()
            .moded()
            .blocks(&[0, 0, B1])
            .moved(&[0, 0, B1])
            .says(&[true, true])
            .flips(&[false, false])
    };
    let h = plain(scene());
    let v = at(h.malloc(8));
    let got = (
        v,
        h.memory().sizes_handled().len(),
        h.memory().mode_reads.get(),
    );
    row(
        out,
        "ME2",
        "flip-malloc-reads-once",
        "mode-read-once",
        &got,
        &(B1, 2, 1),
    );
    let h = debug(scene());
    let v = at(h.malloc(8));
    let got = (
        v,
        h.memory().sizes_handled().len(),
        h.memory().mode_reads.get(),
    );
    row(
        out,
        "ME2",
        "flip-debug-malloc-reads-once",
        "mode-read-once",
        &got,
        &(B1 + USER, 2, 1),
    );
    let h = plain(scene());
    let v = at(h.calloc(4, 4));
    let got = (
        v,
        h.memory().sizes_handled().len(),
        h.memory().mode_reads.get(),
    );
    row(
        out,
        "ME2",
        "flip-calloc-reads-each-time",
        "mode-read-each",
        &got,
        &(0, 1, 2),
    );
    let h = plain(scene());
    let v = realloc(&h, B2, 9);
    let got = (
        v,
        h.memory().sizes_handled().len(),
        h.memory().mode_reads.get(),
    );
    row(
        out,
        "ME2",
        "flip-realloc-reads-each-time",
        "mode-read-each",
        &got,
        &(0, 1, 2),
    );
    let h = plain(Spy::new());
    let _ = h.malloc(LARGEST + 1);
    row(
        out,
        "ME2",
        "malloc-reads-mode-before-refusing",
        "mode-read-once",
        &h.memory().mode_reads.get(),
        &1,
    );
}

/// The header's words and guards, the ask, the sizes.
fn me3_header(out: &mut Vec<Line>) {
    let h = debug(Spy::new().blocks(&[B1, B2]));
    let v = at(h.malloc(16));
    let want = vec![0, 0, 0, 0, 16, 1, 0, GUARD, GUARD];
    row(
        out,
        "ME3",
        "header-words-and-both-guards",
        "header-table",
        &h.memory().header(B1, 16),
        &want,
    );
    row(
        out,
        "ME3",
        "user-pointer-past-the-header",
        "asks",
        &v,
        &(B1 + USER),
    );
    row(
        out,
        "ME3",
        "ask-is-the-size-plus-0x24",
        "asks",
        &h.memory().asks(),
        &vec![(16 + 0x24, false, 1)],
    );
    let _ = h.malloc(4);
    let want = vec![B1, 0, 0, 0, 4, 1, 0, GUARD, GUARD];
    row(
        out,
        "ME3",
        "a-second-block-links-the-first",
        "header-table",
        &h.memory().header(B2, 4),
        &want,
    );
    row(
        out,
        "ME3",
        "the-first-block-links-back",
        "header-table",
        &h.memory().word(B1 + 4),
        &B2,
    );
    let h = debug(Spy::new().blocks(&[B1]));
    let _ = h.malloc(0);
    let got = (h.memory().asks(), h.memory().header(B1, 0));
    let want = (
        vec![(0x24, false, 1)],
        vec![0, 0, 0, 0, 0, 1, 0, GUARD, GUARD],
    );
    row(
        out,
        "ME3",
        "debug-malloc-0-is-not-made-1",
        "zero-not-one",
        &got,
        &want,
    );
    let none: Vec<(u32, bool, i32)> = Vec::new();
    let h = debug(Spy::new());
    let _ = h.malloc(0xffff_ffbc);
    row(
        out,
        "ME3",
        "debug-largest-size-asks",
        "debug-sizes",
        &h.memory().asks(),
        &vec![(LARGEST, false, 1)],
    );
    let h = debug(Spy::new());
    let _ = h.malloc(0xffff_ffbd);
    row(
        out,
        "ME3",
        "debug-past-largest-asks-nothing",
        "debug-sizes",
        &h.memory().asks(),
        &none,
    );
    for (name, size) in [
        ("debug-wrap-ffffffdc-asks-nothing", 0xffff_ffdc),
        ("debug-wrap-ffffffde-asks-nothing", 0xffff_ffde),
        ("debug-wrap-ffffffe0-asks-nothing", 0xffff_ffe0),
    ] {
        let h = debug(Spy::new());
        let v = at(h.malloc(size));
        row(
            out,
            "ME3",
            name,
            "debug-wrap",
            &(v, h.memory().asks()),
            &(0, none.clone()),
        );
    }
}

/// The lock balanced, given back before the handler, and around every word.
fn me3_lock(out: &mut Vec<Line>) {
    let state = |spy: &Spy| {
        (
            spy.held.get(),
            spy.deepest.get(),
            spy.locks.get(),
            spy.unlocked_touches.get(),
        )
    };
    let h = debug(Spy::new().blocks(&[B1]));
    let _ = h.malloc(8);
    row(
        out,
        "ME3",
        "lock-balanced-over-a-make",
        "lock",
        &state(h.memory()),
        &(0, 1, 1, 0),
    );
    debug_free(&h, B1 + USER);
    row(
        out,
        "ME3",
        "lock-balanced-over-a-free",
        "lock",
        &state(h.memory()),
        &(0, 1, 2, 0),
    );
    row(
        out,
        "ME3",
        "free-releases-the-header-under-the-lock",
        "lock",
        &h.memory().releases.borrow().clone(),
        &vec![(B1, 1)],
    );
    let h = debug(Spy::new().moded().blocks(&[0, B1]).says(&[true]));
    let v = at(h.malloc(8));
    let got = (
        v,
        h.memory().handled.borrow().clone(),
        h.memory().asks(),
        state(h.memory()),
    );
    let asks = vec![(8 + 0x24, false, 1), (8 + 0x24, false, 1)];
    row(
        out,
        "ME3",
        "lock-given-back-before-the-handler",
        "lock-handler",
        &got,
        &(B1 + USER, vec![(8, 0)], asks, (0, 1, 2, 0)),
    );
    let h = debug(Spy::new().moded());
    let v = at(h.malloc(8));
    let got = (v, h.memory().sizes_handled());
    row(out, "ME3", "debug-no-fails", "retry", &got, &(0, vec![8]));
    let h = debug(Spy::new().says(&[true]));
    let none: Vec<u32> = Vec::new();
    let v = at(h.malloc(8));
    let got = (v, h.memory().asks().len(), h.memory().sizes_handled());
    row(
        out,
        "ME3",
        "debug-mode-off-no-handler",
        "mode-default",
        &got,
        &(0, 1, none),
    );
    let h = debug(Spy::new());
    debug_free(&h, 0);
    let got = (h.memory().releases.borrow().len(), state(h.memory()));
    row(
        out,
        "ME3",
        "free-of-null-takes-the-lock-releases-nothing",
        "free-null-lock",
        &got,
        &(0, (0, 1, 1, 0)),
    );
    let h = debug(Spy::new().blocks(&[B1]));
    let a = at(h.malloc(8));
    debug_free(&h, a);
    let _ = h.malloc(8);
    row(
        out,
        "ME3",
        "quiet-once-a-call-debug",
        "quiet-once",
        &h.memory().quiets.get(),
        &3,
    );
}

/// Three blocks made; `freed` given back, by its header; both walks told.
fn after_freeing(freed: &[u32]) -> (Vec<u32>, Vec<u32>) {
    let h = debug(Spy::new().blocks(&[B1, B2, B3]));
    for _ in 0..3 {
        let _ = h.malloc(8);
    }
    for block in freed {
        debug_free(&h, block + USER);
    }
    h.memory().walks()
}

/// A list is walked both ways: a walk by next alone stays clean on a broken
/// previous link.
fn me3_list(out: &mut Vec<Line>) {
    let got = after_freeing(&[]);
    row(
        out,
        "ME3",
        "list-three-made",
        "list",
        &got,
        &(vec![B3, B2, B1], vec![B1, B2, B3]),
    );
    let got = after_freeing(&[B3]);
    row(
        out,
        "ME3",
        "list-free-the-newest",
        "list",
        &got,
        &(vec![B2, B1], vec![B1, B2]),
    );
    let got = after_freeing(&[B1]);
    row(
        out,
        "ME3",
        "list-free-the-oldest",
        "list",
        &got,
        &(vec![B3, B2], vec![B2, B3]),
    );
    let got = after_freeing(&[B2]);
    row(
        out,
        "ME3",
        "list-free-a-middle-one",
        "list",
        &got,
        &(vec![B3, B1], vec![B1, B3]),
    );
    let got = after_freeing(&[B3, B2]);
    row(
        out,
        "ME3",
        "list-two-gone-one-left",
        "list",
        &got,
        &(vec![B1], vec![B1]),
    );
    let none: Vec<u32> = Vec::new();
    let got = after_freeing(&[B3, B2, B1]);
    row(
        out,
        "ME3",
        "list-free-the-only-one",
        "list",
        &got,
        &(none.clone(), none),
    );
    let h = debug(Spy::new().blocks(&[B1, B2]));
    let _ = h.malloc(8);
    debug_free(&h, B1 + USER);
    let _ = h.malloc(8);
    row(
        out,
        "ME3",
        "list-made-again-after-empty",
        "list",
        &h.memory().walks(),
        &(vec![B2], vec![B2]),
    );
}

/// Real bytes at 32 bits: the header as bytes, unaligned, a user's bytes kept.
#[cfg(target_pointer_width = "32")]
mod real {
    use super::{DebugHeap, List, Memory, ROW, Words, at, row};
    use std::cell::Cell;
    use std::fmt::Write;

    struct Arena {
        base: *mut u8,
        used: Cell<usize>,
        releases: Cell<u32>,
    }

    impl Memory for Arena {
        fn heap(&self) -> usize {
            0x50
        }
        fn alloc(&self, bytes: u32, _: bool) -> *mut u8 {
            let block = self.base.wrapping_add(self.used.get());
            self.used.set(self.used.get() + bytes as usize + 1);
            block
        }
        unsafe fn resize(&self, _: *mut u8, _: u32) -> *mut u8 {
            core::ptr::null_mut()
        }
        unsafe fn release(&self, _: *mut u8) {
            self.releases.set(self.releases.get() + 1);
        }
        fn new_mode(&self) -> bool {
            false
        }
        fn new_handler(&self, _: u32) -> bool {
            false
        }
    }

    impl Words for Arena {
        fn lock(&self) {}
        fn unlock(&self) {}
        unsafe fn read(&self, address: u32) -> u32 {
            // SAFETY: the contract's: a word of the arena, which may sit at any byte.
            unsafe {
                self.base
                    .with_addr(address as usize)
                    .cast::<u32>()
                    .read_unaligned()
            }
        }
        unsafe fn write(&self, address: u32, value: u32) {
            // SAFETY: as `read`.
            unsafe {
                self.base
                    .with_addr(address as usize)
                    .cast::<u32>()
                    .write_unaligned(value)
            }
        }
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().fold(String::new(), |mut text, byte| {
            let _ = write!(text, "{byte:02x}");
            text
        })
    }

    pub fn real_bytes(out: &mut Vec<super::Line>) {
        let base = Box::leak(vec![0_u8; 256].into_boxed_slice()).as_mut_ptr();
        let first = at(base);
        let list: &'static List = Box::leak(Box::new(List {
            first,
            last: first + 4,
            lock: 0,
            unlock: 0,
            number: 0,
        }));
        // The first block sits at an odd byte, so every word in it is unaligned.
        let arena = Arena {
            base,
            used: Cell::new(11),
            releases: Cell::new(0),
        };
        let h = DebugHeap::new(&ROW, list, arena);
        let older = h.malloc(5);
        let newer = h.malloc(3);
        // SAFETY: both blocks lie inside the leaked arena, and 0x20 before each is its header.
        unsafe {
            older.write_bytes(0x5a, 5);
            let header = std::slice::from_raw_parts(older.sub(0x20), 0x20);
            let want = format!("{:08x}", at(newer.sub(0x20)).swap_bytes());
            let got = (
                hex(header),
                hex(std::slice::from_raw_parts(older, 5)),
                hex(std::slice::from_raw_parts(older.add(5), 4)),
            );
            let want = (
                format!("00000000{want}00000000000000000500000001000000") + "00000000fdfdfdfd",
                "5a5a5a5a5a".to_string(),
                "fdfdfdfd".to_string(),
            );
            row(
                out,
                "ME3",
                "real-header-and-guards-as-bytes",
                "header-table",
                &got,
                &want,
            );
            h.free(older);
            let memory = h.memory();
            let ends = (
                memory.read(first) == at(newer.sub(0x20)),
                memory.read(first + 4) == at(newer.sub(0x20)),
            );
            row(
                out,
                "ME3",
                "real-list-after-freeing-the-oldest",
                "list",
                &ends,
                &(true, true),
            );
            h.free(newer);
            let got = (
                memory.read(first),
                memory.read(first + 4),
                memory.releases.get(),
            );
            row(
                out,
                "ME3",
                "real-list-empty-at-the-end",
                "list",
                &got,
                &(0, 0, 2),
            );
        }
    }
}

/// Every row, in order.
pub fn all() -> Vec<Line> {
    let mut out = Vec::new();
    me1_sizes(&mut out);
    me1_realloc(&mut out);
    me1_free(&mut out);
    me2_doors(&mut out);
    me2_mode(&mut out);
    me3_header(&mut out);
    me3_lock(&mut out);
    me3_list(&mut out);
    #[cfg(target_pointer_width = "32")]
    real::real_bytes(&mut out);
    out
}

/// Where the crate keeps a game's rule against today's documented one: each
/// row's want is the document's, its got the crate's, told and not fixed.
pub fn standing() -> Vec<Line> {
    let mut out = Vec::new();
    let none: Vec<(u32, bool, i32)> = Vec::new();
    let h = plain(Spy::new().blocks(&[B1]));
    let v = at(h.calloc(0x1_0000, 0x1_0000));
    let got = (v, h.memory().asks());
    row(
        &mut out,
        "ME1",
        "calloc-product-wraps-to-0",
        "calloc-wrap",
        &got,
        &(0, none.clone()),
    );
    let h = plain(Spy::new().blocks(&[B1]));
    let v = at(h.calloc(0x1_0001, 0x1_0000));
    let got = (v, h.memory().asks());
    row(
        &mut out,
        "ME1",
        "calloc-product-wraps-small",
        "calloc-wrap",
        &got,
        &(0, none),
    );
    let past = LARGEST + 1;
    let h = plain(Spy::new());
    let _ = h.operator_new(past);
    row(
        &mut out,
        "ME2",
        "new-past-largest-calls-its-handler",
        "new-always",
        &h.memory().sizes_handled(),
        &vec![past],
    );
    let h = plain(Spy::new().moded());
    let _ = h.malloc(past);
    row(
        &mut out,
        "ME2",
        "malloc-past-largest-under-mode-calls-it",
        "mode-on",
        &h.memory().sizes_handled(),
        &vec![past],
    );
    for line in &mut out {
        line.kind = "STANDING";
    }
    out
}
