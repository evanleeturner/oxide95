// SPDX-License-Identifier: MIT
//! The back over cpal, for Linux and macOS, behind the feature `native`: a fixed ring
//! between the mix's writes and the device's callback, silence on an underrun. Windows
//! stays `waveout`, and the browser stays `page`.
//!
//! The crate starts no thread: a program pumps [`crate::Front::pump`] from its own loop,
//! and the device's callback, on cpal's thread, only empties the ring. That callback never
//! logs; the next [`Output::write`] reports what it counted. The rate is never changed
//! here, so a device with no format at the mix's rate refuses the open, and the mix goes
//! on by its clock unwritten.
//!
//! Call: `let front = Front::new(&ROW, host, Native::new());`.

use alloc::boxed::Box;
use alloc::sync::Arc;
use core::fmt::Display;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, FromSample, SampleFormat, SizedSample, StreamConfig};

use crate::lock::Lock;
use crate::output::Output;
use crate::ring::Ring;

/// A callback's slice is converted outside the lock, this many samples at a time, so a
/// writer waits on a copy of this size and never on a whole callback.
const FILL_CHUNK: usize = 256;

/// What the mix's side and the callback share.
pub(crate) struct Shared {
    pub(crate) ring: Ring,
    /// Frames of silence the callback gave since the open.
    short_frames: u64,
}

/// Opens a device for `rate_hz` and starts it calling [`fill`]; the answer keeps it
/// playing until dropped. A dropped stream's callback has ended. A test hands a fake.
type Open = fn(u32, &Arc<Lock<Shared>>) -> Result<Box<dyn Send>, &'static str>;

/// The device's end of the seam.
pub struct Native {
    pub(crate) shared: Arc<Lock<Shared>>,
    held: Option<Box<dyn Send>>,
    device: Open,
    /// The part of `short_frames` already logged.
    told: u64,
}

/// A program may move its front to the thread that pumps it. The real stream's own
/// `Send` is checked where [`open_device`] boxes it.
const _: fn() = || {
    fn sendable<T: Send>() {}
    sendable::<Native>();
};

impl Default for Native {
    fn default() -> Self {
        Self::new()
    }
}

impl Native {
    /// Touches no device until the front opens it.
    #[must_use]
    pub fn new() -> Self {
        Self::over(open_device)
    }

    pub(crate) fn over(device: Open) -> Self {
        let shared = Shared {
            ring: Ring::new(),
            short_frames: 0,
        };
        Self {
            shared: Arc::new(Lock::new(shared)),
            held: None,
            device,
            told: 0,
        }
    }
}

impl Output for Native {
    fn open(&mut self, rate_hz: u32) -> u32 {
        self.close();
        match (self.device)(rate_hz, &self.shared) {
            Ok(stream) => {
                self.held = Some(stream);
                log::debug!(rate:% = oxide_trace::__shown!(rate_hz), by = "sound-mix"; "sound.native.opened");
                rate_hz
            }
            Err(why) => {
                log::warn!(rate:% = oxide_trace::__shown!(rate_hz), why = why, by = "sound-mix"; "sound.native.open.refused");
                0
            }
        }
    }

    fn room(&self) -> usize {
        if self.held.is_some() {
            self.shared.with(|shared| shared.ring.room())
        } else {
            0
        }
    }

    fn write(&mut self, frames: &[i16]) {
        if self.held.is_none() {
            return;
        }
        let short_frames = self.shared.with(|shared| {
            shared.ring.push(frames);
            shared.short_frames
        });
        if short_frames != self.told {
            let frames = short_frames - self.told;
            log::debug!(frames:% = oxide_trace::__shown!(frames), by = "sound-mix"; "sound.native.underrun");
            self.told = short_frames;
        }
    }

    /// The stream ends first and outside the lock: its end waits for a callback that may
    /// be waiting for the lock, and whatever that callback left is cleared after it.
    fn close(&mut self) {
        let Some(stream) = self.held.take() else {
            return;
        };
        drop(stream);
        let lost = self.shared.with(|shared| {
            shared.short_frames = 0;
            shared.ring.clear()
        });
        self.told = 0;
        log::debug!(frames:% = oxide_trace::__shown!(lost), by = "sound-mix"; "sound.native.closed");
    }
}

/// The device's callback. `into` is interleaved stereo; what the ring cannot give is
/// silence, counted in frames.
pub(crate) fn fill<T: Copy + FromSample<i16>>(shared: &Lock<Shared>, into: &mut [T]) {
    let mut chunk = [0_i16; FILL_CHUNK];
    for slots in into.chunks_mut(FILL_CHUNK) {
        let given = shared.with(|shared| {
            let given = shared.ring.pull(&mut chunk[..slots.len()]) * 2;
            shared.short_frames += ((slots.len() - given) / 2) as u64;
            given
        });
        let (full, empty) = slots.split_at_mut(given);
        for (slot, &sample) in full.iter_mut().zip(&chunk) {
            *slot = T::from_sample_(sample);
        }
        empty.fill(T::from_sample_(0));
    }
}

/// The format to ask of a device's rows of channels, lowest rate, highest rate and
/// format: 2 channels with the rate in range, `I16` if any row has it, else `F32`.
pub(crate) fn choose(
    rows: impl Iterator<Item = (u16, u32, u32, SampleFormat)>,
    rate_hz: u32,
) -> Option<SampleFormat> {
    let mut found = None;
    for (channels, lowest, highest, format) in rows {
        if channels != 2 || !(lowest..=highest).contains(&rate_hz) {
            continue;
        }
        match format {
            SampleFormat::I16 => return Some(format),
            SampleFormat::F32 => found = Some(format),
            _ => {}
        }
    }
    found
}

/// Logs what cpal said and answers the words the open reports.
fn said(error: impl Display, words: &'static str) -> &'static str {
    log::warn!(why:% = error, by = "sound-mix"; "sound.native.stream.error");
    words
}

fn open_device(rate_hz: u32, shared: &Arc<Lock<Shared>>) -> Result<Box<dyn Send>, &'static str> {
    let device = cpal::default_host()
        .default_output_device()
        .ok_or("no default output device")?;
    let rows = device
        .supported_output_configs()
        .map_err(|error| said(error, "the device's formats could not be read"))?;
    let rows = rows.map(|row| {
        (
            row.channels(),
            row.min_sample_rate(),
            row.max_sample_rate(),
            row.sample_format(),
        )
    });
    let format = choose(rows, rate_hz).ok_or("no stereo i16 or f32 format at this rate")?;
    let stream = if format == SampleFormat::I16 {
        build::<i16>(&device, rate_hz, shared)
    } else {
        build::<f32>(&device, rate_hz, shared)
    }
    .map_err(|error| said(error, "the device refused the stream"))?;
    stream
        .play()
        .map_err(|error| said(error, "the device refused to start"))?;
    Ok(Box::new(stream))
}

fn build<T>(
    device: &cpal::Device,
    rate_hz: u32,
    shared: &Arc<Lock<Shared>>,
) -> Result<cpal::Stream, cpal::Error>
where
    T: SizedSample + FromSample<i16> + Send + 'static,
{
    let config = StreamConfig {
        channels: 2,
        sample_rate: rate_hz,
        buffer_size: BufferSize::Default,
    };
    let shared = shared.clone();
    device.build_output_stream::<T, _, _>(
        config,
        move |into: &mut [T], _: &cpal::OutputCallbackInfo| fill(&shared, into),
        |error| {
            said(error, "");
        },
        None,
    )
}
