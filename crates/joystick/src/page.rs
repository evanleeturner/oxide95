// SPDX-License-Identifier: MIT
//! The page's pads: everything handed in. The browser offers `getGamepads` to a page
//! and not to a Worker, and a game runs in a Worker, so `page/gamepads.mjs` reads the
//! pads once a frame and whoever owns the Worker passes its numbers to
//! [`PagePads::hand_in`]. Nothing here names a target or blocks.
//!
//! The count never goes down: it is the highest slot ever seen, plus 1, so a program
//! that counted once keeps trying a pulled pad's slot and finds it again when it comes
//! back. It is 0 only while no pad has been seen. A browser hides a pad until a button
//! is pressed on it, so a program that counts once at its start, before that, sees none.
use core::cell::Cell;

use crate::pads::{Pads, Reading, Shape, Span, has};

/// How many pads one hand-in carries.
pub const SLOTS: usize = 4;
/// Each pad's words, in order: present, X, Y, Z, R, U, V from 0 to 65535, `wCaps`, how
/// many buttons, one bit a button, `dwPOV`. An empty slot is all 0.
pub const WORDS_A_PAD: usize = 11;

const PRESENT: usize = 0;
const AXES: usize = 1;
const AXES_END: usize = AXES + 6;
const CAPS: usize = 7;
const BUTTON_COUNT: usize = 8;
const BUTTONS: usize = 9;
const POV: usize = 10;

const EMPTY: [[u32; WORDS_A_PAD]; SLOTS] = [[0; WORDS_A_PAD]; SLOTS];

/// The pads a page's owner feeds.
pub struct PagePads {
    slots: Cell<[[u32; WORDS_A_PAD]; SLOTS]>,
    seen: Cell<u32>,
    /// A bad hand-in has been reported, and none has come good since: a script gone wrong
    /// would otherwise write a line a frame.
    reported: Cell<bool>,
}

impl PagePads {
    /// No pad seen.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            slots: Cell::new(EMPTY),
            seen: Cell::new(0),
            reported: Cell::new(false),
        }
    }

    /// One frame's numbers: whole pads, up to [`SLOTS`], from the first slot. Anything else
    /// is 0 pads and one line, until a good frame.
    pub fn hand_in(&self, words: &[u32]) {
        if words.len() % WORDS_A_PAD != 0 || words.len() > SLOTS * WORDS_A_PAD {
            self.slots.set(EMPTY);
            if !self.reported.replace(true) {
                let count = words.len();
                log::warn!(n:% = oxide_trace::__shown!(count), by = "joystick"; "joy.page.words");
            }
            return;
        }
        self.reported.set(false);
        let mut slots = EMPTY;
        for (slot, pad) in slots.iter_mut().zip(words.chunks_exact(WORDS_A_PAD)) {
            slot.copy_from_slice(pad);
        }
        for (count, slot) in (1..).zip(&slots) {
            if slot[PRESENT] != 0 && count > self.seen.get() {
                self.seen.set(count);
            }
        }
        self.slots.set(slots);
    }

    fn slot(&self, device: u32) -> Option<[u32; WORDS_A_PAD]> {
        let index = usize::try_from(device).ok()?;
        self.slots
            .get()
            .get(index)
            .copied()
            .filter(|slot| slot[PRESENT] != 0)
    }
}

impl Default for PagePads {
    fn default() -> Self {
        Self::new()
    }
}

impl Pads for PagePads {
    fn devices(&self) -> u32 {
        self.seen.get()
    }

    fn shape(&self, device: u32) -> Option<Shape> {
        let slot = self.slot(device)?;
        let extra_axes = slot[CAPS] & (has::Z | has::R | has::U | has::V);
        Some(Shape {
            spans: [Span::FULL; 6],
            caps: slot[CAPS],
            axes: 2 + extra_axes.count_ones(),
            buttons: slot[BUTTON_COUNT],
        })
    }

    fn reading(&self, device: u32) -> Option<Reading> {
        let slot = self.slot(device)?;
        let mut axes = [0; 6];
        axes.copy_from_slice(&slot[AXES..AXES_END]);
        Some(Reading {
            axes,
            buttons: slot[BUTTONS],
            pressed: slot[BUTTONS].count_ones(),
            pov: slot[POV],
        })
    }
}
