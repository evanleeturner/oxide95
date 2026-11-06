// SPDX-License-Identifier: MIT
//! The format's bit stream: within a byte the lowest bit comes first, and a
//! run of bits gathers its first bit into the lowest place.
use crate::{Part, Refused};

/// A read position in one part's bytes. Running out is that part's
/// truncation, never a made-up zero.
pub(crate) struct Bits<'a> {
    bytes: &'a [u8],
    byte: usize,
    bit: u8,
    part: Part,
}

impl<'a> Bits<'a> {
    pub(crate) fn new(bytes: &'a [u8], part: Part) -> Self {
        Self {
            bytes,
            byte: 0,
            bit: 0,
            part,
        }
    }

    pub(crate) fn bit(&mut self) -> Result<bool, Refused> {
        let byte = *self
            .bytes
            .get(self.byte)
            .ok_or(Refused::Truncated(self.part))?;
        let set = (byte >> self.bit) & 1 == 1;
        if self.bit == 7 {
            self.bit = 0;
            self.byte += 1;
        } else {
            self.bit += 1;
        }
        Ok(set)
    }

    pub(crate) fn byte(&mut self) -> Result<u8, Refused> {
        let mut value = 0;
        for place in 0..8 {
            if self.bit()? {
                value |= 1 << place;
            }
        }
        Ok(value)
    }

    pub(crate) fn word(&mut self) -> Result<u16, Refused> {
        Ok(u16::from_le_bytes([self.byte()?, self.byte()?]))
    }
}
