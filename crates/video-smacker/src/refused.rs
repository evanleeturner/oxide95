// SPDX-License-Identifier: MIT
//! Every reason the crate turns a clip or a frame away, by name. A form no
//! clip on hand shows has no code behind it, only its name here, so a clip
//! that holds one stops with the reason instead of a guessed picture.
use core::fmt;

/// The part of a clip whose bytes ran out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    /// The 104-byte header.
    Header,
    /// The frame size and frame type tables.
    Tables,
    /// The decoding trees.
    Trees,
    /// The frames' bytes, as the size table counts them.
    Frames,
    /// A frame's palette chunk.
    Palette,
    /// A frame's sound chunk.
    Sound,
    /// A frame's video chunk.
    Video,
}

/// Why a clip, a frame or a request was turned away.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refused {
    /// The signature is neither `SMK2` nor `SMK4`.
    NotSmacker,
    /// The `SMK4` revision.
    Smk4,
    /// Flag bit 0: an extra ring frame.
    RingFrame,
    /// Flag bit 1: every other line stored.
    Interlaced,
    /// Flag bit 2: lines to be doubled on show.
    Doubled,
    /// A flag bit no description names.
    UnknownFlags,
    /// A rate of 0 or more, or one too long to count in microseconds.
    Rate,
    /// A width or height of 0, or one not a whole number of 4-pixel blocks.
    SizeNotBlocks,
    /// A size or a sound buffer past what any clip here comes near.
    TooLarge,
    /// A clip of no frames.
    NoFrames,
    /// A track of 8-bit samples.
    EightBitSound,
    /// A track in either of the perceptual codec forms.
    BinkAudio,
    /// A track stored unpacked.
    RawSound,
    /// A sound track other than track 0.
    SecondTrack,
    /// A frame marked as holding sound in a clip that declares no track.
    SoundWithoutTrack,
    /// A sound chunk whose stated form or length does not fit its track.
    SoundChunk,
    /// A sound chunk unpacking past the track's stated largest buffer.
    SoundTooLarge,
    /// A palette chunk of length 0, or one running past entry 255.
    Palette,
    /// All four trees absent: no clip on hand holds it and the decoder
    /// compared against will not open it.
    NoTrees,
    /// A frame number past the clip's last, or a read past its end.
    PastEnd,
    /// The named part's bytes ran out.
    Truncated(Part),
}

impl Refused {
    /// The reason's name, as the `smk.refused` line and a caller's log show it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::NotSmacker => "not-smacker",
            Self::Smk4 => "smk4",
            Self::RingFrame => "ring-frame",
            Self::Interlaced => "interlaced",
            Self::Doubled => "doubled",
            Self::UnknownFlags => "unknown-flags",
            Self::Rate => "rate",
            Self::SizeNotBlocks => "size-not-blocks",
            Self::TooLarge => "too-large",
            Self::NoFrames => "no-frames",
            Self::EightBitSound => "eight-bit-sound",
            Self::BinkAudio => "bink-audio",
            Self::RawSound => "raw-sound",
            Self::SecondTrack => "second-track",
            Self::SoundWithoutTrack => "sound-without-track",
            Self::SoundChunk => "sound-chunk",
            Self::SoundTooLarge => "sound-too-large",
            Self::Palette => "palette",
            Self::NoTrees => "no-trees",
            Self::PastEnd => "past-end",
            Self::Truncated(Part::Header) => "truncated-header",
            Self::Truncated(Part::Tables) => "truncated-tables",
            Self::Truncated(Part::Trees) => "truncated-trees",
            Self::Truncated(Part::Frames) => "truncated-frames",
            Self::Truncated(Part::Palette) => "truncated-palette",
            Self::Truncated(Part::Sound) => "truncated-sound",
            Self::Truncated(Part::Video) => "truncated-video",
        }
    }
}

impl fmt::Display for Refused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl core::error::Error for Refused {}

/// Writes the refusal's one DEBUG line; called where a public call returns it.
pub(crate) fn logged(refused: Refused) -> Refused {
    log::debug!(w = refused.name(); "smk.refused");
    refused
}
