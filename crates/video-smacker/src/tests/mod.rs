// SPDX-License-Identifier: MIT
//! The crate's fence. Every clip here comes from the test-only writer, so no
//! test holds a game's bytes. `fixtures`' clips were also run through another
//! decoder as a program, the way the crate's choices were closed: the
//! expected values below are that decoder's answers, written down.
mod fixtures;
mod palette_runs;
mod pictures;
mod probes;
mod refusals;
mod sound;
mod test_log;
mod writer;
