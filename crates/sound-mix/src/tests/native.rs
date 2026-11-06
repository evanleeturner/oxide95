// SPDX-License-Identifier: MIT
//! The Linux and macOS back on a fake device: no device opens here, and [`fill`] is called
//! as the device's callback would be. Every expected number was worked by hand before the
//! code first ran: the mix runs at 44100, so 10 ms is 441 frames, and a voice's frame `n`
//! holds `n` left and `-n` right at its default volume and pan.

use alloc::boxed::Box;
use alloc::sync::Arc;

use cpal::SampleFormat::{F32, I16, U8};

use super::{little, pcm, rig};
use crate::lock::Lock;
use crate::native::{Native, Shared, choose, fill};
use crate::output::Output;
use crate::ring::RING_FRAMES;
use crate::test_log::debug_lines;
use crate::{SMP_DONE, SMP_PLAYING};

const MIX_HZ: u32 = 44_100;

/// A device's stream. Its end leaves one frame behind, as a last callback's write might,
/// and takes the lock: a close that held the lock across this would never return.
struct FakeStream {
    shared: Arc<Lock<Shared>>,
}

impl Drop for FakeStream {
    fn drop(&mut self) {
        self.shared.with(|shared| shared.ring.push(&[9, 9]));
    }
}

#[allow(clippy::unnecessary_wraps, reason = "the seam's own shape")]
fn taking(_rate_hz: u32, shared: &Arc<Lock<Shared>>) -> Result<Box<dyn Send>, &'static str> {
    Ok(Box::new(FakeStream {
        shared: shared.clone(),
    }))
}

fn refusing(_rate_hz: u32, _shared: &Arc<Lock<Shared>>) -> Result<Box<dyn Send>, &'static str> {
    Err("no device here")
}

/// `count` stereo frames at the mix's rate, numbered from 1.
fn numbered(count: i16) -> Vec<u8> {
    let samples: Vec<i16> = (1..=count).flat_map(|n| [n, -n]).collect();
    pcm(2, MIX_HZ, 16, &little(&samples))
}

fn frames_in(lines: &[String], event: &str) -> Vec<u64> {
    let start = format!("{event} frames=");
    lines
        .iter()
        .filter_map(|line| line.strip_prefix(start.as_str()))
        .map(|rest| {
            let number = rest.split(' ').next().expect("a line has a number");
            u64::from_str_radix(number, 16).expect("frames are hex")
        })
        .collect()
}

fn named<'a>(lines: &'a [String], event: &str) -> Vec<&'a String> {
    lines
        .iter()
        .filter(|line| line.split(' ').next() == Some(event))
        .collect()
}

#[test]
fn nothing_held_means_no_room_and_a_write_keeps_nothing() {
    let mut back = Native::over(taking);
    assert_eq!(back.room(), 0);
    back.write(&[1, 1]);
    assert_eq!(back.open(MIX_HZ), MIX_HZ);
    assert_eq!(
        back.room(),
        RING_FRAMES,
        "the write before the open kept nothing"
    );
}

/// The ring holds 16384 frames. 16383 written leave a room of 1; a write of 3 past that
/// keeps its first alone, so frame 16384 is `2` and the one after it is silence.
#[test]
fn a_write_never_goes_past_the_room() {
    let mut back = Native::over(taking);
    back.open(MIX_HZ);
    back.write(&vec![1; (RING_FRAMES - 1) * 2]);
    assert_eq!(back.room(), 1);
    back.write(&[2, 2, 3, 3, 4, 4]);
    assert_eq!(back.room(), 0);
    let mut out = vec![7_i16; (RING_FRAMES + 1) * 2];
    fill(&back.shared, &mut out);
    assert_eq!(out[RING_FRAMES * 2 - 4..], [1, 1, 2, 2, 0, 0]);
}

/// 10 ms renders 441 frames. A callback asking 443 gets frames 1 to 441 and 2 frames of
/// silence; the next write reports those 2, and the one after has nothing to report.
#[test]
fn frames_come_oldest_first_and_a_short_callback_is_silence_counted() {
    let back = Native::over(taking);
    let shared = back.shared.clone();
    let (seat, front, driver) = rig(back);
    let handle = front.allocate_file_sample(driver, 0x10, &numbered(30_000), 0);
    front.start_sample(handle);
    seat.at(10_000);
    let lines = debug_lines(|| front.pump());
    assert_eq!(frames_in(&lines, "sound.native.underrun"), [] as [u64; 0]);
    let mut out = vec![7_i16; 443 * 2];
    fill(&shared, &mut out);
    assert_eq!(out[..2], [1, -1]);
    assert_eq!(out[880..882], [441, -441]);
    assert_eq!(out[882..], [0, 0, 0, 0]);
    seat.at(20_000);
    let lines = debug_lines(|| front.pump());
    assert_eq!(frames_in(&lines, "sound.native.underrun"), [2]);
    seat.at(30_000);
    let lines = debug_lines(|| front.pump());
    assert_eq!(frames_in(&lines, "sound.native.underrun"), [] as [u64; 0]);
}

