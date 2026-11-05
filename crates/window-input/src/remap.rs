// SPDX-License-Identifier: MIT
//! The remap rows: a chord a program
//! reads that a browser keeps for itself never reaches a page, so the
//! page's back reads another chord in its place. Under winit and Wine every
//! chord passes and no row applies.
//!
//! The chords a browser keeps, [`BROWSER_KEPT`]: Ctrl with W, T or N (a tab
//! closed, a tab or a window opened) and F5 (a reload). A program's rows
//! crate names a row for each one its tables bind, with the chord pressed
//! in its place.
//!
//! The named gap: browsers keep Ctrl+Shift with W, T and N, Ctrl+Tab and
//! Ctrl+F4 too, which have no row here; the rewrite of the page's events by
//! the rows is the page's back's, not yet written.

/// A key and the two modifiers a remap reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chord {
    /// The virtual key.
    pub key: u8,
    /// Ctrl held.
    pub ctrl: bool,
    /// Alt held.
    pub alt: bool,
}

impl Chord {
    /// `key` with neither modifier.
    #[must_use]
    pub const fn key(key: u8) -> Self {
        Self {
            key,
            ctrl: false,
            alt: false,
        }
    }

    /// `key` with Ctrl.
    #[must_use]
    pub const fn ctrl(key: u8) -> Self {
        Self {
            ctrl: true,
            ..Self::key(key)
        }
    }

    /// `key` with Alt.
    #[must_use]
    pub const fn alt(key: u8) -> Self {
        Self {
            alt: true,
            ..Self::key(key)
        }
    }
}

/// One remap row: the program reads `kept`, which a browser never hands a
/// page; the page reads `pressed` in its place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Remap {
    pub kept: Chord,
    pub pressed: Chord,
}

/// The chords a browser keeps from a page: Ctrl+W, Ctrl+T, Ctrl+N and F5.
pub const BROWSER_KEPT: &[Chord] = &[
    Chord::ctrl(b'W'),
    Chord::ctrl(b'T'),
    Chord::ctrl(b'N'),
    Chord::key(0x74),
];

/// The chord the program reads for the page's `pressed` under `rows`: the
/// kept chord of the row pressed so, or `pressed` itself.
#[must_use]
pub fn read(rows: &[Remap], pressed: Chord) -> Chord {
    rows.iter()
        .find(|row| row.pressed == pressed)
        .map_or(pressed, |row| row.kept)
}
