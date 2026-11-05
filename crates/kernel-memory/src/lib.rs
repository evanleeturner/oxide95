// SPDX-License-Identifier: MIT
//! The C library's memory helpers: malloc, operator
//! new, free, operator delete, calloc and realloc, written once. Each file
//! that carries a copy is a [`table::Row`], kept in its game's rows crate;
//! where its blocks come from is a backend behind [`Memory`]; [`Heap`]
//! holds the C library's rules
//! and writes one `imp.call` line a call: `c` the row's name, `a` the
//! arguments, `h` the heap, `v` the value returned.
//!
//! The rules, as a game's copy runs them (its bodies read in Ghidra's
//! export):
//!
//! - **No small-block heap.** malloc asks the file's heap for every block
//!   itself, and free hands every block back to it: a block carries no
//!   header of the C library's, so one the unswapped C made frees here as
//!   it would there.
//! - **malloc** reads `_newmode`, then refuses a size above `0xffffffe0`
//!   at once, with no handler; a size of 0 becomes 1. A failed block
//!   calls the new handler when `_newmode` is set, and tries again while
//!   the handler answers yes. **operator new** is malloc with the handler
//!   always on.
//! - **calloc** multiplies, wrapping as 32 bits do; a product of 0
//!   becomes 1; a product above `0xffffffe0` fails without asking the
//!   heap. Its block is zeroed. Its failures, and realloc's, call the
//!   handler under `_newmode` as malloc's do, reading it each time.
//! - **realloc** of a null block is malloc's call; of size 0 it frees the
//!   block and returns null; else the heap moves the block, sizes above
//!   `0xffffffe0` failing without asking it. A size of 0 is not made 1.
//! - **free** of a null block does nothing; **operator delete** is free.
//!
//! Every block goes back to the heap that made it: one file, one heap,
//! one row, and a [`Heap`] serves one row alone.
//!
//! # The browser's backend, planned, not built
//!
//! A backend answers [`Memory`]. In a browser the blocks would come from
//! the module's own linear memory: its offsets are 32 bits wide, as the
//! game's pointers are. One arena a file, so a block still goes home;
//! blocks aligned to 8, as the Windows heap aligns them; a size header in
//! front of each, so `release` needs no size; `zeroed` blocks cleared;
//! `heap` the arena's base. A refused `memory.grow` returns null, so the
//! rules above run the handler as on Windows. Its `quiet` stays the bare
//! default: there is no thread's last error to keep. Its log sink is set
//! at the binary's root, as every sink is.
//!
//! The crate names nothing of any one target, so it builds for
//! `wasm32-unknown-unknown` today; the Windows heap is a game's own
//! backend.
#![cfg_attr(not(test), no_std)]

pub mod debug;
pub mod table;

use core::ptr::null_mut;
use oxide_trace::{Args, Shown};
use table::{Helper, Row};

/// The largest size the C library asks a heap for, `_HEAP_MAXREQ`.
const LARGEST: u32 = 0xffff_ffe0;

/// Where one file's blocks come from, and its C library's two settings
/// the rules read: the seam between the helpers and a target.
pub trait Memory {
    /// The heap, as the line's `h` shows it.
    fn heap(&self) -> usize;
    /// A new block of `bytes`, cleared when `zeroed`; null when refused.
    fn alloc(&self, bytes: u32, zeroed: bool) -> *mut u8;
    /// The block grown or shrunk to `bytes`, perhaps moved; null when
    /// refused, the old block then untouched.
    ///
    /// # Safety
    ///
    /// `block` came from this memory and is not yet released.
    unsafe fn resize(&self, block: *mut u8, bytes: u32) -> *mut u8;
    /// The block handed back.
    ///
    /// # Safety
    ///
    /// `block` came from this memory, is not null and is not yet released.
    unsafe fn release(&self, block: *mut u8);
    /// `_newmode`: whether a failed malloc calls the new handler.
    fn new_mode(&self) -> bool;
    /// The new handler, called after a failed block of `bytes`: true to
    /// try again, false to fail.
    fn new_handler(&self, bytes: u32) -> bool;
    /// Writes one line around whatever the target must keep across it.
    fn quiet(&self, line: impl FnOnce()) {
        line();
    }
}

/// One file's memory helpers: its row, and its memory.
pub struct Heap<M> {
    file: &'static Row,
    memory: M,
}

impl<M: Memory> Heap<M> {
    pub const fn new(file: &'static Row, memory: M) -> Self {
        Self { file, memory }
    }

    /// The memory the helpers ask.
    pub const fn memory(&self) -> &M {
        &self.memory
    }

    /// malloc: a block of `bytes`, the handler under `_newmode`.
    pub fn malloc(&self, bytes: u32) -> *mut u8 {
        let block = self.allocate(bytes, self.memory.new_mode());
        self.line(self.file.malloc.name, [oxide_trace::__shown!(bytes)], block);
        block
    }