/// A second with no callback renders 44100 frames: 16384 fit, 27716 are dropped.
#[test]
fn a_stalled_device_loses_the_newest_frames_and_they_are_counted() {
    let back = Native::over(taking);
    let shared = back.shared.clone();
    let (seat, front, driver) = rig(back);
    let handle = front.allocate_file_sample(driver, 0x10, &numbered(30_000), 0);
    front.start_sample(handle);
    seat.at(1_000_000);
    let lines = debug_lines(|| front.pump());
    assert_eq!(frames_in(&lines, "sound.frames.dropped"), [27_716]);
    assert_eq!(
        front.sample_status(handle),
        SMP_DONE,
        "30,000 frames end inside the second"
    );
    let mut out = vec![7_i16; (RING_FRAMES + 1) * 2];
    fill(&shared, &mut out);
    assert_eq!(out[RING_FRAMES * 2 - 2..], [16_384, -16_384, 0, 0]);
}

#[test]
fn a_float_device_takes_the_same_frames() {
    let mut back = Native::over(taking);
    back.open(MIX_HZ);
    back.write(&[16_384, -16_384]);
    let mut out = [7.0_f32; 4];
    fill(&back.shared, &mut out);
    assert_eq!(
        out.map(f32::to_bits),
        [0.5_f32, -0.5, 0.0, 0.0].map(f32::to_bits),
        "16384 over 32768 is exactly a half, and silence is exactly 0"
    );
}

/// 100 ms is 4410 frames, so the sample ends at 100 ms with no device at all.
#[test]
fn a_refused_open_answers_0_and_the_mix_keeps_its_clock() {
    let mut back = Native::over(refusing);
    let lines = debug_lines(|| assert_eq!(back.open(MIX_HZ), 0));
    assert_eq!(back.room(), 0);
    assert_eq!(named(&lines, "sound.native.open.refused").len(), 1);
    assert_eq!(named(&lines, "sound.native.opened").len(), 0);

    let (seat, front, driver) = rig(back);
    let handle = front.allocate_file_sample(driver, 0x10, &numbered(4410), 0);
    front.register_eos_callback(handle, 0x77);
    front.start_sample(handle);
    seat.at(50_000);
    assert_eq!(front.sample_status(handle), SMP_PLAYING);
    seat.at(100_000);
    assert_eq!(front.sample_status(handle), SMP_DONE);
    assert_eq!(seat.fired(), [(0x77, handle)]);
}

/// The close stops the stream first, so what its end leaves is cleared with the rest: 2
/// written and 1 left are 3 lost, and the reopen finds none of them.
#[test]
fn a_close_empties_the_ring_after_the_stream_ends() {
    let mut back = Native::over(taking);
    back.open(MIX_HZ);
    back.write(&[1, 1, 2, 2]);
    let lines = debug_lines(|| back.close());
    assert_eq!(frames_in(&lines, "sound.native.closed"), [3]);
    assert_eq!(back.room(), 0);
    let lines = debug_lines(|| back.close());
    assert!(
        lines.is_empty(),
        "a second close has nothing to do: {lines:?}"
    );

    assert_eq!(back.open(MIX_HZ), MIX_HZ);
    assert_eq!(back.room(), RING_FRAMES, "0 frames from before the close");
    let mut out = [7_i16; 2];
    fill(&back.shared, &mut out);
    assert_eq!(out, [0, 0]);
}

/// That 1 frame of silence is forgotten with the close that ends this open: the next
/// open's first write has no underrun to report.
#[test]
fn an_open_on_an_open_back_closes_the_old_stream_first() {
    let mut back = Native::over(taking);
    back.open(MIX_HZ);
    let mut out = [7_i16; 2];
    fill(&back.shared, &mut out);
    let lines = debug_lines(|| assert_eq!(back.open(MIX_HZ), MIX_HZ));
    assert_eq!(frames_in(&lines, "sound.native.closed"), [1]);
    assert_eq!(named(&lines, "sound.native.opened").len(), 1);
    assert_eq!(back.room(), RING_FRAMES);
    let lines = debug_lines(|| back.write(&[1, 1]));
    assert_eq!(frames_in(&lines, "sound.native.underrun"), [] as [u64; 0]);
}

#[test]
fn the_format_wanted_is_2_channels_at_the_rate_i16_before_f32() {
    let wide = |format| (2, 8_000, 96_000, format);
    let chosen =
        |rows: &[(u16, u32, u32, cpal::SampleFormat)]| choose(rows.iter().copied(), MIX_HZ);
    assert_eq!(chosen(&[wide(F32), wide(I16)]), Some(I16));
    assert_eq!(chosen(&[wide(I16), wide(F32)]), Some(I16));
    assert_eq!(chosen(&[wide(F32)]), Some(F32));
    assert_eq!(chosen(&[wide(U8)]), None, "neither of the two");
    assert_eq!(chosen(&[(1, 8_000, 96_000, I16)]), None, "one channel");
    assert_eq!(
        chosen(&[(2, 48_000, 48_000, I16)]),
        None,
        "the rate out of range"
    );
    assert_eq!(
        chosen(&[(2, MIX_HZ, MIX_HZ, F32)]),
        Some(F32),
        "the range's ends count"
    );
    assert_eq!(chosen(&[]), None);
}
