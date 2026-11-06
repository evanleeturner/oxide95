// SPDX-License-Identifier: MIT
//! The `waveOut` back against a fake winmm. The real calls never run on this host, so what
//! these prove stops at the seam: the fake is a witness of what winmm documents, not winmm.
#![allow(unsafe_code)]

use std::collections::VecDeque;
use std::ptr::addr_of_mut;
use std::slice;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::Duration;

use super::{Back as Recording, ROW, Seat, hex, little, pcm, rig, sha256};
use crate::output::Output;
use crate::test_log::debug_lines;
use crate::waveout::{
    BUFFER_FRAMES, BUFFERS, Back as Device, CALLBACK_NULL, WAVE_FORMAT_PCM, WAVE_MAPPER, WHDR_DONE,
    WaveFormatEx, WaveHdr, Winmm, pump_until,
};
use crate::{Front, Host, SMP_DONE, SMP_PLAYING};

/// winmm's answers, from mingw-w64's `mmsyscom.h` and `mmeapi.h`.
const BAD_HANDLE: u32 = 5;
const STILL_PLAYING: u32 = 33;
const UNPREPARED: u32 = 34;
/// Not winmm's: its documents give a second prepare "no effect" and an answer of 0, so here
/// the fake is stricter than the real thing, to catch a back that loses count.
const STRICTER: u32 = 99;
const WHDR_PREPARED: u32 = 2;
const WHDR_INQUEUE: u32 = 16;
#[allow(clippy::cast_possible_truncation)]
const HEADER_SIZE: u32 = size_of::<WaveHdr>() as u32;
const MIX_RATE: u32 = 44_100;

/// One header's flags. Another thread writes them, as a device thread does.
fn flags<'a>(header: *mut WaveHdr) -> &'a AtomicU32 {
    // SAFETY: only live headers come here, and every access to `flags` while one is queued
    // is atomic: the back's read, and the fake's writes.
    unsafe { AtomicU32::from_ptr(addr_of_mut!((*header).flags)) }
}

/// Everything the fake winmm saw. Addresses are kept as numbers, so this crosses threads.
#[derive(Default)]
struct Books {
    at_once: bool,
    refuse_open: u32,
    refuse: Option<(&'static str, u32)>,
    handle: usize,
    opens: usize,
    closes: u32,
    calls: u32,
    asked: Vec<(u32, usize, usize, u32)>,
    formats: Vec<WaveFormatEx>,
    taken: Vec<i16>,
    played: Vec<i16>,
    lengths: Vec<u32>,
    written_at: Vec<usize>,
    queue: VecDeque<usize>,
    prepared: Vec<usize>,
    unprepared: Vec<usize>,
    refusals: Vec<(&'static str, u32)>,
}

impl Books {
    /// A call's first checks: the code a test asked for, then a handle that is not open.
    fn check(&mut self, call: &'static str, handle: usize) -> u32 {
        self.calls += 1;
        if let Some((named, code)) = self.refuse
            && named == call
        {
            return code;
        }
        if self.handle == 0 || handle != self.handle {
            return self.no(call, BAD_HANDLE);
        }
        0
    }

    fn no(&mut self, call: &'static str, code: u32) -> u32 {
        self.refusals.push((call, code));
        code
    }

    /// The device reaches the oldest buffer and plays it, reading its bytes only now: a back
    /// that overwrote a queued buffer shows up here, as played bytes unlike those taken.
    fn play_oldest(&mut self) -> bool {
        let Some(at) = self.queue.pop_front() else {
            return false;
        };
        let header = at as *mut WaveHdr;
        // SAFETY: a queued header stays prepared, its unprepare refused above, so the back
        // holds it and its buffer still and writes neither.
        let samples =
            unsafe { slice::from_raw_parts((*header).data, (*header).length as usize / 2) };
        self.played.extend_from_slice(samples);
        finished(header);
        true
    }
}

fn finished(header: *mut WaveHdr) {
    let flags = flags(header);
    flags.fetch_and(!WHDR_INQUEUE, Ordering::Release);
    flags.fetch_or(WHDR_DONE, Ordering::Release);
}

/// A fake winmm, refusing what the real one documents refusing. Clones share their books.
#[derive(Clone, Default)]
struct Fake(Arc<Mutex<Books>>);

impl Fake {
    /// Every buffer finishes the moment it is written.
    fn at_once() -> Self {
        let fake = Self::default();
        fake.books().at_once = true;
        fake
    }

    /// Buffers wait for [`Fake::finish`]: a stalled device, or one on its own thread.
    fn waits() -> Self {
        Self::default()
    }

    fn books(&self) -> MutexGuard<'_, Books> {
        self.0.lock().expect("no test panics holding this")
    }

    fn finish(&self, buffers: usize) {
        let mut books = self.books();
        for _ in 0..buffers {
            if !books.play_oldest() {
                return;
            }
        }
    }
}

impl Winmm for Fake {
    unsafe fn open(
        &self,
        handle: *mut usize,
        device: u32,
        format: *const WaveFormatEx,
        callback: usize,
        instance: usize,
        flags: u32,
    ) -> u32 {
        let mut books = self.books();
        books.calls += 1;
        books.asked.push((device, callback, instance, flags));
        // SAFETY: the trait's own; the struct is packed, so read it unaligned.
        books.formats.push(unsafe { format.read_unaligned() });
        if books.refuse_open != 0 {
            return books.refuse_open;
        }
        books.opens += 1;
        books.handle = books.opens + 100;
        // SAFETY: the trait's own.
        unsafe { handle.write(books.handle) };
        0
    }

