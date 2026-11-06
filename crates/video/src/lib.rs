// SPDX-License-Identifier: MIT
//! The video stand-in: the 8 names a program asks of RAD's Smacker library, each
//! a front in [`front::Videos`] with one `imp.call` line, `by=video`, over
//! `video_smacker`'s decoder. RAD's library is not rebuilt: a front answers what
//! a caller reads back, and a clip's frames land in a buffer the caller named.
//!
//! - **Backs:** one trait each, so no front reads the OS: [`Clock`] the
//!   microseconds, moved by its owner; [`ClipBytes`] a clip's whole file, read at
//!   open; [`SoundDoor`] a clip's stream, whose real back is [`MixDoor`].
//! - **The handle:** [`Handle`], a block at fixed offsets that a caller reads
//!   through the pointer `smack_open` gave it.
//! - **`smack_wait` never waits:** it answers whether the next frame is due. The
//!   word stays so a log line reads against the import it stands for.
//! - **Sound:** a clip opened naming its track, over a door that plays, opens one
//!   stream, pushes each painted frame's samples at `smack_do_frame` and closes at
//!   `smack_close`. A frame stepped past is never heard. A door that plays nothing
//!   takes no call and every `smack_sound_in_track` answers 0, so a caller that
//!   asks plays its clip silent.
//! - **Two questions stay open, each one flag in [`Choices`]:** what
//!   `smack_goto(1)` names, and whether a void block reaches the target.
//! - **Refused by name:** a handle no clip holds, a `smack_goto` past the clip, a
//!   null buffer, a frame the decoder refuses: a `video.refused` line, nothing
//!   written, no cursor moved. A clip that cannot be read opens as null.
//! - **Walls:** a caller's own loop that spins needs a yield on a page, which no
//!   front here gives. A rewind decodes forward from the first frame: the format
//!   has no key frames.
//!
//! The crate names no game and nothing of any one target, so it builds for
//! `wasm32-unknown-unknown`; a game's facts live in its rows crate.
#![cfg_attr(not(test), no_std)]

extern crate alloc;

pub mod front;
mod handle;
mod mix_door;
#[cfg(test)]
mod tests;

use alloc::vec::Vec;

pub use front::{Place, Videos};
pub use handle::{Handle, PALETTE_BYTES};
pub use mix_door::MixDoor;

/// The clock a front reads: an interval on one clock, so no zone enters.
pub trait Clock {
    /// The clock's microseconds. A front never moves it.
    fn now_us(&self) -> u64;
}

/// Where a clip's bytes come from: read whole at open.
pub trait ClipBytes {
    /// The whole file `name`, as the caller spelled it; `None` for one it lacks.
    fn read(&mut self, name: &[u8]) -> Option<Vec<u8>>;
}

/// A clip's sound stream, as the mixer's door takes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Track {
    /// Samples a second, a channel.
    pub rate_hz: u32,
    /// 1 or 2; 2 interleaves left then right.
    pub channels: u8,
}

/// The sound door: one stream a clip that plays, each painted frame's samples
/// pushed at `smack_do_frame`.
pub trait SoundDoor {
    /// Whether a stream through here reaches anything. The default, `false`, is a
    /// door that plays nothing: no front calls it, and a caller asking whether a
    /// clip has sound is told 0.
    fn plays(&self) -> bool {
        false
    }
    /// Opens a clip's stream.
    fn stream_open(&mut self, track: Track) -> u32;
    /// Pushes one frame's interleaved samples.
    fn stream_push(&mut self, stream: u32, samples: &[i16]);
    /// Closes a clip's stream.
    fn stream_close(&mut self, stream: u32);
}

/// What `smack_goto`'s number counts from. Open until a caller's buffer is
/// compared against the original library's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GotoBase {
    /// 1 names the first frame. The default: the one caller read so far restarts
    /// its own counter at 1 past a clip's last frame, a reading, no measurement.
    #[default]
    FromOne,
    /// 0 names the first frame, as the decoder counts.
    FromZero,
}

impl GotoBase {
    /// The decoder's frame for the caller's number, if it names one.
    fn index(self, frame: u32) -> Option<u32> {
        match self {
            Self::FromOne => frame.checked_sub(1),
            Self::FromZero => Some(frame),
        }
    }
}

/// What a void block does to the target. Open until a caller's buffer is
/// compared against the original library's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VoidBlocks {
    /// Only painted blocks reach the target. The default: on a buffer only the
    /// clip writes it equals a direct decode, and it never paints over what a
    /// caller drew there.
    #[default]
    LeftAlone,
    /// Every frame writes its whole picture.
    Written,
}

/// The two open questions, each one flag.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Choices {
    /// What `smack_goto`'s number counts from.
    pub goto_base: GotoBase,
    /// What a void block does to the target.
    pub void_blocks: VoidBlocks,
}
