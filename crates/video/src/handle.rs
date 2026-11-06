// SPDX-License-Identifier: MIT
//! The block a caller reads through the pointer `smack_open` gave it. The first
//! three fields sit where the clip's own header holds them.
use core::mem::offset_of;

/// A palette's bytes: red, green and blue an entry, 256 entries.
pub const PALETTE_BYTES: usize = 768;

/// What a caller reads back: 5 fields at fixed offsets, the rest reserved.
#[repr(C)]
#[derive(Debug)]
pub struct Handle {
    reserved_head: u32,
    /// `+0x4`: pixels across.
    pub width: u32,
    /// `+0x8`: pixels down.
    pub height: u32,
    /// `+0xc`: frames in the clip.
    pub frames: u32,
    reserved_middle: [u8; 0x58],
    /// `+0x68`: 1 when the frame just painted changed the palette, else 0.
    pub palette_changed: u32,
    /// `+0x6c`: the palette as it now stands.
    pub palette: [u8; PALETTE_BYTES],
}

const _: () = {
    assert!(offset_of!(Handle, width) == 0x4);
    assert!(offset_of!(Handle, height) == 0x8);
    assert!(offset_of!(Handle, frames) == 0xc);
    assert!(offset_of!(Handle, palette_changed) == 0x68);
    assert!(offset_of!(Handle, palette) == 0x6c);
};

impl Handle {
    /// A clip's handle before its first frame.
    pub(crate) const fn new(width: u32, height: u32, frames: u32) -> Self {
        Self {
            reserved_head: 0,
            width,
            height,
            frames,
            reserved_middle: [0; 0x58],
            palette_changed: 0,
            palette: [0; PALETTE_BYTES],
        }
    }
}