    unsafe fn prepare(&self, handle: usize, header: *mut WaveHdr, _size: u32) -> u32 {
        let mut books = self.books();
        let code = books.check("prepare", handle);
        if code != 0 {
            return code;
        }
        let at = header as usize;
        if books.prepared.contains(&at) {
            return books.no("prepare twice", STRICTER);
        }
        if flags(header).load(Ordering::Acquire) != 0 {
            return books.no("prepare with flags set", STRICTER);
        }
        books.prepared.push(at);
        flags(header).fetch_or(WHDR_PREPARED, Ordering::Release);
        0
    }

    unsafe fn write(&self, handle: usize, header: *mut WaveHdr, _size: u32) -> u32 {
        let mut books = self.books();
        let code = books.check("write", handle);
        if code != 0 {
            return code;
        }
        let at = header as usize;
        if !books.prepared.contains(&at) {
            return books.no("write", UNPREPARED);
        }
        if books.queue.contains(&at) {
            return books.no("write", STILL_PLAYING);
        }
        flags(header).fetch_or(WHDR_INQUEUE, Ordering::Release);
        // SAFETY: the trait's own; the back filled the buffer before this call.
        let (samples, length) = unsafe {
            let length = (*header).length;
            (
                slice::from_raw_parts((*header).data, length as usize / 2),
                length,
            )
        };
        books.taken.extend_from_slice(samples);
        books.lengths.push(length);
        books.written_at.push(at);
        books.queue.push_back(at);
        if books.at_once {
            books.play_oldest();
        }
        0
    }

    unsafe fn unprepare(&self, handle: usize, header: *mut WaveHdr, _size: u32) -> u32 {
        let mut books = self.books();
        let code = books.check("unprepare", handle);
        if code != 0 {
            return code;
        }
        let at = header as usize;
        if books.queue.contains(&at) {
            return books.no("unprepare", STILL_PLAYING);
        }
        if !books.prepared.contains(&at) {
            return books.no("unprepare not prepared", STRICTER);
        }
        books.prepared.retain(|&held| held != at);
        books.unprepared.push(at);
        flags(header).fetch_and(!WHDR_PREPARED, Ordering::Release);
        0
    }

