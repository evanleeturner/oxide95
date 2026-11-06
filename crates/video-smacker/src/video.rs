// SPDX-License-Identifier: MIT
//! A frame's video chunk: 4x4 blocks, left to right and top to bottom, in
//! chains of one kind. A void block leaves what the last frame painted, so
//! the pixels stay with the clip from frame to frame.
use crate::bits::Bits;
use crate::tree::WordTree;
use crate::{Part, Refused};

/// The four trees, in the order the clip stores them.
pub(crate) struct Trees {
    pub(crate) map: WordTree,
    pub(crate) colours: WordTree,
    pub(crate) full: WordTree,
    pub(crate) kinds: WordTree,
}

/// How many blocks of each kind a frame painted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Blocks {
    pub(crate) mono: u32,
    pub(crate) full: u32,
    pub(crate) void: u32,
    pub(crate) solid: u32,
}

/// A chain's length from its 6-bit code: one more than the code up to a
/// chain of 59, then 128, 256, 512, 1024 and 2048.
fn chain(code: u8) -> usize {
    let code = usize::from(code & 0x3f);
    if code < 59 {
        code + 1
    } else {
        128 << (code - 59)
    }
}

/// A mono block: two colours and one bit a pixel, from the top-left pixel
/// along each row, lowest bit first; a set bit takes the colour in the high
/// byte.
fn mono(
    trees: &mut Trees,
    bits: &mut Bits<'_>,
    pixels: &mut [u8],
    at: usize,
    width: usize,
) -> Result<(), Refused> {
    let [clear, set] = trees.colours.decode(bits)?.to_le_bytes();
    let map = trees.map.decode(bits)?;
    for y in 0..4 {
        for x in 0..4 {
            let colour = if (map >> (y * 4 + x)) & 1 == 1 {
                set
            } else {
                clear
            };
            pixels[at + y * width + x] = colour;
        }
    }
    Ok(())
}

/// A full block: four rows, each two values, the first for the row's right
/// pair and the second for its left, the low byte the left pixel of a pair.
fn full(
    trees: &mut Trees,
    bits: &mut Bits<'_>,
    pixels: &mut [u8],
    at: usize,
    width: usize,
) -> Result<(), Refused> {
    for y in 0..4 {
        let row = at + y * width;
        let [third, fourth] = trees.full.decode(bits)?.to_le_bytes();
        let [first, second] = trees.full.decode(bits)?.to_le_bytes();
        pixels[row..row + 4].copy_from_slice(&[first, second, third, fourth]);
    }
    Ok(())
}

/// Paints one video chunk over `pixels`, whose width and height are whole
/// numbers of blocks. Bits left over after the last block are padding.
pub(crate) fn paint(
    chunk: &[u8],
    trees: &mut Trees,
    pixels: &mut [u8],
    width: usize,
    painted: &mut [bool],
) -> Result<Blocks, Refused> {
    trees.map.forget();
    trees.colours.forget();
    trees.full.forget();
    trees.kinds.forget();
    let mut bits = Bits::new(chunk, Part::Video);
    let across = width / 4;
    let total = pixels.len() / 16;
    let mut counts = Blocks::default();
    let mut block = 0;
    while block < total {
        let [code, extra] = trees.kinds.decode(&mut bits)?.to_le_bytes();
        let end = usize::min(block + chain(code >> 2), total);
        painted[block..end].fill(code & 3 != 2);
        for one in block..end {
            let at = (one / across) * 4 * width + (one % across) * 4;
            match code & 3 {
                0 => {
                    mono(trees, &mut bits, pixels, at, width)?;
                    counts.mono += 1;
                }
                1 => {
                    full(trees, &mut bits, pixels, at, width)?;
                    counts.full += 1;
                }
                2 => counts.void += 1,
                _ => {
                    for y in 0..4 {
                        let row = at + y * width;
                        pixels[row..row + 4].fill(extra);
                    }
                    counts.solid += 1;
                }
            }
        }
        block = end;
    }
    Ok(counts)
}
