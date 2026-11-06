// SPDX-License-Identifier: MIT
//! The decoding trees. Video reads 16-bit values through a word tree, whose
//! leaves were themselves packed through two byte trees; sound reads bytes
//! through byte trees. Three leaves of a word tree stand for the three most
//! recent distinct values rather than for a value of their own, so a repeat
//! costs the shortest codes; a frame starts with all three at 0.
use alloc::vec::Vec;

use crate::Refused;
use crate::bits::Bits;

/// A node is a leaf with this bit; without it, the index of its `1` side,
/// its `0` side being the very next node.
const LEAF: u32 = 1 << 31;
/// With [`LEAF`], the low 2 bits name a recent-value slot, not a value.
const RECENT: u32 = 1 << 30;

/// A tree in one flat run, each node before its children, so a walk only
/// ever moves forward and always ends.
struct Shape(Vec<u32>);

impl Shape {
    /// Unpacks a shape with an explicit stack: its depth is whatever the
    /// stream says, and a recursive reader would hand a damaged clip the
    /// call stack. Each node costs at least one bit, so the bytes bound it.
    fn read(
        bits: &mut Bits<'_>,
        mut leaf: impl FnMut(&mut Bits<'_>) -> Result<u32, Refused>,
    ) -> Result<Self, Refused> {
        let mut nodes = Vec::new();
        let mut open = Vec::new();
        loop {
            if bits.bit()? {
                open.push(nodes.len());
                nodes.push(0);
            } else {
                nodes.push(leaf(bits)?);
                let Some(branch) = open.pop() else {
                    return Ok(Self(nodes));
                };
                nodes[branch] = u32::try_from(nodes.len()).map_err(|_| Refused::TooLarge)?;
            }
        }
    }

    fn walk(&self, bits: &mut Bits<'_>) -> Result<u32, Refused> {
        let mut at = 0;
        loop {
            let node = self.0[at];
            if node & LEAF != 0 {
                return Ok(node);
            }
            at = if bits.bit()? { node as usize } else { at + 1 };
        }
    }
}

/// A tree of byte values. An absent one answers 0 and reads no bits.
pub(crate) struct ByteTree(Option<Shape>);

impl ByteTree {
    /// The tree a mono track's unused right channel holds.
    pub(crate) fn absent() -> Self {
        Self(None)
    }

    pub(crate) fn read(bits: &mut Bits<'_>) -> Result<Self, Refused> {
        if !bits.bit()? {
            return Ok(Self(None));
        }
        let shape = Shape::read(bits, |bits| Ok(LEAF | u32::from(bits.byte()?)))?;
        bits.bit()?;
        Ok(Self(Some(shape)))
    }

    pub(crate) fn decode(&self, bits: &mut Bits<'_>) -> Result<u8, Refused> {
        match &self.0 {
            None => Ok(0),
            Some(shape) => Ok(shape.walk(bits)?.to_le_bytes()[0]),
        }
    }
}

/// A tree of 16-bit values with its three recent-value slots.
pub(crate) struct WordTree {
    shape: Option<Shape>,
    recent: [u16; 3],
}

impl WordTree {
    pub(crate) fn read(bits: &mut Bits<'_>) -> Result<Self, Refused> {
        let mut tree = Self {
            shape: None,
            recent: [0; 3],
        };
        if !bits.bit()? {
            return Ok(tree);
        }
        let low = ByteTree::read(bits)?;
        let high = ByteTree::read(bits)?;
        let markers = [bits.word()?, bits.word()?, bits.word()?];
        let shape = Shape::read(bits, |bits| {
            let value = u16::from_le_bytes([low.decode(bits)?, high.decode(bits)?]);
            for (slot, marker) in (0u32..).zip(markers) {
                if marker == value {
                    return Ok(LEAF | RECENT | slot);
                }
            }
            Ok(LEAF | u32::from(value))
        })?;
        bits.bit()?;
        tree.shape = Some(shape);
        Ok(tree)
    }

    /// Clears the recent-value slots: a frame's first read starts from 0s.
    pub(crate) fn forget(&mut self) {
        self.recent = [0; 3];
    }

    pub(crate) fn decode(&mut self, bits: &mut Bits<'_>) -> Result<u16, Refused> {
        let Some(shape) = &self.shape else {
            return Ok(0);
        };
        let leaf = shape.walk(bits)?;
        let value = if leaf & RECENT == 0 {
            let [low, high, ..] = leaf.to_le_bytes();
            u16::from_le_bytes([low, high])
        } else {
            self.recent[(leaf & 3) as usize]
        };
        if value != self.recent[0] {
            self.recent = [value, self.recent[0], self.recent[1]];
        }
        Ok(value)
    }
}