    unsafe fn reset(&self, handle: usize) -> u32 {
        let mut books = self.books();
        let code = books.check("reset", handle);
        if code != 0 {
            return code;
        }
        for at in books.queue.drain(..) {
            finished(at as *mut WaveHdr);
        }
        0
    }

    unsafe fn close(&self, handle: usize) -> u32 {
        let mut books = self.books();
        let code = books.check("close", handle);
        if code != 0 {
            return code;
        }
        if !books.queue.is_empty() {
            return books.no("close", STILL_PLAYING);
        }
        books.handle = 0;
        books.closes += 1;
        0
    }
}

/// Runs `body` on its own thread and fails the test if it passes `seconds`: a hang here is a
/// failure, never a test that waits for ever.
fn within<T: Send + 'static>(seconds: u64, body: impl FnOnce() -> T + Send + 'static) -> T {
    let (sender, receiver) = channel();
    thread::spawn(move || sender.send(body()));
    receiver
        .recv_timeout(Duration::from_secs(seconds))
        .expect("the body ended inside its limit")
}

/// The calls of the crate's reference mix: voices and a door, through set readings.
#[allow(clippy::cast_possible_truncation)]
fn whole_run<O: Output>(output: O) {
    let tone: Vec<u8> = (0..4_000u32).map(|at| (at * 7 % 256) as u8).collect();
    let wide: Vec<i16> = (0..3_000i32).map(|at| (at * 11 % 30_000) as i16).collect();
    let (seat, front, driver) = rig(output);
    let a = front.allocate_file_sample(driver, 16, &pcm(1, 11_025, 8, &tone), 0);
    let b = front.allocate_file_sample(driver, 32, &pcm(1, 22_050, 16, &little(&wide)), 0);
    let c = front.allocate_file_sample(driver, 48, &pcm(2, 22_050, 8, &tone), 0);
    let door = front.stream_open(22_050, 1);
    front.stream_push(door, &wide);
    front.set_sample_pan(b, 20);
    front.set_sample_volume(c, 90);
    front.set_sample_loop_count(a, 0);
    front.start_sample(a);
    front.start_sample(b);
    front.start_sample(c);
    for reading in (1..200u64).map(|at| at * 1_337) {
        seat.at(reading);
        front.pump();
    }
}

fn the_mix() -> Vec<i16> {
    let recording = Recording::default();
    whole_run(recording.clone());
    recording.frames()
}

fn digest(samples: &[i16]) -> String {
    hex(&sha256(&little(samples)))
}

fn lines_named(lines: &[String], name: &str) -> usize {
    lines
        .iter()
        .filter(|line| line.split(' ').next() == Some(name))
        .count()
}

fn dropped_frames(lines: &[String]) -> usize {
    lines
        .iter()
        .filter(|line| line.starts_with("sound.frames.dropped"))
        .map(|line| {
            line.split(' ')
                .find_map(|part| part.strip_prefix("frames="))
                .and_then(|number| usize::from_str_radix(number, 16).ok())
                .expect("a dropped line carries its frames, in hex")
        })
        .sum()
}

/// A second of held sound: long enough that every test's readings end inside it.
fn long_file() -> Vec<u8> {
    pcm(1, 44_100, 16, &little(&vec![9_000; 44_100]))
}

/// 1,000 frames at 11,025 Hz: over at the reading 90,703.
fn short_file() -> Vec<u8> {
    pcm(1, 11_025, 8, &[128; 1000])
}

#[test]
fn the_bytes_winmm_takes_equal_the_mix_over_a_whole_run() {
    let mix = the_mix();
    assert!(mix.len() > 20_000, "the run wrote {}", mix.len());
    assert!(mix.iter().any(|&sample| sample != 0), "the run was silent");
    for (count, frames) in [(8, 16), (4, 64), (BUFFERS, BUFFER_FRAMES)] {
        let fake = Fake::at_once();
        whole_run(Device::new(fake.clone(), count, frames));
        let books = fake.books();
        assert_eq!(
            digest(&books.taken),
            digest(&mix),
            "taken, {count} of {frames}"
        );
        assert_eq!(
            digest(&books.played),
            digest(&mix),
            "played, {count} of {frames}"
        );
        assert_eq!(books.refusals, [], "{count} of {frames}");
    }
}

