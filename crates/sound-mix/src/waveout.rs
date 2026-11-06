// SPDX-License-Identifier: MIT
//! The `waveOut` back: the mix through winmm, its calls and structs declared by hand,
//! with no outside crate.
//!
//! winmm is asked for no callback function. Its documents say "Calling other wave
//! functions will cause deadlock" inside one, and one would be a thread nobody here
//! names; so the open asks for `CALLBACK_NULL`, and each buffer's `WHDR_DONE` is read
//! back from its header.
//!
//! Every [`Output::write`] goes out at once, short or not. Only the mix calls a back and
//! the mix runs only while something sounds, so a frame held for its buffer to fill
//! could sit unheard until the next sound. The cost: no lead is built up front, so a late
//! pump leaves a gap. A gap also puts the device behind, so the lead grows to the worst
//! lateness seen.
//!
//! The order of locks, outermost first: the front's one lock, held through every call
//! into this file; whatever winmm holds inside its own calls; the logger's. Nothing here
//! takes the front's lock, and no winmm call comes back into this crate. The logger's
//! place breaks the front's rule of writing lines with its lock free, since a back has no
//! [`crate::Host`]: so only an open, a close and a refusal write a line, never a normal
//! write.
//!
//! The buffers live on the heap from a good open to the close: winmm holds their
//! addresses, and whatever holds this back may move it. A refused open allocates nothing,
//! and neither do `room` and `write`.
//!
//! The thread: [`pump_until`] is its whole body. On Windows `start_pump_thread` makes it,
//! and the holder starts it and ends it by setting `stop`. The crate never starts one, and
//! the browser's build has no thread, only a frame calling [`Front::pump`].
#![allow(unsafe_code)]

use alloc::vec::Vec;
use core::cell::UnsafeCell;
use core::mem::{offset_of, size_of};
use core::ptr::{addr_of_mut, null_mut};
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use crate::output::Output;
use crate::{Front, Host, MIX_CHUNK};

pub(crate) const WAVE_FORMAT_PCM: u16 = 1;
pub(crate) const WAVE_MAPPER: u32 = u32::MAX;
pub(crate) const CALLBACK_NULL: u32 = 0;
pub(crate) const WHDR_DONE: u32 = 1;

/// How many buffers the device's queue holds, and each one's frames: about 23 ms each at
/// the mix's rate, so about 370 ms of a stalled pump before frames are dropped and counted.
pub(crate) const BUFFERS: usize = 16;
pub(crate) const BUFFER_FRAMES: usize = 1024;

/// winmm's `WAVEHDR`. Its numbers are mingw-w64's `mmeapi.h`, under `mmsystem.h`'s packing
/// of 1; each field already falls on its own alignment, so `repr(C)` lays it out the same.
#[repr(C)]
pub(crate) struct WaveHdr {
    pub(crate) data: *mut i16,
    pub(crate) length: u32,
    recorded: u32,
    user: usize,
    pub(crate) flags: u32,
    loops: u32,
    next: *mut WaveHdr,
    reserved: usize,
}

impl WaveHdr {
    /// What a prepare asks for: the buffer and its length in bytes, and the flags 0.
    pub(crate) const fn new(data: *mut i16, length: u32) -> Self {
        Self { data, length, recorded: 0, user: 0, flags: 0, loops: 0, next: null_mut(), reserved: 0 }
    }
}

/// winmm's `WAVEFORMATEX`, from the same header; truly packed, its `rate` falling at 4 but
/// its size at 18.
#[derive(Clone, Copy)]
#[repr(C, packed)]
pub(crate) struct WaveFormatEx {
    pub(crate) tag: u16,
    pub(crate) channels: u16,
    pub(crate) rate: u32,
    pub(crate) bytes_per_second: u32,
    pub(crate) block_align: u16,
    pub(crate) bits: u16,
    pub(crate) extra: u16,
}

#[cfg(target_pointer_width = "32")]
const HEADER_LAYOUT: (usize, [usize; 8]) = (32, [0, 4, 8, 12, 16, 20, 24, 28]);
#[cfg(target_pointer_width = "64")]
const HEADER_LAYOUT: (usize, [usize; 8]) = (48, [0, 8, 12, 16, 24, 28, 32, 40]);

