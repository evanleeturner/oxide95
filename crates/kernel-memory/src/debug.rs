// SPDX-License-Identifier: MIT
//! The debug C library's memory rules: malloc and free
//! as a game file runs them, the 1998 debug build's. Each block
//! carries a header of 0x20 bytes before the user's bytes and a guard of 4
//! after them, and every live block hangs on one list; so a block made
//! here and freed by the unswapped C library's own free, or made there and
//! freed here, finds the header and the list as that free reads them.
//!
//! The header, as the library's make writes it and its free reads it
//! each word at the block's start plus:
//!
//! | Offset | Word | Written here |
//! | --- | --- | --- |
//! | 0x00 | the next block's header, the older | the list's first end |
//! | 0x04 | the previous block's header | 0 |
//! | 0x08 | the caller's file name | 0, as `_malloc` passes it |
//! | 0x0c | the caller's line | 0, as `_malloc` passes it |
//! | 0x10 | the user's size | the size |
//! | 0x14 | the block's use | 1, a normal block |
//! | 0x18 | the request number | 0: left out |
//! | 0x1c | the guard before | `0xfdfdfdfd` |
//! | 0x20 | the user's bytes | not filled: left out |
//! | 0x20 + size | the guard after | `0xfdfdfdfd` |
//!
//! Carried, each for its reader in the unswapped free: the
//! links (it unlinks by them), the size (it finds the guard after by it),
//! the use (it asserts it), the guards (`_CheckBytes`). Left out, read by
//! nothing the game reaches, since the debug flag stays 1 all run and the
//! checks and dumps that would read them never run: the request number
//! and its counter, the break request, the totals, the fills `0xcd` and
//! `0xdd`, the allocation hook (it answers yes), and the free's own checks
//! and reports. Each address is the game's, kept in its rows crate.
//!
//! - **malloc** reads `_newmode` once, then under the heap lock asks the
//!   heap for the size plus 0x24. A size whose ask would pass
//!   `0xffffffe0` fails without asking; the original's add wraps for the
//!   sizes `0xffffffdc` to `0xffffffe0` and asks a few bytes, a 1998 bug
//!   not carried. The lock is given back before a failure calls the new
//!   handler, under `_newmode` alone, with the user's size; it tries again
//!   while the handler answers yes. A size of 0 is not made 1. The user's
//!   pointer is the header plus 0x20.
//! - **free** takes the lock; a null block does nothing; else it unlinks
//!   the header from the list and hands it back to the heap.
//!
//! In a browser the words are the arena's, the lock a no-op on one
//! thread; on Windows they are the game's own globals and its lock,
//! called at its address. Neither backend is built here.
use core::ptr::null_mut;

use crate::table::{List, Row};
use crate::{Heap, LARGEST, Memory};

/// The header before the user's bytes.
const HEADER: u32 = 0x20;
/// What a block asks beyond the user's size: the header and the guard after.
const OVERHEAD: u32 = HEADER + 4;
/// The guards' bytes, `0xfd` four times.
const GUARD: u32 = 0xfdfd_fdfd;
/// A normal block's use, `_NORMAL_BLOCK`.
const NORMAL: u32 = 1;

/// What the debug rules ask of a target beyond [`Memory`]: the C library's
/// heap lock, and 32-bit words at the game's own addresses.
pub trait Words: Memory {
    /// Takes the C library's heap lock, as its own make and free do.
    fn lock(&self);
    /// Gives the heap lock back.
    fn unlock(&self);
    /// The word at `address`, which may sit at any byte.
    ///
    /// # Safety
    ///
    /// `address` is one of the list's ends, or a word of a block this
    /// memory's heap made and has not released.
    #[allow(
        unsafe_code,
        reason = "an address is a bare number here, so its safety is the caller's promise"
    )]
    unsafe fn read(&self, address: u32) -> u32;
    /// Writes `value` at `address`, which may sit at any byte.
    ///
    /// # Safety
    ///
    /// As [`Words::read`].
    #[allow(
        unsafe_code,
        reason = "an address is a bare number here, so its safety is the caller's promise"
    )]
    unsafe fn write(&self, address: u32, value: u32);
}