#[test]
fn a_whole_run_wraps_its_buffers_and_ends_on_a_short_fill() {
    let fake = Fake::at_once();
    whole_run(Device::new(fake.clone(), 8, 16));
    let books = fake.books();
    let full = 16 * 4;
    assert!(
        books.lengths.contains(&full),
        "no fill ran on into the next buffer"
    );
    let mut distinct = books.written_at.clone();
    distinct.sort_unstable();
    distinct.dedup();
    assert!(distinct.len() > 1, "only one buffer was ever written");
    assert!(
        books.written_at.len() > distinct.len(),
        "no buffer was written twice"
    );
    assert!(
        *books.lengths.last().expect("the run wrote") < full,
        "no short last fill"
    );
}

#[test]
fn a_write_longer_than_a_buffer_runs_on_into_the_next() {
    let fake = Fake::waits();
    let mut back = Device::new(fake.clone(), 3, 4);
    assert_eq!(back.room(), 0, "closed: no room");
    assert_eq!(back.open(MIX_RATE), MIX_RATE);
    assert_eq!(back.room(), 12);
    let frames: Vec<i16> = (0..20).collect();
    back.write(&frames);
    assert_eq!(
        fake.books().lengths,
        [16, 16, 8],
        "2 full buffers and a short one"
    );
    assert_eq!(back.room(), 0, "a short fill holds its whole buffer");
    fake.finish(1);
    assert_eq!(back.room(), 4);
    assert_eq!(fake.books().played, frames[..8]);
    fake.finish(2);
    assert_eq!(back.room(), 12);
    assert_eq!(fake.books().played, frames);
    back.write(&frames[..6]);
    assert_eq!(
        fake.books().unprepared.len(),
        3,
        "a write hands back what is done first"
    );
    assert_eq!(back.room(), 8);
    back.close();
    assert_eq!(back.room(), 0);
    assert_eq!(fake.books().refusals, []);
}

#[test]
fn a_stalled_device_takes_its_room_and_the_front_counts_the_rest() {
    let mix = the_mix();
    let fake = Fake::waits();
    let lines = debug_lines(|| whole_run(Device::new(fake.clone(), 4, 64)));
    let books = fake.books();
    assert_eq!(
        books.lengths.len(),
        4,
        "the stalled queue holds 4 buffers and no more"
    );
    assert_eq!(
        books.taken,
        mix[..books.taken.len()],
        "what it took is the mix's start"
    );
    let dropped = dropped_frames(&lines);
    assert!(dropped > 0);
    assert_eq!(
        books.taken.len() / 2 + dropped,
        mix.len() / 2,
        "each frame taken or counted"
    );
    assert_eq!(books.refusals, []);
}

#[test]
fn a_device_that_catches_up_takes_frames_again() {
    let fake = Fake::waits();
    let (seat, front, driver) = rig(Device::new(fake.clone(), 2, 64));
    let handle = front.allocate_file_sample(driver, 16, &long_file(), 0);
    front.start_sample(handle);
    for reading in [1_000, 2_000, 3_000, 4_000] {
        seat.at(reading);
        front.pump();
    }
    let stalled = fake.books().taken.len();
    assert_eq!(fake.books().lengths.len(), 2);
    fake.finish(2);
    seat.at(5_000);
    front.pump();
    assert!(fake.books().taken.len() > stalled);
    assert_eq!(fake.books().refusals, []);
}