/// Nothing runs these structs on a machine without winmm, so the compiler is the witness.
const _: () = {
    let (size, at) = HEADER_LAYOUT;
    assert!(size_of::<WaveHdr>() == size);
    assert!(offset_of!(WaveHdr, data) == at[0]);
    assert!(offset_of!(WaveHdr, length) == at[1]);
    assert!(offset_of!(WaveHdr, recorded) == at[2]);
    assert!(offset_of!(WaveHdr, user) == at[3]);
    assert!(offset_of!(WaveHdr, flags) == at[4]);
    assert!(offset_of!(WaveHdr, loops) == at[5]);
    assert!(offset_of!(WaveHdr, next) == at[6]);
    assert!(offset_of!(WaveHdr, reserved) == at[7]);
    assert!(size_of::<WaveFormatEx>() == 18);
    assert!(offset_of!(WaveFormatEx, tag) == 0);
    assert!(offset_of!(WaveFormatEx, channels) == 2);
    assert!(offset_of!(WaveFormatEx, rate) == 4);
    assert!(offset_of!(WaveFormatEx, bytes_per_second) == 8);
    assert!(offset_of!(WaveFormatEx, block_align) == 12);
    assert!(offset_of!(WaveFormatEx, bits) == 14);
    assert!(offset_of!(WaveFormatEx, extra) == 16);
};

#[allow(clippy::cast_possible_truncation)]
const HEADER_SIZE: u32 = size_of::<WaveHdr>() as u32;

/// winmm's six calls, each answering its `MMRESULT`, 0 for success. The seam: the real ones
/// behind `cfg(windows)`, a fake in the tests.
pub(crate) trait Winmm {
    /// # Safety
    /// `handle` and `format` are valid for this call.
    unsafe fn open(
        &self,
        handle: *mut usize,
        device: u32,
        format: *const WaveFormatEx,
        callback: usize,
        instance: usize,
        flags: u32,
    ) -> u32;
    /// # Safety
    /// `header` is valid and stays put until unprepared, and its buffer with it.
    unsafe fn prepare(&self, handle: usize, header: *mut WaveHdr, size: u32) -> u32;
    /// # Safety
    /// As [`Winmm::prepare`].
    unsafe fn write(&self, handle: usize, header: *mut WaveHdr, size: u32) -> u32;
    /// # Safety
    /// `header` is valid for this call.
    unsafe fn unprepare(&self, handle: usize, header: *mut WaveHdr, size: u32) -> u32;
    /// # Safety
    /// `handle` came from a good [`Winmm::open`] and is not closed.
    unsafe fn reset(&self, handle: usize) -> u32;
    /// # Safety
    /// As [`Winmm::reset`].
    unsafe fn close(&self, handle: usize) -> u32;
}

/// One buffer and its header. `prepared` runs from a good prepare to a good unprepare; the
/// device touches the header only inside that span.
struct Slot {
    header: UnsafeCell<WaveHdr>,
    samples: Vec<i16>,
    prepared: bool,
}

// SAFETY: the header's pointers lead only to this slot's own `samples`, which move with it,
// and `Back` reaches a slot only through the front's lock. winmm's own thread writes only
// `flags`, and those are read atomically.
unsafe impl Send for Slot {}

impl Slot {
    fn new(frames: usize) -> Self {
        Self {
            header: UnsafeCell::new(WaveHdr::new(null_mut(), 0)),
            samples: alloc::vec![0; frames * 2],
            prepared: false,
        }
    }

    /// Whether the device has finished this buffer. Another thread sets the flag.
    fn done(&self) -> bool {
        // SAFETY: the header lives as long as `self`, `flags` is a `u32` at an aligned
        // offset, and while another thread may write it nothing here writes it plainly.
        let flags = unsafe { AtomicU32::from_ptr(addr_of_mut!((*self.header.get()).flags)) };
        flags.load(Ordering::Acquire) & WHDR_DONE != 0
    }
}

