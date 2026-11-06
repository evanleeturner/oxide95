// SPDX-License-Identifier: MIT
//! A Smacker clip's decoder: bytes in; pixels, palette and samples out.
//! "Frame" here is the clip's frame. It does no I/O and names no game, and
//! holds no float and no `unsafe`, so a page's `wasm32` and a game's swap
//! can both take it.
//!
//! Written from the format's public description, its gaps closed by
//! comparing the answers of a decoder run as a program, never from any
//! decoder's code. The forms no clip on hand shows are refused by name,
//! with no code behind them: see [`Refused`].
#![cfg_attr(not(test), no_std)]

extern crate alloc;

mod bits;
mod palette;
mod refused;
mod sound;
#[cfg(test)]
mod tests;
mod tree;
mod video;

use alloc::vec;
use alloc::vec::Vec;
use core::fmt;

use bits::Bits;
pub use refused::{Part, Refused};
use refused::logged;
use tree::WordTree;
use video::Trees;

/// The header's fixed length.
const HEADER_BYTES: usize = 104;
/// The most pixels a clip may ask for: a damaged size must not take the
/// memory, and the largest frame on hand is a sixth of a megapixel.
const MOST_PIXELS: u64 = 1 << 26;
/// The largest unpacked sound buffer a clip may state.
const MOST_SOUND_BYTES: u32 = 1 << 24;

/// What a clip says about itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    /// Pixels across, a multiple of 4.
    pub width: u32,
    /// Pixels down, a multiple of 4.
    pub height: u32,
    /// Frames in the clip.
    pub frames: u32,
    /// A frame's length in microseconds.
    pub frame_us: u32,
    /// Track 0, if the clip has sound.
    pub track: Option<Track>,
}

/// A sound track: always packed 16-bit samples.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Track {
    /// 1 or 2; 2 interleaves left then right.
    pub channels: u8,
    /// Samples a second, a channel.
    pub hz: u32,
    /// The most bytes any one frame's sound unpacks to.
    pub largest_unpacked: u32,
}

/// A frame as the caller keeps it, its own copy, free to be written over:
/// the next frame paints from the clip's pixels, never from these.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picture {
    /// One palette index a pixel, row by row with no gap between rows.
    pub pixels: Vec<u8>,
    /// Red, green and blue an entry, 256 entries.
    pub palette: [u8; 768],
}

impl Picture {
    /// An empty picture; the first frame painted sizes it.
    #[must_use]
    pub fn new() -> Self {
        Self {
            pixels: Vec::new(),
            palette: [0; 768],
        }
    }
}

impl Default for Picture {
    fn default() -> Self {
        Self::new()
    }
}

/// What painting a frame changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Painted {
    /// The frame painted, counted from 0.
    pub frame: u32,
    /// Whether the frame carried a palette.
    pub palette_changed: bool,
    /// Blocks written: those not left as the last frame had them.
    pub blocks_changed: u32,
}

/// A frame's bytes, cut at its chunks.
struct Chunks<'a> {
    palette: Option<&'a [u8]>,
    sound: Option<&'a [u8]>,
    video: &'a [u8],
}

/// A parsed clip and its place: the cursor is the frame
/// [`Clip::decode_next_frame`] paints next. It owns its bytes, so a game's
/// swap can hold one with no lifetime to carry.
pub struct Clip {
    header: Header,
    kinds: Vec<u8>,
    ends: Vec<usize>,
    data: Vec<u8>,
    trees: Trees,
    pixels: Vec<u8>,
    palette: [u8; palette::BYTES],
    cursor: u32,
}

impl fmt::Debug for Clip {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Clip")
            .field("header", &self.header)
            .field("cursor", &self.cursor)
            .finish_non_exhaustive()
    }
}

/// The `at`th little-endian `u32` of `bytes`, which holds it.
fn word(bytes: &[u8], at: usize) -> u32 {
    let mut four = [0; 4];
    four.copy_from_slice(&bytes[at * 4..at * 4 + 4]);
    u32::from_le_bytes(four)
}

/// Refuses each form that has no code: a clip showing one stops here.
fn check_form(head: &[u8]) -> Result<(), Refused> {
    match &head[..4] {
        b"SMK2" => {}
        b"SMK4" => return Err(Refused::Smk4),
        _ => return Err(Refused::NotSmacker),
    }
    let flags = word(head, 5);
    if flags & 1 != 0 {
        return Err(Refused::RingFrame);
    }
    if flags & 2 != 0 {
        return Err(Refused::Interlaced);
    }
    if flags & 4 != 0 {
        return Err(Refused::Doubled);
    }
    if flags != 0 {
        return Err(Refused::UnknownFlags);
    }
    Ok(())
}