#[test]
fn a_refused_open_falls_to_silence_with_one_warning() {
    let fake = Fake::waits();
    fake.books().refuse_open = 2;
    let seat = Arc::new(Seat::default());
    let front = Front::new(&ROW, seat.clone(), Device::new(fake.clone(), 4, 64));
    let mut handle = 0;
    let lines = debug_lines(|| {
        let (answer, driver) = front.wave_out_open(0, 11_025, 1, 8);
        assert_eq!(answer, 0, "the program goes on");
        handle = front.allocate_file_sample(driver, 16, &short_file(), 0);
        front.register_eos_callback(handle, 64);
        front.start_sample(handle);
        seat.at(90_703);
        assert_eq!(
            front.sample_status(handle),
            SMP_DONE,
            "it ends on time, unheard"
        );
        seat.at(200_000);
        front.pump();
    });
    assert_eq!(lines_named(&lines, "sound.output.refused"), 1, "{lines:?}");
    assert_eq!(lines_named(&lines, "sound.waveout.refused"), 0, "{lines:?}");
    let refused: Vec<_> = lines
        .iter()
        .filter(|line| line.starts_with("sound.waveout.open.refused"))
        .collect();
    assert_eq!(refused.len(), 1, "{lines:?}");
    assert!(refused[0].contains("code=2"), "{}", refused[0]);
    assert_eq!(seat.fired(), [(64, handle)], "its callback fires once");
    let books = fake.books();
    assert_eq!(books.calls, 1, "the open alone reached winmm");
    assert_eq!(books.lengths, [] as [u32; 0]);
}

#[test]
fn the_open_asks_for_stereo_pcm_on_the_mapper_and_no_callback_function() {
    let fake = Fake::at_once();
    let (_seat, _front, _driver) = rig(Device::new(fake.clone(), 4, 64));
    let books = fake.books();
    assert_eq!(books.asked, [(WAVE_MAPPER, 0, 0, CALLBACK_NULL)]);
    let format = books.formats[0];
    assert_eq!({ format.tag }, WAVE_FORMAT_PCM);
    assert_eq!({ format.channels }, 2);
    assert_eq!({ format.rate }, MIX_RATE);
    assert_eq!({ format.bytes_per_second }, MIX_RATE * 4);
    assert_eq!({ format.block_align }, 4);
    assert_eq!({ format.bits }, 16);
    assert_eq!({ format.extra }, 0);
}

#[test]
fn a_callback_that_calls_the_front_again_does_not_hang() {
    within(30, || {
        let fake = Fake::at_once();
        let (seat, front, driver) = rig(Device::new(fake, 4, 64));
        let front = Arc::new(front);
        let handle = front.allocate_file_sample(driver, 16, &short_file(), 0);
        front.register_eos_callback(handle, 64);
        let again = front.clone();
        *seat.on_fire.lock().expect("no test panics holding this") = Some(Box::new(move |fired| {
            assert_eq!(again.sample_status(fired), SMP_DONE);
            again.pump();
        }));
        front.start_sample(handle);
        seat.at(90_703);
        front.pump();
        assert_eq!(seat.fired(), [(64, handle)]);
    });
}

#[test]
fn back_mix_and_clock_on_two_threads_at_volume_never_hang() {
    within(120, || {
        let fake = Fake::waits();
        let (seat, front, driver) = rig(Device::new(fake.clone(), 8, 64));
        let handle = front.allocate_file_sample(driver, 16, &long_file(), 0);
        front.set_sample_loop_count(handle, 0);
        front.start_sample(handle);
        let stop = AtomicBool::new(false);
        thread::scope(|scope| {
            for _ in 0..2 {
                scope.spawn(|| pump_until(&front, &stop, thread::yield_now));
            }
            scope.spawn(|| {
                while !stop.load(Ordering::Acquire) {
                    fake.finish(1);
                    thread::yield_now();
                }
            });
            for step in 1..=8_000u64 {
                seat.at(step * 100);
                assert_eq!(front.sample_status(handle), SMP_PLAYING);
                thread::yield_now();
            }
            stop.store(true, Ordering::Release);
        });
        let books = fake.books();
        assert!(!books.played.is_empty(), "the device finished nothing");
        assert_eq!(books.refusals, []);
        assert_eq!(
            books.played,
            books.taken[..books.played.len()],
            "played as taken, in order"
        );
    });
}

