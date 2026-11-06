// SPDX-License-Identifier: MIT
//! `ddraw.h`'s numbers, the SDK's names kept, and where a description's fields
//! lie, by 32-bit word.

pub const DD_OK: u32 = 0;
/// `MAKE_DDHRESULT(120)`.
pub const DDERR_INVALIDMODE: u32 = 0x8876_0078;
/// `MAKE_DDHRESULT(130)`.
pub const DDERR_INVALIDOBJECT: u32 = 0x8876_0082;
/// `E_INVALIDARG`.
pub const DDERR_INVALIDPARAMS: u32 = 0x8007_0057;
/// `MAKE_DDHRESULT(582)`.
pub const DDERR_NOTFLIPPABLE: u32 = 0x8876_0246;
/// `MAKE_DDHRESULT(255)`.
pub const DDERR_NOTFOUND: u32 = 0x8876_00ff;
/// `E_OUTOFMEMORY`.
pub const DDERR_OUTOFMEMORY: u32 = 0x8007_000e;
/// `MAKE_DDHRESULT(450)`: never answered here, nothing being lost; a caller
/// compares `IsLost`'s answer with it.
pub const DDERR_SURFACELOST: u32 = 0x8876_01c2;
/// `E_NOTIMPL`.
pub const DDERR_UNSUPPORTED: u32 = 0x8000_4001;

pub const DDBLT_COLORFILL: u32 = 0x400;
pub const DDBLT_WAIT: u32 = 0x0100_0000;
pub const DDBLTFAST_WAIT: u32 = 0x10;
pub const DDPCAPS_8BIT: u32 = 0x4;
pub const DDPF_PALETTEINDEXED8: u32 = 0x20;
pub const DDPF_RGB: u32 = 0x40;

pub const DDSCAPS_BACKBUFFER: u32 = 0x4;
pub const DDSCAPS_COMPLEX: u32 = 0x8;
pub const DDSCAPS_FLIP: u32 = 0x10;
pub const DDSCAPS_FRONTBUFFER: u32 = 0x20;
pub const DDSCAPS_PRIMARYSURFACE: u32 = 0x200;
pub const DDSCAPS_SYSTEMMEMORY: u32 = 0x800;

pub const DDSD_CAPS: u32 = 0x1;
pub const DDSD_HEIGHT: u32 = 0x2;
pub const DDSD_WIDTH: u32 = 0x4;
pub const DDSD_PITCH: u32 = 0x8;
pub const DDSD_BACKBUFFERCOUNT: u32 = 0x20;
pub const DDSD_PIXELFORMAT: u32 = 0x1000;

/// A `DDSURFACEDESC`, as a 32-bit target's memory holds it.
pub type Desc = [u32; DESC_WORDS];
pub const DESC_WORDS: usize = 27;
/// `sizeof(DDSURFACEDESC)`, its `dwSize`.
pub const DESC_BYTES: u32 = 0x6c;
pub const DESC_SIZE: usize = 0;
pub const DESC_FLAGS: usize = 1;
pub const DESC_HEIGHT: usize = 2;
pub const DESC_WIDTH: usize = 3;
pub const DESC_PITCH: usize = 4;
pub const DESC_BACK_COUNT: usize = 5;
/// `lpSurface`, byte offset `0x24`.
pub const DESC_SURFACE: usize = 9;
/// The `DDPIXELFORMAT`'s `dwSize`, byte offset `0x48`.
pub const DESC_FORMAT_SIZE: usize = 18;
pub const DESC_FORMAT_FLAGS: usize = 19;
pub const DESC_FORMAT_BITS: usize = 21;
/// `ddsCaps.dwCaps`, byte offset `0x68`.
pub const DESC_CAPS: usize = 26;
/// `sizeof(DDPIXELFORMAT)`.
pub const FORMAT_BYTES: u32 = 0x20;

/// A `DDBLTFX`'s words, `dwSize` 100.
pub const FX_WORDS: usize = 25;
/// `dwFillColor`, byte offset `0x50`.
pub const FX_COLOUR: usize = 20;
/// A palette's entries, `DDPCAPS_8BIT`'s.
pub const PALETTE_ENTRIES: usize = 256;
