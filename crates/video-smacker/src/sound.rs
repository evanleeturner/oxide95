// SPDX-License-Identifier: MIT
//! A frame's packed sound: starting values, then a byte tree for each byte
//! of a sample and the deltas read through them. Only the form on hand, 16
//! bits packed, has code; the rest is refused by name.
use alloc::vec::Vec;

use crate::bits::Bits;
use crate::tree::ByteTree;
use crate::{Part, Refused, Track};

/// Unpacks one chunk, from its unpacked-length field on, to interleaved
/// samples, left first. A chunk whose first bit says no data gives none.
pub(crate) fn unpack(chunk: &[u8], track: &Track) -> Result<Vec<i16>, Refused> {
    let (length, packed) = chunk
        .split_first_chunk::<4>()
        .ok_or(Refused::Truncated(Part::Sound))?;
    let length = u32::from_le_bytes(*length);
    if length > track.largest_unpacked {
        return Err(Refused::SoundTooLarge);
    }
    let channels = usize::from(track.channels);
    let wanted = length as usize / 2;
    if length % 2 != 0 || wanted % channels != 0 {
        return Err(Refused::SoundChunk);
    }
    let mut bits = Bits::new(packed, Part::Sound);
    if !bits.bit()? {
        return Ok(Vec::new());
    }
    let stereo = bits.bit()?;
    if !bits.bit()? {
        return Err(Refused::EightBitSound);
    }
    if stereo != (channels == 2) {
        return Err(Refused::SoundChunk);
    }
    let left = (ByteTree::read(&mut bits)?, ByteTree::read(&mut bits)?);
    let right = if stereo {
        (ByteTree::read(&mut bits)?, ByteTree::read(&mut bits)?)
    } else {
        (ByteTree::absent(), ByteTree::absent())
    };
    let mut last_right = 0;
    if stereo {
        last_right = base(&mut bits)?;
    }
    let mut last_left = base(&mut bits)?;
    let mut samples = Vec::with_capacity(wanted);
    if wanted == 0 {
        return Ok(samples);
    }
    samples.push(last_left);
    if stereo {
        samples.push(last_right);
    }
    while samples.len() < wanted {
        last_left = last_left.wrapping_add(delta(&left, &mut bits)?);
        samples.push(last_left);
        if stereo {
            last_right = last_right.wrapping_add(delta(&right, &mut bits)?);
            samples.push(last_right);
        }
    }
    Ok(samples)
}

/// A channel's starting value, its high byte first.
fn base(bits: &mut Bits<'_>) -> Result<i16, Refused> {
    let high = bits.byte()?;
    let low = bits.byte()?;
    Ok(i16::from_le_bytes([low, high]))
}

/// A channel's next change, its low byte first.
fn delta(trees: &(ByteTree, ByteTree), bits: &mut Bits<'_>) -> Result<i16, Refused> {
    let low = trees.0.decode(bits)?;
    let high = trees.1.decode(bits)?;
    Ok(i16::from_le_bytes([low, high]))
}