/// `SuspendThread` does not exist on this host, so a parked thread holding the crate's lock
/// stands in for a frozen thread stopped inside it. It is a stand-in, not the freeze.
#[test]
fn a_thread_holding_the_lock_stands_in_for_the_frozen_one() {
    within(60, || {
        let (_seat, front, _driver) = rig(Device::new(Fake::at_once(), 4, 64));
        let front = &front;
        let (held_sender, held) = channel();
        let (free, free_receiver) = channel::<()>();
        let pumps = AtomicU64::new(0);
        let stop = AtomicBool::new(false);
        thread::scope(|scope| {
            scope.spawn(move || {
                front.state.with(|_| {
                    held_sender.send(()).expect("the test is waiting");
                    free_receiver.recv().expect("the test lets go");
                });
            });
            held.recv().expect("the holder holds");
            scope.spawn(|| {
                pump_until(front, &stop, || {
                    pumps.fetch_add(1, Ordering::SeqCst);
                });
            });
            thread::sleep(Duration::from_millis(200));
            assert_eq!(
                pumps.load(Ordering::SeqCst),
                0,
                "a pump moved while the lock was held"
            );
            free.send(()).expect("the holder is waiting");
            while pumps.load(Ordering::SeqCst) < 5 {
                thread::yield_now();
            }
            stop.store(true, Ordering::Release);
        });
    });
}

#[test]
fn the_fake_refuses_what_winmm_documents_refusing() {
    let fake = Fake::waits();
    let mut samples = [0i16; 4];
    let mut header = WaveHdr::new(samples.as_mut_ptr(), 8);
    let at = &raw mut header;
    let format = WaveFormatEx {
        tag: WAVE_FORMAT_PCM,
        channels: 2,
        rate: MIX_RATE,
        bytes_per_second: MIX_RATE * 4,
        block_align: 4,
        bits: 16,
        extra: 0,
    };
    let mut handle = 0;
    // SAFETY: every pointer is to a local alive through the block, and the queue is emptied
    // before the block ends.
    unsafe {
        assert_eq!(
            fake.open(&raw mut handle, WAVE_MAPPER, &raw const format, 0, 0, 0),
            0
        );
        assert_eq!(fake.write(handle, at, HEADER_SIZE), UNPREPARED);
        assert_eq!(fake.prepare(handle, at, HEADER_SIZE), 0);
        assert_eq!(fake.prepare(handle, at, HEADER_SIZE), STRICTER);
        assert_eq!(fake.write(handle, at, HEADER_SIZE), 0);
        assert_eq!(fake.unprepare(handle, at, HEADER_SIZE), STILL_PLAYING);
        assert_eq!(fake.close(handle), STILL_PLAYING);
        assert_eq!(fake.reset(handle), 0);
        assert_ne!(
            flags(at).load(Ordering::Acquire) & WHDR_DONE,
            0,
            "a reset marks done"
        );
        assert_eq!(fake.unprepare(handle, at, HEADER_SIZE), 0);
        assert_eq!(fake.close(handle), 0);
        assert_eq!(fake.write(handle, at, HEADER_SIZE), BAD_HANDLE);
    }
    assert_eq!(fake.books().refusals.len(), 5);
}

/// Two opens with a close between them; `at_close` runs once the first is closed.
fn open_close_open<O: Output>(output: O, at_close: impl FnOnce()) {
    let (seat, front, driver) = rig(output);
    let file = long_file();
    let first = front.allocate_file_sample(driver, 16, &file, 0);
    front.start_sample(first);
    for reading in (1..20u64).map(|at| at * 1_337) {
        seat.at(reading);
        front.pump();
    }
    front.wave_out_close(driver);
    at_close();
    let (answer, driver) = front.wave_out_open(0, 11_025, 1, 8);
    assert_eq!(answer, 0, "a later open works");
    let second = front.allocate_file_sample(driver, 32, &file, 0);
    front.start_sample(second);
    for reading in (20..60u64).map(|at| at * 1_337) {
        seat.at(reading);
        front.pump();
    }
}