/// A winmm call said no: one line, and from here this back takes nothing, so the front
/// counts what follows as dropped and no flood of lines comes after it.
fn refused(dead: &mut bool, call: &str, code: u32) {
    if !*dead {
        log::warn!(
            call = call,
            code:% = oxide_trace::__shown!(code),
            by = "sound-mix"; "sound.waveout.refused"
        );
    }
    *dead = true;
}

/// The back over any [`Winmm`]; its numbers are the buffers and each one's frames.
pub(crate) struct Back<W: Winmm> {
    winmm: W,
    handle: usize,
    count: usize,
    frames: usize,
    slots: Vec<Slot>,
    dead: bool,
}

impl<W: Winmm> Back<W> {
    /// `const`, so a `static` can hold a [`Front`] over it. `frames` must divide the mix's
    /// chunk: the front reads [`Output::room`] once and then writes chunk after chunk, and
    /// each write starts a new buffer, so any other size would promise room it did not have.
    pub(crate) const fn new(winmm: W, count: usize, frames: usize) -> Self {
        assert!(
            count > 0 && frames > 0 && MIX_CHUNK % (frames as u64) == 0,
            "a back needs buffers, each dividing the mix's chunk"
        );
        Self { winmm, handle: 0, count, frames, slots: Vec::new(), dead: false }
    }

    /// Hands back every buffer the device has finished.
    fn reap(&mut self) {
        if self.dead {
            return;
        }
        for slot in &mut self.slots {
            if slot.prepared && slot.done() {
                // SAFETY: the handle is open while slots exist, and the header is this slot's.
                let code = unsafe { self.winmm.unprepare(self.handle, slot.header.get(), HEADER_SIZE) };
                if code != 0 {
                    refused(&mut self.dead, "waveOutUnprepareHeader", code);
                    return;
                }
                slot.prepared = false;
            }
        }
    }

    /// One buffer's worth to the device, with a fresh prepare: the documents name the
    /// header's length as set before it, so a short fill never changes a prepared header.
    #[allow(clippy::cast_possible_truncation)]
    fn send(&mut self, fill: &[i16]) {
        let slot = self
            .slots
            .iter_mut()
            .find(|slot| !slot.prepared)
            .expect("invariant: the front writes no more than room said, and the chunk divides");
        slot.samples[..fill.len()].copy_from_slice(fill);
        let header = slot.header.get();
        // SAFETY: not prepared, so neither winmm nor its thread holds this header.
        unsafe { header.write(WaveHdr::new(slot.samples.as_mut_ptr(), size_of_val(fill) as u32)) };
        // SAFETY: the header and its buffer stay put until a good unprepare: the slots'
        // block is not touched until the close, and neither is `samples`' block.
        let code = unsafe { self.winmm.prepare(self.handle, header, HEADER_SIZE) };
        if code != 0 {
            refused(&mut self.dead, "waveOutPrepareHeader", code);
            return;
        }
        slot.prepared = true;
        // SAFETY: as the prepare's, and prepared just above.
        let code = unsafe { self.winmm.write(self.handle, header, HEADER_SIZE) };
        if code != 0 {
            refused(&mut self.dead, "waveOutWrite", code);
        }
    }
}

impl<W: Winmm> Output for Back<W> {
    fn open(&mut self, rate_hz: u32) -> u32 {
        self.close();
        let format = WaveFormatEx {
            tag: WAVE_FORMAT_PCM,
            channels: 2,
            rate: rate_hz,
            bytes_per_second: rate_hz * 4,
            block_align: 4,
            bits: 16,
            extra: 0,
        };
        let mut handle = 0;
        // SAFETY: both pointers are to locals alive through the call, and no callback is asked.
        let code = unsafe {
            self.winmm.open(&raw mut handle, WAVE_MAPPER, &raw const format, 0, 0, CALLBACK_NULL)
        };
        if code != 0 {
            log::debug!(code:% = oxide_trace::__shown!(code), by = "sound-mix"; "sound.waveout.open.refused");
            return 0;
        }
        self.handle = handle;
        self.slots = (0..self.count).map(|_| Slot::new(self.frames)).collect();
        log::debug!(
            buffers:% = oxide_trace::__shown!(self.count),
            frames:% = oxide_trace::__shown!(self.frames),
            by = "sound-mix"; "sound.waveout.opened"
        );
        rate_hz
    }

