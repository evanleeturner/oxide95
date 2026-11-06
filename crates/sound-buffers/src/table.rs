// SPDX-License-Identifier: MIT
//! The `DirectSound` rows: what one file asks of the stand-in, kept in its game's
//! rows crate, and the names this crate answers or stops at.

/// One file's `DirectSound` facts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    pub file: &'static str,
    /// The `SetCooperativeLevel` levels it asks; any other is refused.
    pub levels: &'static [u32],
}

/// The names this crate answers: the create, the object's 4, the buffer's 16.
pub const FRONTS: [&str; 21] = [
    "DSOUND!DirectSoundCreate",
    "IDirectSound::Release",
    "IDirectSound::CreateSoundBuffer",
    "IDirectSound::DuplicateSoundBuffer",
    "IDirectSound::SetCooperativeLevel",
    "IDirectSoundBuffer::Release",
    "IDirectSoundBuffer::GetCurrentPosition",
    "IDirectSoundBuffer::GetFormat",
    "IDirectSoundBuffer::GetVolume",
    "IDirectSoundBuffer::GetPan",
    "IDirectSoundBuffer::GetStatus",
    "IDirectSoundBuffer::Lock",
    "IDirectSoundBuffer::Play",
    "IDirectSoundBuffer::SetCurrentPosition",
    "IDirectSoundBuffer::SetFormat",
    "IDirectSoundBuffer::SetVolume",
    "IDirectSoundBuffer::SetPan",
    "IDirectSoundBuffer::SetFrequency",
    "IDirectSoundBuffer::Stop",
    "IDirectSoundBuffer::Unlock",
    "IDirectSoundBuffer::Restore",
];

/// The names that end the program, one line first, never a quiet answer: sound
/// conversion and recording, which nothing here can answer truthfully.
pub const STOPS: [&str; 10] = [
    "MSACM32!acmStreamClose",
    "MSACM32!acmStreamConvert",
    "MSACM32!acmStreamPrepareHeader",
    "MSACM32!acmStreamSize",
    "MSACM32!acmStreamUnprepareHeader",
    "WINMM!waveInAddBuffer",
    "WINMM!waveInClose",
    "WINMM!waveInReset",
    "WINMM!waveInStart",
    "WINMM!waveInStop",
];