#[test]
fn a_close_with_buffers_in_flight_resets_unprepares_each_once_and_closes() {
    let recording = Recording::default();
    let mark = Mutex::new(0);
    open_close_open(recording.clone(), || {
        *mark.lock().expect("no test panics holding this") = recording.frames().len();
    });
    let want = recording.frames();
    let want = &want[*mark.lock().expect("no test panics holding this")..];
    assert!(
        want.iter().any(|&sample| sample != 0),
        "the second open's mix is silent"
    );

    let fake = Fake::waits();
    let at_close = fake.clone();
    let taken_at_close = Mutex::new(0);
    open_close_open(Device::new(fake.clone(), 4, 64), || {
        let mut books = at_close.books();
        assert_eq!(books.lengths.len(), 4, "the 4 buffers were in flight");
        assert_eq!(books.refusals, []);
        assert_eq!(books.closes, 1);
        assert!(books.queue.is_empty());
        assert_eq!(
            books.prepared,
            [] as [usize; 0],
            "a header was never unprepared"
        );
        let mut unprepared = books.unprepared.clone();
        unprepared.sort_unstable();
        let mut written = books.written_at.clone();
        written.sort_unstable();
        assert_eq!(unprepared, written, "each header unprepared once");
        *taken_at_close.lock().expect("no test panics holding this") = books.taken.len();
        books.at_once = true;
    });
    let books = fake.books();
    assert_eq!(books.refusals, []);
    assert_eq!(books.closes, 2, "the back's drop closed the second open");
    let taken_at_close = *taken_at_close.lock().expect("no test panics holding this");
    assert_eq!(
        digest(&books.taken[taken_at_close..]),
        digest(want),
        "the later open's bytes"
    );
}

#[test]
fn a_refused_write_warns_once_and_what_follows_counts_as_dropped() {
    let fake = Fake::waits();
    let (seat, front, driver) = rig(Device::new(fake.clone(), 4, 64));
    fake.books().refuse = Some(("write", 6));
    let handle = front.allocate_file_sample(driver, 16, &long_file(), 0);
    front.start_sample(handle);
    let lines = debug_lines(|| {
        for reading in (1..10u64).map(|at| at * 1_337) {
            seat.at(reading);
            front.pump();
        }
    });
    let refused: Vec<_> = lines
        .iter()
        .filter(|line| line.starts_with("sound.waveout.refused"))
        .collect();
    assert_eq!(refused.len(), 1, "{lines:?}");
    assert!(
        refused[0].contains("call=waveOutWrite") && refused[0].contains("code=6"),
        "{lines:?}"
    );
    assert!(dropped_frames(&lines) > 0, "{lines:?}");
    assert_eq!(fake.books().lengths, [] as [u32; 0]);
    let closes = debug_lines(|| front.shutdown());
    assert_eq!(
        lines_named(&closes, "sound.waveout.closed"),
        1,
        "{closes:?}"
    );
    assert_eq!(
        fake.books().prepared,
        [] as [usize; 0],
        "the refused buffer is unprepared too"
    );
}

#[test]
fn a_refused_close_keeps_its_buffers_with_one_warning() {
    let fake = Fake::waits();
    let (seat, front, driver) = rig(Device::new(fake.clone(), 4, 64));
    let handle = front.allocate_file_sample(driver, 16, &long_file(), 0);
    front.start_sample(handle);
    seat.at(1_337);
    front.pump();
    fake.books().refuse = Some(("close", 6));
    let lines = debug_lines(|| front.wave_out_close(driver));
    let refused: Vec<_> = lines
        .iter()
        .filter(|line| line.starts_with("sound.waveout.refused"))
        .collect();
    assert_eq!(refused.len(), 1, "{lines:?}");
    assert!(refused[0].contains("call=waveOutClose"), "{lines:?}");
    assert_eq!(lines_named(&lines, "sound.waveout.closed"), 0, "{lines:?}");
    fake.books().refuse = None;
    let (answer, _driver) = front.wave_out_open(0, 11_025, 1, 8);
    assert_eq!(answer, 0, "the program goes on");
}