    fn room(&self) -> usize {
        if self.dead {
            return 0;
        }
        self.slots.iter().filter(|slot| !slot.prepared || slot.done()).count() * self.frames
    }

    fn write(&mut self, frames: &[i16]) {
        self.reap();
        for fill in frames.chunks(self.frames * 2) {
            if self.dead {
                return;
            }
            self.send(fill);
        }
    }

    /// Resets, unprepares each buffer once, closes. Any refusal here means winmm may still
    /// hold a buffer's address, so the buffers are kept for good, never freed under it.
    fn close(&mut self) {
        if self.slots.is_empty() {
            return;
        }
        self.dead = false;
        // SAFETY: slots exist, so the handle is open.
        let code = unsafe { self.winmm.reset(self.handle) };
        if code != 0 {
            refused(&mut self.dead, "waveOutReset", code);
        }
        for slot in &mut self.slots {
            if slot.prepared {
                // SAFETY: as the reset's, and the header is this slot's.
                let code = unsafe { self.winmm.unprepare(self.handle, slot.header.get(), HEADER_SIZE) };
                if code != 0 {
                    refused(&mut self.dead, "waveOutUnprepareHeader", code);
                }
                slot.prepared = code != 0;
            }
        }
        // SAFETY: as the reset's.
        let code = unsafe { self.winmm.close(self.handle) };
        if code != 0 {
            refused(&mut self.dead, "waveOutClose", code);
        }
        let slots = core::mem::take(&mut self.slots);
        if self.dead {
            core::mem::forget(slots);
        } else {
            drop(slots);
            log::debug!(by = "sound-mix"; "sound.waveout.closed");
        }
        self.dead = false;
        self.handle = 0;
    }
}

impl<W: Winmm> Drop for Back<W> {
    /// winmm holds the buffers' addresses until a close, so none may be freed before one.
    fn drop(&mut self) {
        self.close();
    }
}

/// The thread's whole body: pumps until told to stop, with `rest` between pumps. A pump
/// after the front's shutdown touches no device, since nothing is sounding then.
pub(crate) fn pump_until<H: Host, O: Output>(
    front: &Front<H, O>,
    stop: &AtomicBool,
    mut rest: impl FnMut(),
) {
    while !stop.load(Ordering::Acquire) {
        front.pump();
        rest();
    }
}

#[cfg(windows)]
pub use real::{WaveOutBack, start_pump_thread};

#[cfg(windows)]
mod real {
    use alloc::boxed::Box;
    use core::ffi::c_void;
    use core::ptr::null_mut;
    use core::sync::atomic::AtomicBool;

    use super::{
        BUFFER_FRAMES, BUFFERS, Back, Front, Host, Output, WaveFormatEx, WaveHdr, Winmm, pump_until,
    };

    /// How long the pump thread rests between pumps, in milliseconds.
    const PUMP_REST_MS: u32 = 10;

    #[link(name = "winmm")]
    unsafe extern "system" {
        fn waveOutOpen(
            handle: *mut usize,
            device: u32,
            format: *const WaveFormatEx,
            callback: usize,
            instance: usize,
            flags: u32,
        ) -> u32;
        fn waveOutPrepareHeader(handle: usize, header: *mut WaveHdr, size: u32) -> u32;
        fn waveOutWrite(handle: usize, header: *mut WaveHdr, size: u32) -> u32;
        fn waveOutUnprepareHeader(handle: usize, header: *mut WaveHdr, size: u32) -> u32;
        fn waveOutReset(handle: usize) -> u32;
        fn waveOutClose(handle: usize) -> u32;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateThread(
            attributes: *mut c_void,
            stack: usize,
            start: Option<unsafe extern "system" fn(*mut c_void) -> u32>,
            argument: *mut c_void,
            flags: u32,
            id: *mut u32,
        ) -> *mut c_void;
        fn CloseHandle(handle: *mut c_void) -> i32;
        fn Sleep(milliseconds: u32);
    }

