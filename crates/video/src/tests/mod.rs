// SPDX-License-Identifier: MIT
//! The fronts on fakes, over clips built by `video_smacker`'s own test-only
//! writer: no game byte in this tree. A direct decode by that crate is what the
//! fronts' pixels answer to; `disc` runs the same check over clips named by a
//! list outside the tree.
#![allow(unsafe_code, reason = "a test reads the handle as a caller does")]

mod clips;
mod clock;
mod disc;
mod fakes;
mod fronts;
mod mix;
mod sound;
mod test_log;
#[allow(
    dead_code,
    reason = "another crate's writer; these tests use part of it"
)]
#[path = "../../../video-smacker/src/tests/writer.rs"]
mod writer;