#[test]
fn the_thread_body_pumps_until_told_to_stop() {
    within(30, || {
        let fake = Fake::at_once();
        let (_seat, front, _driver) = rig(Device::new(fake, 4, 64));
        let stop = AtomicBool::new(false);
        let pumps = AtomicU64::new(0);
        let count = || {
            pumps.fetch_add(1, Ordering::SeqCst);
        };
        thread::scope(|scope| {
            scope.spawn(|| pump_until(&front, &stop, count));
            while pumps.load(Ordering::SeqCst) < 5 {
                thread::yield_now();
            }
            stop.store(true, Ordering::Release);
        });
        let at_stop = pumps.load(Ordering::SeqCst);
        pump_until(&front, &stop, count);
        assert_eq!(
            pumps.load(Ordering::SeqCst),
            at_stop,
            "stopped before it starts: no pump"
        );
    });
}

#[test]
fn a_pump_after_shutdown_touches_no_device() {
    let fake = Fake::at_once();
    let (seat, front, driver) = rig(Device::new(fake.clone(), 4, 64));
    let handle = front.allocate_file_sample(driver, 16, &long_file(), 0);
    front.start_sample(handle);
    seat.at(1_337);
    front.pump();
    assert!(!fake.books().lengths.is_empty());
    front.shutdown();
    assert_eq!(fake.books().closes, 1);
    let calls = fake.books().calls;
    for reading in [2_000, 3_000, 200_000] {
        seat.at(reading);
        front.pump();
    }
    assert_eq!(
        fake.books().calls,
        calls,
        "a pump reached winmm after the shutdown"
    );
    assert_eq!(fake.books().refusals, []);
}

#[test]
#[should_panic(expected = "each dividing the mix's chunk")]
fn a_buffer_that_does_not_divide_the_chunk_is_refused_at_its_making() {
    let _ = Device::new(Fake::default(), 2, 300);
}

/// A winmm with no state and a target with none, so that each is `const`, as a `static` holding
/// a front needs; the fake's `Arc` could not be.
struct Still;

impl Winmm for Still {
    unsafe fn open(
        &self,
        _handle: *mut usize,
        _device: u32,
        _format: *const WaveFormatEx,
        _callback: usize,
        _instance: usize,
        _flags: u32,
    ) -> u32 {
        2
    }

    unsafe fn prepare(&self, _handle: usize, _header: *mut WaveHdr, _size: u32) -> u32 {
        2
    }

    unsafe fn write(&self, _handle: usize, _header: *mut WaveHdr, _size: u32) -> u32 {
        2
    }

    unsafe fn unprepare(&self, _handle: usize, _header: *mut WaveHdr, _size: u32) -> u32 {
        2
    }

    unsafe fn reset(&self, _handle: usize) -> u32 {
        2
    }

    unsafe fn close(&self, _handle: usize) -> u32 {
        2
    }
}

struct Quiet;

impl Host for Quiet {
    fn now_us(&self) -> u64 {
        0
    }

    fn call_back(&self, _callback: u32, _handle: u32) {}

    fn read_file(&self, _name: &str) -> Option<u32> {
        None
    }

    fn free_block(&self, _block: u32) {}
}

static HELD: Front<Quiet, Device<Still>> = Front::new(&ROW, Quiet, Device::new(Still, 2, 16));

#[cfg(windows)]
static HELD_REAL: Front<Quiet, crate::waveout::WaveOutBack> =
    Front::new(&ROW, Quiet, crate::waveout::WaveOutBack::new());

#[test]
fn a_static_holds_a_front_over_the_back() {
    assert_eq!(HELD.wave_out_open(0, 11_025, 1, 8).0, 0);
    HELD.pump();
    #[cfg(windows)]
    HELD_REAL.pump();
}