    /// operator new: malloc with the handler always on.
    pub fn operator_new(&self, bytes: u32) -> *mut u8 {
        let block = self.allocate(bytes, true);
        self.line(
            name(self.file.operator_new, "operator new"),
            [oxide_trace::__shown!(bytes)],
            block,
        );
        block
    }

    /// free: the block back to its heap; null does nothing.
    ///
    /// # Safety
    ///
    /// `block` is null, or came from this file's heap and is not yet freed.
    pub unsafe fn free(&self, block: *mut u8) {
        // SAFETY: the caller's promise, null excluded here.
        unsafe { self.free_block(block) };
        self.line_void(self.file.free.name, [oxide_trace::__shown!(block)]);
    }

    /// operator delete: free.
    ///
    /// # Safety
    ///
    /// As [`Heap::free`].
    pub unsafe fn operator_delete(&self, block: *mut u8) {
        // SAFETY: the caller's promise, null excluded here.
        unsafe { self.free_block(block) };
        self.line_void(
            name(self.file.operator_delete, "operator delete"),
            [oxide_trace::__shown!(block)],
        );
    }

    /// calloc: `count` blocks of `size`, zeroed, as one.
    pub fn calloc(&self, count: u32, size: u32) -> *mut u8 {
        let bytes = match count.wrapping_mul(size) {
            0 => 1,
            bytes => bytes,
        };
        let block = loop {
            let block = if bytes <= LARGEST {
                self.memory.alloc(bytes, true)
            } else {
                null_mut()
            };
            if !block.is_null() || !self.handled(bytes) {
                break block;
            }
        };
        let arguments = [oxide_trace::__shown!(count), oxide_trace::__shown!(size)];
        self.line(name(self.file.calloc, "calloc"), arguments, block);
        block
    }

    /// realloc: the block grown or shrunk to `bytes`.
    ///
    /// # Safety
    ///
    /// `block` is null, or came from this file's heap and is not yet freed.
    pub unsafe fn realloc(&self, block: *mut u8, bytes: u32) -> *mut u8 {
        let moved = if block.is_null() {
            self.allocate(bytes, self.memory.new_mode())
        } else if bytes == 0 {
            // SAFETY: the caller's promise, null excluded above.
            unsafe { self.memory.release(block) };
            null_mut()
        } else {
            loop {
                let moved = if bytes <= LARGEST {
                    // SAFETY: the caller's promise, null excluded above.
                    unsafe { self.memory.resize(block, bytes) }
                } else {
                    null_mut()
                };
                if !moved.is_null() || !self.handled(bytes) {
                    break moved;
                }
            }
        };
        let arguments = [oxide_trace::__shown!(block), oxide_trace::__shown!(bytes)];
        self.line(name(self.file.realloc, "realloc"), arguments, moved);
        moved
    }

    /// The shared allocation of malloc and operator new, `handled` their
    /// reading of whether a failure calls the new handler.
    fn allocate(&self, bytes: u32, handled: bool) -> *mut u8 {
        if bytes > LARGEST {
            return null_mut();
        }
        let bytes = bytes.max(1);
        loop {
            let block = self.memory.alloc(bytes, false);
            if !block.is_null() || !handled || !self.memory.new_handler(bytes) {
                return block;
            }
        }
    }

    /// After a failure under calloc's and realloc's reading: try again?
    fn handled(&self, bytes: u32) -> bool {
        self.memory.new_mode() && self.memory.new_handler(bytes)
    }

    /// free's body: null does nothing.
    ///
    /// # Safety
    ///
    /// As [`Heap::free`].
    unsafe fn free_block(&self, block: *mut u8) {
        if !block.is_null() {
            // SAFETY: the caller's promise, null excluded here.
            unsafe { self.memory.release(block) };
        }
    }

    fn line<const N: usize>(&self, name: &'static str, arguments: [Shown<'_>; N], value: *mut u8) {
        let heap = self.memory.heap();
        self.memory.quiet(|| {
            log::debug!(
                c = name, a:% = Args(arguments), h:% = oxide_trace::__shown!(heap),
                v:% = oxide_trace::__shown!(value); "imp.call"
            );
        });
    }

    fn line_void<const N: usize>(&self, name: &'static str, arguments: [Shown<'_>; N]) {
        let heap = self.memory.heap();
        self.memory.quiet(|| {
            log::debug!(
                c = name, a:% = Args(arguments), h:% = oxide_trace::__shown!(heap); "imp.call"
            );
        });
    }
}

/// A row's helper's name, or the C library's own where the row has none.
fn name(helper: Option<Helper>, fallback: &'static str) -> &'static str {
    helper.map_or(fallback, |helper| helper.name)
}
