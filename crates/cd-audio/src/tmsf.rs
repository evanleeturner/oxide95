// SPDX-License-Identifier: MIT
//! A place on a disc as a program reads it: four bytes, low to high, a track, its
//! minutes, seconds and frames, 75 frames to a second.

/// Audio frames in a second of a disc.
pub const FRAMES_A_SECOND: u32 = 75;
const SECONDS_A_MINUTE: u32 = 60;

/// The word for `frame` frames into `track`. Minutes past 255 do not fit a byte; no disc
/// reaches them.
#[must_use]
pub const fn pack(track: u8, frame: u32) -> u32 {
    let seconds = frame / FRAMES_A_SECOND;
    let minutes = seconds / SECONDS_A_MINUTE;
    track as u32
        | (minutes & 0xff) << 8
        | (seconds % SECONDS_A_MINUTE) << 16
        | (frame % FRAMES_A_SECOND) << 24
}

/// The track and its frame count in a word [`pack`] wrote.
#[must_use]
pub const fn unpack(word: u32) -> (u8, u32) {
    let seconds = byte(word, 1) * SECONDS_A_MINUTE + byte(word, 2);
    (
        byte(word, 0) as u8,
        seconds * FRAMES_A_SECOND + byte(word, 3),
    )
}

const fn byte(word: u32, at: u32) -> u32 {
    (word >> (at * 8)) & 0xff
}
