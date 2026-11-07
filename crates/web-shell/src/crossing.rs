// SPDX-License-Identifier: MIT
//! The crossing: the 32-bit words the page's thread and the blocked Worker share.
//!
//! The wasm's own memory stays private, so these words live in a separate
//! `SharedArrayBuffer`, reached one call at a time through [`Words`]. A key's
//! edges go through a ring rather than a table of what is down, because
//! `Page::hand` takes events in order and a table would lose a short tap. The
//! page writes the head; the Worker writes the tail.
//!
//! `page/crossing.mjs` holds the same numbers for the page; a test here reads that
//! file, so the two cannot part.

/// Counted by the page's own timer, to prove the page's thread keeps running.
pub const PAGE_TICKS: u32 = 0;
/// The Worker's turns.
pub const POLLS: u32 = 1;
/// The turns that read the held key down.
pub const POLLS_DOWN: u32 = 2;
/// The pictures handed to the page.
pub const FRAMES: u32 = 3;
/// The disc's state, one of the `DISC_*` values.
pub const DISC: u32 = 4;
/// Set by the page to ask for the file to be read; cleared by the Worker.
pub const READ_AGAIN: u32 = 5;
/// The next ring slot the page writes, counted without wrapping.
pub const HEAD: u32 = 6;
/// The next ring slot the Worker reads, counted without wrapping.
pub const TAIL: u32 = 7;
/// The key edges that reached `Page::hand`.
pub const EDGES: u32 = 8;
/// The last edge's key, as `window_input`'s front answers it: [`Edge::word`].
pub const LAST_KEY: u32 = 9;
/// Set by a timer the Worker starts before its loop, so only a yield can fire it.
pub const YIELDED: u32 = 10;
/// Edges the page refused because the ring was full.
pub const LOST: u32 = 11;
/// The word the Worker sits on; nothing ever notifies it.
pub const SIT: u32 = 12;
/// The mark's left edge in the last picture, for a reader of the crossing.
pub const MARK_LEFT: u32 = 13;
/// Not zero while the page holds the mark still: pictures go on, a key still lifts it.
pub const STILL: u32 = 14;
/// The first of the [`MARKERS`] bytes read from the file.
pub const MARKER_AT: u32 = 16;
/// How many marker bytes the file read returns.
pub const MARKERS: u32 = 5;
/// The ring's first slot.
pub const RING: u32 = 32;
/// The ring's slots.
pub const RING_SLOTS: u32 = 256;
/// The words in all.
pub const WORDS: u32 = RING + RING_SLOTS;

/// No file was picked.
pub const DISC_NONE: i32 = 0;
/// The file read right, its markers in the `MARKER_AT` words.
pub const DISC_READ: i32 = 1;
/// The file would not read: moved, renamed or changed since it was picked.
pub const DISC_MOVED: i32 = 2;
/// The file read, but it is not a disc image this reader takes.
pub const DISC_REFUSED: i32 = 3;

/// The shared words, however a target reaches them.
pub trait Words {
    fn load(&self, word: u32) -> i32;
    fn store(&self, word: u32, value: i32);

    /// Adds 1. Only one thread writes each counting word, so no atomic add is needed.
    fn count(&self, word: u32) {
        self.store(word, self.load(word).wrapping_add(1));
    }
}

/// One key going down or up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Edge {
    /// The virtual key number.
    pub key: u8,
    pub down: bool,
}

impl Edge {
    /// The edge as one ring slot: the key, and bit 8 set for down.
    #[must_use]
    pub const fn word(self) -> i32 {
        self.key as i32 | ((self.down as i32) << 8)
    }

    /// The edge a ring slot holds; bits above the ninth are ignored.
    #[must_use]
    pub const fn from_word(word: i32) -> Self {
        Self {
            key: word.to_le_bytes()[0],
            down: word & 0x100 != 0,
        }
    }
}

/// A number's bits as a signed number, two's complement: `cast_signed` is
/// newer than the crate's Rust.
pub(crate) const fn signed(value: u32) -> i32 {
    i32::from_ne_bytes(value.to_ne_bytes())
}