/// Track 0 from its header words; any other track present is refused.
fn read_track(head: &[u8]) -> Result<Option<Track>, Refused> {
    const PACKED: u32 = 1 << 31;
    const PRESENT: u32 = 1 << 30;
    const SIXTEEN: u32 = 1 << 29;
    const STEREO: u32 = 1 << 28;
    const CODEC: u32 = 3 << 26;
    if (1..7).any(|other| word(head, 18 + other) & PRESENT != 0) {
        return Err(Refused::SecondTrack);
    }
    let rate = word(head, 18);
    if rate & PRESENT == 0 {
        return Ok(None);
    }
    if rate & CODEC != 0 {
        return Err(Refused::BinkAudio);
    }
    if rate & PACKED == 0 {
        return Err(Refused::RawSound);
    }
    if rate & SIXTEEN == 0 {
        return Err(Refused::EightBitSound);
    }
    let largest_unpacked = word(head, 6);
    if largest_unpacked > MOST_SOUND_BYTES {
        return Err(Refused::TooLarge);
    }
    Ok(Some(Track {
        channels: if rate & STEREO != 0 { 2 } else { 1 },
        hz: rate & 0x00ff_ffff,
        largest_unpacked,
    }))
}

/// The header's facts, or the first thing about it refused.
fn read_header(head: &[u8]) -> Result<Header, Refused> {
    check_form(head)?;
    let (width, height, frames) = (word(head, 1), word(head, 2), word(head, 3));
    let rate = i32::from_le_bytes(word(head, 4).to_le_bytes());
    if rate >= 0 {
        return Err(Refused::Rate);
    }
    let frame_us = rate.unsigned_abs().checked_mul(10).ok_or(Refused::Rate)?;
    if width == 0 || height == 0 || width % 4 != 0 || height % 4 != 0 {
        return Err(Refused::SizeNotBlocks);
    }
    if u64::from(width) * u64::from(height) > MOST_PIXELS {
        return Err(Refused::TooLarge);
    }
    if frames == 0 {
        return Err(Refused::NoFrames);
    }
    Ok(Header {
        width,
        height,
        frames,
        frame_us,
        track: read_track(head)?,
    })
}

/// Cuts a frame at its chunks, in the stored order: palette, sound, video.
fn cut_frame(frame: &[u8], kind: u8) -> Result<Chunks<'_>, Refused> {
    let mut rest = frame;
    let mut palette = None;
    if kind & 1 != 0 {
        let cut = Refused::Truncated(Part::Palette);
        let length = usize::from(*rest.first().ok_or(cut)?) * 4;
        if length == 0 {
            return Err(Refused::Palette);
        }
        let (chunk, after) = rest.split_at_checked(length).ok_or(cut)?;
        palette = Some(chunk);
        rest = after;
    }
    let mut sound = None;
    if kind & 2 != 0 {
        let cut = Refused::Truncated(Part::Sound);
        let (length, _) = rest.split_first_chunk::<4>().ok_or(cut)?;
        let length = u32::from_le_bytes(*length) as usize;
        if length < 8 {
            return Err(Refused::SoundChunk);
        }
        let (chunk, after) = rest.split_at_checked(length).ok_or(cut)?;
        sound = Some(&chunk[4..]);
        rest = after;
    }
    Ok(Chunks {
        palette,
        sound,
        video: rest,
    })
}

impl Clip {
    /// Reads a whole clip and takes its own copy of the bytes. A clip cut
    /// short anywhere the header counts is refused here, before any frame.
    pub fn parse(bytes: &[u8]) -> Result<Self, Refused> {
        Self::read(bytes).map_err(logged)
    }

    fn read(bytes: &[u8]) -> Result<Self, Refused> {
        let (head, rest) = bytes
            .split_at_checked(HEADER_BYTES)
            .ok_or(Refused::Truncated(Part::Header))?;
        let header = read_header(head)?;
        let frames = header.frames as usize;
        let cut = Refused::Truncated(Part::Tables);
        let (sizes, rest) = rest
            .split_at_checked(frames.checked_mul(4).ok_or(cut)?)
            .ok_or(cut)?;
        let (kinds, rest) = rest.split_at_checked(frames).ok_or(cut)?;
        for &kind in kinds {
            if kind & 0xfc != 0 {
                return Err(Refused::SecondTrack);
            }
            if kind & 2 != 0 && header.track.is_none() {
                return Err(Refused::SoundWithoutTrack);
            }
        }
        let (packed_trees, rest) = rest
            .split_at_checked(word(head, 13) as usize)
            .ok_or(Refused::Truncated(Part::Trees))?;
        let mut bits = Bits::new(packed_trees, Part::Trees);
        let trees = Trees {
            map: WordTree::read(&mut bits)?,
            colours: WordTree::read(&mut bits)?,
            full: WordTree::read(&mut bits)?,
            kinds: WordTree::read(&mut bits)?,
        };
        let cut = Refused::Truncated(Part::Frames);
        let mut ends = Vec::with_capacity(frames);
        let mut end = 0usize;
        for frame in 0..frames {
            // The size's low 2 bits are marks, not length.
            end = end
                .checked_add((word(sizes, frame) & !3) as usize)
                .ok_or(cut)?;
            ends.push(end);
        }
        let data = rest.get(..end).ok_or(cut)?.to_vec();
        let pixels = vec![0; (header.width as usize) * (header.height as usize)];
        log::debug!(
            w = header.width, h = header.height, f = header.frames, t = header.frame_us,
            c = header.track.map_or(0, |track| track.channels); "smk.open"
        );
        Ok(Self {
            header,
            kinds: kinds.to_vec(),
            ends,
            data,
            trees,
            pixels,
            palette: [0; palette::BYTES],
            cursor: 0,
        })
    }

