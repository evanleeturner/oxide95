// SPDX-License-Identifier: MIT
//! A tee over any back: it hashes what the mix hands over and forwards it unchanged, so
//! one type reads the mix's sound on every route, as text over `NullBack` and on the
//! real route over `Page`, `Native` or `WaveOutBack`.
//!
//! It reads the hand-off alone, never what the back then did: a frame dropped for want
//! of room never reaches it, and the mix counts that one. Its block line goes out
//! inside the mix's lock and outside `Host::quiet`, so it suits a bench, not a target
//! that must keep something across a line.
//!
//! Call: `let witness = Witness::new(back); let seen = witness.seen();`, hand `witness`
//! to `Front::new`, which owns it from there, then `seen.summary()` -> frames, blocks and
//! the hash, and `seen.log_summary()` at the log's end.

use alloc::sync::Arc;
use core::sync::atomic::{AtomicU64, Ordering};

use crate::output::Output;

/// The hash of nothing: where a stream's hash starts.
pub const HASH_START: u64 = 0xcbf2_9ce4_8422_2325;
const HASH_PRIME: u64 = 0x0000_0100_0000_01b3;

/// FNV-1a 64 over `bytes`, going on from `from`.
pub(crate) fn hash_bytes(from: u64, bytes: impl Iterator<Item = u8>) -> u64 {
    bytes.fold(from, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(HASH_PRIME)
    })
}

/// The hash of `samples`' little-endian bytes, going on from `from`. It reads a stream,
/// so one sound cut into other blocks has one hash. A check against damage, no defence
/// against anyone forging a match.
#[must_use]
pub fn hash_samples(from: u64, samples: &[i16]) -> u64 {
    hash_bytes(from, samples.iter().flat_map(|sample| sample.to_le_bytes()))
}

/// What the witness has seen handed to its back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Summary {
    /// Whole frames.
    pub frames: u64,
    /// Calls to the back's `write`. It moves with the clock's steps, so it is no fact
    /// about the sound.
    pub blocks: u64,
    /// [`hash_samples`] over all of them, from [`HASH_START`].
    pub hash: u64,
}

/// One writer, the mix; atomics so a witness over a `Send` back stays `Send`.
struct Tally {
    frames: AtomicU64,
    blocks: AtomicU64,
    hash: AtomicU64,
}

/// The holder's end of a [`Witness`]: clones read one tally.
#[derive(Clone)]
pub struct Seen {
    tally: Arc<Tally>,
}

impl Seen {
    #[must_use]
    pub fn summary(&self) -> Summary {
        Summary {
            frames: self.tally.frames.load(Ordering::Relaxed),
            blocks: self.tally.blocks.load(Ordering::Relaxed),
            hash: self.tally.hash.load(Ordering::Relaxed),
        }
    }

    /// The summary's line, for the end of a run's log.
    pub fn log_summary(&self) {
        let summary = self.summary();
        log::info!(
            frames:% = oxide_trace::__shown!(summary.frames),
            blocks:% = oxide_trace::__shown!(summary.blocks),
            hash:% = oxide_trace::__shown!(summary.hash),
            by = "sound-mix"; "sound.witness.summary"
        );
    }
}

/// The tee. It owns its back, and every answer is the back's own.
pub struct Witness<O> {
    back: O,
    seen: Seen,
}

impl<O: Output> Witness<O> {
    #[must_use]
    pub fn new(back: O) -> Self {
        let tally = Tally {
            frames: AtomicU64::new(0),
            blocks: AtomicU64::new(0),
            hash: AtomicU64::new(HASH_START),
        };
        Self {
            back,
            seen: Seen {
                tally: Arc::new(tally),
            },
        }
    }

    /// The handle that still reads the counts once a front owns this.
    #[must_use]
    pub fn seen(&self) -> Seen {
        self.seen.clone()
    }
}

impl<O: Output> Output for Witness<O> {
    fn open(&mut self, rate_hz: u32) -> u32 {
        self.back.open(rate_hz)
    }

    fn room(&self) -> usize {
        self.back.room()
    }

    fn write(&mut self, frames: &[i16]) {
        let tally = &self.seen.tally;
        let hash = hash_samples(tally.hash.load(Ordering::Relaxed), frames);
        tally.hash.store(hash, Ordering::Relaxed);
        let whole = (frames.len() / 2) as u64;
        tally.frames.fetch_add(whole, Ordering::Relaxed);
        tally.blocks.fetch_add(1, Ordering::Relaxed);
        log::debug!(
            frames:% = oxide_trace::__shown!(whole),
            hash:% = oxide_trace::__shown!(hash),
            by = "sound-mix"; "sound.witness.block"
        );
        self.back.write(frames);
    }

    fn close(&mut self) {
        self.back.close();
    }
}