/// One debug file's memory helpers: the plain [`Heap`] for its row, its
/// memory and its lines, and the file's list.
pub struct DebugHeap<M> {
    heap: Heap<M>,
    list: &'static List,
}

impl<M: Words> DebugHeap<M> {
    pub const fn new(file: &'static Row, list: &'static List, memory: M) -> Self {
        Self {
            heap: Heap::new(file, memory),
            list,
        }
    }

    /// The memory the helpers ask.
    pub const fn memory(&self) -> &M {
        self.heap.memory()
    }

    /// malloc: a block of `bytes` with its header and guards, the handler
    /// under `_newmode`.
    pub fn malloc(&self, bytes: u32) -> *mut u8 {
        let memory = self.memory();
        let handled = memory.new_mode();
        let block = loop {
            memory.lock();
            let block = self.make(bytes);
            memory.unlock();
            if !block.is_null() || !handled || !memory.new_handler(bytes) {
                break block;
            }
        };
        let name = self.heap.file.malloc.name;
        self.heap.line(name, [oxide_trace::__shown!(bytes)], block);
        block
    }

    /// free: the block unlinked and handed back; null does nothing.
    ///
    /// # Safety
    ///
    /// `block` is null, or a block this file's malloc made, here or in the
    /// unswapped C library, and not yet freed.
    #[allow(
        unsafe_code,
        reason = "the caller vouches for the block, whose header names the list's others"
    )]
    pub unsafe fn free(&self, block: *mut u8) {
        let memory = self.memory();
        memory.lock();
        if !block.is_null() {
            let header = block.wrapping_sub(HEADER as usize);
            let at = address(header);
            // SAFETY: the caller's promise: the header is this heap's, and
            // its links name the list's other headers, or 0 at an end.
            unsafe {
                let next = memory.read(at);
                let previous = memory.read(at + 4);
                memory.write(if next == 0 { self.list.last } else { next + 4 }, previous);
                memory.write(
                    if previous == 0 {
                        self.list.first
                    } else {
                        previous
                    },
                    next,
                );
                memory.release(header);
            }
        }
        memory.unlock();
        let name = self.heap.file.free.name;
        self.heap.line_void(name, [oxide_trace::__shown!(block)]);
    }

    /// The make, under the lock: a block asked of the heap, its header and
    /// guards written, put at the list's first end; null when the size or
    /// the heap refuses.
    #[allow(
        unsafe_code,
        reason = "the list's ends and the new block are words this heap reads and writes by address"
    )]
    fn make(&self, bytes: u32) -> *mut u8 {
        let Some(ask) = bytes.checked_add(OVERHEAD).filter(|&ask| ask <= LARGEST) else {
            return null_mut();
        };
        let memory = self.memory();
        let header = memory.alloc(ask, false);
        if header.is_null() {
            return header;
        }
        let at = address(header);
        let words = [
            (0, 0),
            (4, 0),
            (8, 0),
            (0xc, 0),
            (0x10, bytes),
            (0x14, NORMAL),
            (0x18, 0),
            (0x1c, GUARD),
            (HEADER + bytes, GUARD),
        ];
        // SAFETY: the list's ends are the row's; every other word is the
        // first end's header, or inside the `ask` bytes just made at `at`.
        unsafe {
            let first = memory.read(self.list.first);
            for (offset, word) in words {
                memory.write(at + offset, word);
            }
            memory.write(at, first);
            memory.write(
                if first == 0 {
                    self.list.last
                } else {
                    first + 4
                },
                at,
            );
            memory.write(self.list.first, at);
        }
        header.wrapping_add(HEADER as usize)
    }
}

/// A block's address as the game's 32-bit word.
#[allow(
    clippy::cast_possible_truncation,
    reason = "every target the game runs on, i686 and wasm32, has 32-bit pointers"
)]
fn address(block: *mut u8) -> u32 {
    block.addr() as u32
}