    /// What the clip says about itself.
    #[must_use]
    pub fn header(&self) -> &Header {
        &self.header
    }

    /// The frame [`Clip::decode_next_frame`] paints next; the frame count
    /// once the last has been painted.
    #[must_use]
    pub fn cursor(&self) -> u32 {
        self.cursor
    }

    /// The bytes of frame `frame`, which the caller has checked exists, and
    /// its type byte.
    fn frame_bytes(&self, frame: u32) -> (&[u8], u8) {
        let at = frame as usize;
        let start = if at == 0 { 0 } else { self.ends[at - 1] };
        (&self.data[start..self.ends[at]], self.kinds[at])
    }

    /// Moves the cursor so frame `frame` paints next. The format has no key
    /// frames to land on, so this paints every frame on the way, and going
    /// back starts again from the first, whose palette is the clip's.
    pub fn seek(&mut self, frame: u32) -> Result<(), Refused> {
        self.seek_to(frame).map_err(logged)
    }

    fn seek_to(&mut self, frame: u32) -> Result<(), Refused> {
        if frame >= self.header.frames {
            return Err(Refused::PastEnd);
        }
        if frame < self.cursor {
            self.pixels.fill(0);
            self.palette = [0; palette::BYTES];
            self.cursor = 0;
        }
        while self.cursor < frame {
            self.paint_next()?;
        }
        Ok(())
    }

    /// Paints the frame at the cursor over the clip's own pixels and moves
    /// the cursor on. A refused frame leaves the cursor there; its pixels may
    /// be part painted until a seek back starts again.
    fn paint_next(&mut self) -> Result<Painted, Refused> {
        let frame = self.cursor;
        if frame >= self.header.frames {
            return Err(Refused::PastEnd);
        }
        let at = frame as usize;
        let start = if at == 0 { 0 } else { self.ends[at - 1] };
        let bytes = &self.data[start..self.ends[at]];
        let chunks = cut_frame(bytes, self.kinds[at])?;
        if let Some(chunk) = chunks.palette {
            palette::apply(chunk, &mut self.palette)?;
        }
        let blocks = video::paint(
            chunks.video,
            &mut self.trees,
            &mut self.pixels,
            self.header.width as usize,
        )?;
        log::debug!(
            i = frame, n = bytes.len(), p = u8::from(chunks.palette.is_some()),
            m = blocks.mono, u = blocks.full, s = blocks.solid, v = blocks.void; "smk.frame"
        );
        let painted = Painted {
            frame,
            palette_changed: chunks.palette.is_some(),
            blocks_changed: blocks.mono + blocks.full + blocks.solid,
        };
        self.cursor = frame + 1;
        Ok(painted)
    }

    /// Paints the frame at the cursor into `picture` and moves the cursor on.
    pub fn decode_next_frame(&mut self, picture: &mut Picture) -> Result<Painted, Refused> {
        let painted = self.paint_next().map_err(logged)?;
        picture.pixels.clear();
        picture.pixels.extend_from_slice(&self.pixels);
        picture.palette = self.palette;
        Ok(painted)
    }

    /// The samples of the frame at the cursor, interleaved left then right,
    /// none if it holds no sound. The cursor stays: read this, then paint.
    pub fn frame_samples(&self) -> Result<Vec<i16>, Refused> {
        self.samples_here().map_err(logged)
    }

    fn samples_here(&self) -> Result<Vec<i16>, Refused> {
        if self.cursor >= self.header.frames {
            return Err(Refused::PastEnd);
        }
        let (bytes, kind) = self.frame_bytes(self.cursor);
        let chunks = cut_frame(bytes, kind)?;
        let (Some(chunk), Some(track)) = (chunks.sound, self.header.track.as_ref()) else {
            return Ok(Vec::new());
        };
        let samples = sound::unpack(chunk, track)?;
        log::debug!(i = self.cursor, n = samples.len(); "smk.sound");
        Ok(samples)
    }
}
