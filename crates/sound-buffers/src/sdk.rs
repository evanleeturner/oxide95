// SPDX-License-Identifier: MIT
//! `dsound.h`'s numbers and the two structures a program hands over.

pub const DS_OK: u32 = 0;
pub const DSERR_UNSUPPORTED: u32 = 0x8000_4001;
pub const DSERR_INVALIDPARAM: u32 = 0x8007_0057;
pub const DSERR_NOAGGREGATION: u32 = 0x8004_0110;
pub const DSERR_INVALIDCALL: u32 = 0x8878_0032;
pub const DSERR_BADFORMAT: u32 = 0x8878_0064;

pub const DSBCAPS_PRIMARYBUFFER: u32 = 1;
pub const DSBPLAY_LOOPING: u32 = 1;
pub const DSBLOCK_FROMWRITECURSOR: u32 = 1;
pub const DSBLOCK_ENTIREBUFFER: u32 = 2;
pub const DSBSTATUS_PLAYING: u32 = 1;
pub const DSBSTATUS_LOOPING: u32 = 4;

pub const DSBVOLUME_MIN: i32 = -10_000;
pub const DSBVOLUME_MAX: i32 = 0;
pub const DSBPAN_LEFT: i32 = -10_000;
pub const DSBPAN_RIGHT: i32 = 10_000;
pub const DSBFREQUENCY_ORIGINAL: u32 = 0;
pub const DSBFREQUENCY_MIN: u32 = 100;
/// The older headers' ceiling, the one a program of that age was built against.
pub const DSBFREQUENCY_MAX: u32 = 100_000;
pub const DSBSIZE_MAX: u32 = 0x0fff_ffff;

pub const WAVE_FORMAT_PCM: u16 = 1;

/// A `WAVEFORMATEX`: 18 bytes, its last word off a 4-byte boundary, read and
/// written unaligned.
#[repr(C, packed(2))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WaveFormat {
    pub tag: u16,
    pub channels: u16,
    pub rate_hz: u32,
    pub bytes_per_second: u32,
    pub block_align: u16,
    pub bits: u16,
    pub extra: u16,
}

/// A `DSBUFFERDESC`'s first five fields, all a front reads: the later `DirectX`
/// field past them is not read, and `size` is shown, never judged.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Desc {
    pub size: u32,
    pub flags: u32,
    pub bytes: u32,
    pub reserved: u32,
    pub format: *const WaveFormat,
}
