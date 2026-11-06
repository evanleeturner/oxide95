// SPDX-License-Identifier: MIT
//! The `DirectSound` stand-in: the object and buffers a program reaches through
//! tables, written once over `sound-mix`'s doors. A front answers what a program
//! reads back of a call, its return and what it wrote, and sounds what its buffers
//! say.
//!
//! [`buffers::Buffers`] is the portable part, in a file's 32-bit numbers;
//! [`com::Com`] is the shape a program calls, the tables in `dsound.h`'s order;
//! [`table::Row`] is one file's facts, kept in its game's rows crate; [`sdk`] is
//! `dsound.h`'s numbers; [`mix_back::MixBack`] is the real way into a mixer.
//!
//! The 21 fronts are [`table::FRONTS`], each writing one `imp.call` line. Every
//! other slot of the 32 answers `DSERR_UNSUPPORTED` with one `sound.buffers.absent`
//! line, popping its own arguments, and is never null. Each of the 10 in
//! [`table::STOPS`] ends the program through [`buffers::Buffers::named_stop`], one
//! `sound.buffers.named.stop` line first, never a quiet answer.
//!
//! Chosen here and not measured against a real `DirectSound`: a natural end leaves
//! the position at 0; a duplicate starts still at its first frame; `DSBCAPS_CTRL*`
//! is not enforced, so a buffer without the flag still takes a volume, pan or
//! frequency; `GetStatus` never says lost and `Restore` answers 0; a `Lock` that
//! would wrap, with a null second pointer, gives the first span alone.
//!
//! Not answered yet: `DSBLOCK_FROMWRITECURSOR` is refused, as no caller was found;
//! `DSBCAPS_STATIC` and the hardware flags are not read; `DSBPLAY_*` past looping is
//! not read. A volume or pan set while playing reaches the frames not yet with the
//! door, so it is heard up to [`buffers::LEAD_US`] late.
//!
//! No call here has yet run in a real program: the fronts have run on a host's
//! tests. A 64-bit host's word from `Lock` is not a pointer, so a host writes through
//! [`buffers::Buffers::with_bytes`]. One thread's calls: a named wall, as
//! `draw-surfaces`.
#![cfg_attr(not(test), no_std)]

extern crate alloc;

pub mod buffers;
pub mod com;
pub mod mix_back;
pub mod sdk;
pub mod table;
#[cfg(test)]
mod test_log;
#[cfg(test)]
mod tests;