/// A signed number's bits as a number, so a word past `i32::MAX` still picks a slot.
pub(crate) const fn unsigned(value: i32) -> u32 {
    u32::from_ne_bytes(value.to_ne_bytes())
}

/// The ring's oldest edge, taken out; `None` when the Worker has caught up.
pub fn next_edge(words: &impl Words) -> Option<Edge> {
    let tail = words.load(TAIL);
    if tail == words.load(HEAD) {
        return None;
    }
    let edge = Edge::from_word(words.load(RING + unsigned(tail) % RING_SLOTS));
    words.store(TAIL, tail.wrapping_add(1));
    Some(edge)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use core::cell::RefCell;

    /// The shared words as a plain vector, for one thread's tests.
    pub(crate) struct Plain(pub RefCell<Vec<i32>>);

    impl Plain {
        pub(crate) fn new() -> Self {
            Self(RefCell::new(vec![0; WORDS as usize]))
        }

        /// As the page's `pushEdge` does, without its full-ring check.
        pub(crate) fn push(&self, edge: Edge) {
            let head = self.load(HEAD);
            self.store(RING + unsigned(head) % RING_SLOTS, edge.word());
            self.store(HEAD, head.wrapping_add(1));
        }
    }

    impl Words for Plain {
        fn load(&self, word: u32) -> i32 {
            self.0.borrow()[word as usize]
        }

        fn store(&self, word: u32, value: i32) {
            self.0.borrow_mut()[word as usize] = value;
        }
    }

    #[test]
    fn edges_come_out_in_the_order_they_went_in() {
        let words = Plain::new();
        let edges = [(0x41, true), (0x42, true), (0x41, false), (0x42, false)];
        for (key, down) in edges {
            words.push(Edge { key, down });
        }
        for (key, down) in edges {
            assert_eq!(next_edge(&words), Some(Edge { key, down }));
        }
        assert_eq!(next_edge(&words), None);
    }

    #[test]
    fn the_ring_wraps_past_its_last_slot_and_past_the_word_itself() {
        let words = Plain::new();
        for start in [signed(RING_SLOTS) - 1, i32::MAX] {
            words.store(HEAD, start);
            words.store(TAIL, start);
            for key in 1..=3 {
                words.push(Edge { key, down: true });
            }
            for key in 1..=3 {
                assert_eq!(next_edge(&words), Some(Edge { key, down: true }));
            }
            assert_eq!(next_edge(&words), None);
        }
    }

    #[test]
    fn an_edge_survives_its_word() {
        for key in [0, 0x41, 0xff] {
            for down in [false, true] {
                let edge = Edge { key, down };
                assert_eq!(Edge::from_word(edge.word()), edge);
            }
        }
    }

    /// The page's copy of the numbers: every word here, the same value there.
    #[test]
    fn the_pages_numbers_are_these() {
        let page = include_str!("../page/crossing.mjs");
        let numbers = [
            ("PAGE_TICKS", PAGE_TICKS),
            ("POLLS", POLLS),
            ("POLLS_DOWN", POLLS_DOWN),
            ("FRAMES", FRAMES),
            ("DISC", DISC),
            ("READ_AGAIN", READ_AGAIN),
            ("HEAD", HEAD),
            ("TAIL", TAIL),
            ("EDGES", EDGES),
            ("LAST_KEY", LAST_KEY),
            ("YIELDED", YIELDED),
            ("LOST", LOST),
            ("SIT", SIT),
            ("MARK_LEFT", MARK_LEFT),
            ("STILL", STILL),
            ("MARKER_AT", MARKER_AT),
            ("MARKERS", MARKERS),
            ("RING", RING),
            ("RING_SLOTS", RING_SLOTS),
            ("DISC_NONE", unsigned(DISC_NONE)),
            ("DISC_READ", unsigned(DISC_READ)),
            ("DISC_MOVED", unsigned(DISC_MOVED)),
            ("DISC_REFUSED", unsigned(DISC_REFUSED)),
        ];
        for (name, value) in numbers {
            let line = format!("export const {name} = {value};");
            assert!(page.contains(&line), "the page lacks `{line}`");
        }
    }
}
