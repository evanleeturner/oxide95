// SPDX-License-Identifier: MIT
//! A frame's palette chunk: 256 entries of red, green and blue, rewritten
//! from the first entry on by three kinds of run. An entry no run reaches
//! keeps its old colour.
use crate::{Part, Refused};

/// A palette's bytes, red, green and blue an entry.
pub(crate) const BYTES: usize = 768;

/// A stored 6-bit component as 8 bits: four steps of 16 values, each step
/// starting 0x41 above the last, 4 apart within it, so 63 lands on 255.
fn widened(stored: u8) -> u8 {
    let value = stored & 0x3f;
    (value >> 4) * 0x41 + (value & 15) * 4
}

/// Rewrites `palette` from `chunk`, which starts at its own length byte.
pub(crate) fn apply(chunk: &[u8], palette: &mut [u8; BYTES]) -> Result<(), Refused> {
    let cut = Refused::Truncated(Part::Palette);
    let old = *palette;
    let mut entry = 0;
    let mut at = 1;
    while entry < 256 {
        let Some(&first) = chunk.get(at) else {
            break;
        };
        if first & 0x80 != 0 {
            entry += usize::from(first & 0x7f) + 1;
            at += 1;
            if entry > 256 {
                return Err(Refused::Palette);
            }
        } else if first & 0x40 != 0 {
            let count = usize::from(first & 0x3f) + 1;
            let from = usize::from(*chunk.get(at + 1).ok_or(cut)?);
            if entry + count > 256 || from + count > 256 {
                return Err(Refused::Palette);
            }
            palette[entry * 3..(entry + count) * 3]
                .copy_from_slice(&old[from * 3..(from + count) * 3]);
            entry += count;
            at += 2;
        } else {
            let blue = widened(first);
            let green = widened(*chunk.get(at + 1).ok_or(cut)?);
            let red = widened(*chunk.get(at + 2).ok_or(cut)?);
            palette[entry * 3..entry * 3 + 3].copy_from_slice(&[red, green, blue]);
            entry += 1;
            at += 3;
        }
    }
    Ok(())
}
