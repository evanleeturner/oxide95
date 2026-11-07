// SPDX-License-Identifier: MIT
//! The `DirectDraw` rows: what one file asks of the stand-in, kept in its game's
//! rows crate. A number no row holds is refused, so a call no run measured
//! answers loudly instead of drawing a guess.

/// One file's `DirectDraw` facts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    pub file: &'static str,
    /// The `SetCooperativeLevel` flags it asks.
    pub levels: &'static [u32],
    /// The `SetDisplayMode` modes it asks: width, height, bits.
    pub modes: &'static [(u32, u32, u32)],
    /// What `SetDisplayMode` answers for a mode `modes` does not hold.
    pub refused: u32,
    /// The most back buffers a flip chain is given.
    pub chain: u32,
    /// The `CreatePalette` flags it asks.
    pub palette_flags: u32,
}
