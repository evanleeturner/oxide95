// SPDX-License-Identifier: MIT
//! A disc's audio tracks as the spans the playing front counts in. An image's tracks say
//! how long they run; a store's files do not, so the caller measures each one.
use alloc::vec::Vec;

use logical_disc::{Audio, Encoded, Track};
use oxide_trace::Args;

use crate::table::Span;
use crate::{Back, numbers};

/// Each of `tracks` as a span. A file `measure` cannot size keeps its number and runs for
/// no frames, so a play of it answers that it is not on the disc: one line, and no fault.
#[must_use]
pub fn spans(
    back: &impl Back,
    tracks: &[Track],
    mut measure: impl FnMut(&Encoded) -> Option<u32>,
) -> Vec<Span> {
    tracks
        .iter()
        .map(|track| {
            let frames = match &track.audio {
                Audio::Pcm(pcm) => pcm.frames,
                Audio::Encoded(encoded) => measure(encoded).unwrap_or_else(|| {
                    unread(back, track.number);
                    0
                }),
            };
            Span {
                number: track.number,
                frames,
            }
        })
        .collect()
}

/// The one line a track that will not read gets.
pub(crate) fn unread(back: &impl Back, track: u8) {
    let track = u32::from(track);
    back.quiet(|| {
        log::warn!(a:% = numbers!(track), by = "cd-audio"; "cd.track.unread");
    });
}
