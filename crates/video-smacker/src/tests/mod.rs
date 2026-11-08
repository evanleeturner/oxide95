// SPDX-License-Identifier: MIT
//! The crate's fence. Every clip here comes from the test-only writer, so no
//! test holds a game's bytes. `fixtures`' clips were also run through another
//! decoder as a program, the way the crate's choices were closed: a picture
//! or a sound expected below is that decoder's answer, written down. Two forms
//! have no such answer and are refused by name, a choice the crate made: a
//! palette chunk ending before entry 255, which that decoder reads on past,
//! and all four trees absent, which it will not open.
mod fixtures;
mod palette_runs;
mod pictures;
mod probes;
mod refusal_cuts;
mod refusals;
mod sound;
mod test_log;
mod writer;