    struct Real;

    impl Winmm for Real {
        unsafe fn open(
            &self,
            handle: *mut usize,
            device: u32,
            format: *const WaveFormatEx,
            callback: usize,
            instance: usize,
            flags: u32,
        ) -> u32 {
            // SAFETY: the trait's own.
            unsafe { waveOutOpen(handle, device, format, callback, instance, flags) }
        }

        unsafe fn prepare(&self, handle: usize, header: *mut WaveHdr, size: u32) -> u32 {
            // SAFETY: the trait's own.
            unsafe { waveOutPrepareHeader(handle, header, size) }
        }

        unsafe fn write(&self, handle: usize, header: *mut WaveHdr, size: u32) -> u32 {
            // SAFETY: the trait's own.
            unsafe { waveOutWrite(handle, header, size) }
        }

        unsafe fn unprepare(&self, handle: usize, header: *mut WaveHdr, size: u32) -> u32 {
            // SAFETY: the trait's own.
            unsafe { waveOutUnprepareHeader(handle, header, size) }
        }

        unsafe fn reset(&self, handle: usize) -> u32 {
            // SAFETY: the trait's own.
            unsafe { waveOutReset(handle) }
        }

        unsafe fn close(&self, handle: usize) -> u32 {
            // SAFETY: the trait's own.
            unsafe { waveOutClose(handle) }
        }
    }

    /// The mix through winmm. A newtype, so the seam the tests use never leaks out.
    pub struct WaveOutBack(Back<Real>);

    impl WaveOutBack {
        /// `const`, so a `static` can hold a [`Front`] over it.
        #[must_use]
        pub const fn new() -> Self {
            Self(Back::new(Real, BUFFERS, BUFFER_FRAMES))
        }
    }

    impl Default for WaveOutBack {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Output for WaveOutBack {
        fn open(&mut self, rate_hz: u32) -> u32 {
            self.0.open(rate_hz)
        }

        fn room(&self) -> usize {
            self.0.room()
        }

        fn write(&mut self, frames: &[i16]) {
            self.0.write(frames);
        }

        fn close(&mut self) {
            self.0.close();
        }
    }

    /// Runs [`pump_until`] on a new thread, so a sample ends and its callback fires though
    /// the program calls nothing. The holder starts it and ends it by setting `stop`; a
    /// callback then fires on this thread, beside the program's own. Not `std::thread`:
    /// the crate is `no_std`. False if no thread was made: then only the program's own
    /// calls move the mix.
    pub fn start_pump_thread<H: Host + Sync + 'static, O: Output + Send + 'static>(
        front: &'static Front<H, O>,
        stop: &'static AtomicBool,
    ) -> bool {
        let pair = Box::into_raw(Box::new((front, stop)));
        // SAFETY: the start function matches the signature it takes, and is given the pair
        // of its own types, which it frees; the attributes and id may be null.
        let thread = unsafe {
            CreateThread(null_mut(), 0, Some(pump_thread::<H, O>), pair.cast(), 0, null_mut())
        };
        if thread.is_null() {
            // SAFETY: no thread was made, so this is the only owner of the pair.
            drop(unsafe { Box::from_raw(pair) });
            return false;
        }
        // SAFETY: a good handle, not needed again; the thread runs on without it.
        unsafe { CloseHandle(thread) };
        true
    }

    unsafe extern "system" fn pump_thread<H: Host + Sync + 'static, O: Output + Send + 'static>(
        argument: *mut c_void,
    ) -> u32 {
        // SAFETY: `start_pump_thread` handed this thread the box it made, once.
        let pair = unsafe {
            Box::from_raw(argument.cast::<(&'static Front<H, O>, &'static AtomicBool)>())
        };
        let (front, stop) = *pair;
        pump_until(front, stop, || {
            // SAFETY: no pointer crosses, and a sleep has no precondition.
            unsafe { Sleep(PUMP_REST_MS) }
        });
        0
    }
}
