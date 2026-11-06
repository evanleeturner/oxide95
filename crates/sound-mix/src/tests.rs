// SPDX-License-Identifier: MIT
//! The front against a driven clock and a back that keeps what it is handed.
//! The mixes' expected numbers were worked by hand before the code first ran;
//! the replay's order and answers are a logged run's, handles mapped by use.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::table::{FRONTS, Row, STOPS};
use crate::test_log::debug_lines;
use crate::wave::{self, WaveError};
use crate::{Front, Host, NullBack, Output, SMP_DONE, SMP_PLAYING, SMP_STOPPED};

static ROW: Row = Row { file: "a program", module: "MSS32" };

/// The target, its clock driven by the test.
#[derive(Default)]
struct Seat {
    now_us: AtomicU64,
    fired: Mutex<Vec<(u32, u32)>>,
    freed: Mutex<Vec<u32>>,
    next_block: Mutex<Option<u32>>,
    on_fire: Mutex<Option<Box<dyn FnOnce(u32) + Send>>>,
}

impl Host for Arc<Seat> {
    fn now_us(&self) -> u64 {
        self.now_us.load(Ordering::Relaxed)
    }

    fn call_back(&self, callback: u32, handle: u32) {
        self.fired.lock().expect("no test panics holding this").push((callback, handle));
        let call = self.on_fire.lock().expect("no test panics holding this").take();
        if let Some(call) = call {
            call(handle);
        }
    }

    fn read_file(&self, _name: &str) -> Option<u32> {
        *self.next_block.lock().expect("no test panics holding this")
    }

    fn free_block(&self, block: u32) {
        self.freed.lock().expect("no test panics holding this").push(block);
    }
}

impl Seat {
    fn at(&self, now_us: u64) {
        self.now_us.store(now_us, Ordering::Relaxed);
    }

    fn fired(&self) -> Vec<(u32, u32)> {
        self.fired.lock().expect("no test panics holding this").clone()
    }

    fn hand_out(&self, block: Option<u32>) {
        *self.next_block.lock().expect("no test panics holding this") = block;
    }
}

#[derive(Default)]
struct Seen {
    frames: Vec<i16>,
    opens: u32,
    closes: u32,
}

/// A back that keeps its frames, with room and a taken rate set by the test.
#[derive(Clone)]
struct Back {
    seen: Arc<Mutex<Seen>>,
    room: usize,
    took: Option<u32>,
}

impl Default for Back {
    fn default() -> Self {
        Self { seen: Arc::default(), room: usize::MAX, took: None }
    }
}

impl Back {
    fn frames(&self) -> Vec<i16> {
        self.seen.lock().expect("no test panics holding this").frames.clone()
    }

    fn closes(&self) -> u32 {
        self.seen.lock().expect("no test panics holding this").closes
    }
}

impl Output for Back {
    fn open(&mut self, rate_hz: u32) -> u32 {
        self.seen.lock().expect("no test panics holding this").opens += 1;
        self.took.unwrap_or(rate_hz)
    }

    fn room(&self) -> usize {
        self.room
    }

    fn write(&mut self, frames: &[i16]) {
        assert!(frames.len() / 2 <= self.room, "the front wrote past the room it was told");
        self.seen.lock().expect("no test panics holding this").frames.extend_from_slice(frames);
    }

    fn close(&mut self) {
        self.seen.lock().expect("no test panics holding this").closes += 1;
    }
}

/// A front with its driver open at the clock's 0.
fn rig<O: Output>(output: O) -> (Arc<Seat>, Front<Arc<Seat>, O>, u32) {
    let seat = Arc::new(Seat::default());
    let front = Front::new(&ROW, seat.clone(), output);
    let (answer, driver) = front.wave_out_open(0, 11_025, 1, 8);
    assert_eq!(answer, 0);
    assert_ne!(driver, 0);
    (seat, front, driver)
}

#[allow(clippy::cast_possible_truncation)]
fn wav(tag: u16, channels: u16, rate_hz: u32, bits: u16, data: &[u8]) -> Vec<u8> {
    let block = channels * bits / 8;
    let mut file = Vec::new();
    file.extend_from_slice(b"RIFF");
    file.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    file.extend_from_slice(b"WAVEfmt ");
    file.extend_from_slice(&16u32.to_le_bytes());
    file.extend_from_slice(&tag.to_le_bytes());
    file.extend_from_slice(&channels.to_le_bytes());
    file.extend_from_slice(&rate_hz.to_le_bytes());
    file.extend_from_slice(&(rate_hz * u32::from(block)).to_le_bytes());
    file.extend_from_slice(&block.to_le_bytes());
    file.extend_from_slice(&bits.to_le_bytes());
    file.extend_from_slice(b"data");
    file.extend_from_slice(&(data.len() as u32).to_le_bytes());
    file.extend_from_slice(data);
    file
}

fn pcm(channels: u16, rate_hz: u32, bits: u16, data: &[u8]) -> Vec<u8> {
    wav(1, channels, rate_hz, bits, data)
}

fn little(samples: &[i16]) -> Vec<u8> {
    samples.iter().flat_map(|sample| sample.to_le_bytes()).collect()
}

/// `count` frames of one held pair.
fn held(left: i16, right: i16, count: usize) -> Vec<i16> {
    [left, right].repeat(count)
}

/// The front's panic and the lines written on its way, for a call that must stop.
fn stops(call: impl FnOnce()) -> Vec<String> {
    debug_lines(|| {
        let caught = catch_unwind(AssertUnwindSafe(call));
        assert!(caught.is_err(), "this call must end the program");
    })
}

// 1: Placed.

#[test]
fn the_lists_hold_29_and_15_names_none_twice() {
    let mut all: Vec<&str> = FRONTS.iter().chain(STOPS.iter()).copied().collect();
    all.sort_unstable();
    all.dedup();
    assert_eq!((FRONTS.len(), STOPS.len(), all.len()), (29, 15, 44));
}

#[test]
fn every_front_writes_one_line_of_its_own_name_and_this_crate() {
    let (seat, front, _driver) = rig(NullBack);
    seat.hand_out(Some(0x40));
    let file = pcm(1, 11_025, 8, &[128; 8]);
    let lines = debug_lines(|| {
        front.startup();
        let (_, opened) = front.wave_out_open(0, 11_025, 1, 8);
        front.serve();
        front.set_preference(1, 2);
        let handle = front.allocate_sample_handle(opened);
        front.init_sample(handle);
        front.set_sample_volume(handle, 1);
        front.set_sample_loop_count(handle, 1);
        front.set_sample_file(handle, 0x10, &file, u32::MAX);
        front.sample_status(handle);
        let other = front.allocate_file_sample(opened, 0x10, &file, u32::MAX);
        front.set_sample_type(other, 0, 0);
        front.set_sample_playback_rate(handle, 22_050);
        front.set_sample_pan(handle, 64);
        front.start_sample(handle);
        front.stop_sample(handle);
        front.resume_sample(handle);
        front.end_sample(handle);
        front.set_sample_user_data(handle, 0, 9);
        front.sample_user_data(handle, 0);
        front.register_eos_callback(handle, 7);
        front.load_sample_buffer(other, 0, 0x20, &[1, 2]);
        front.sample_buffer_ready(other);
        let block = front.file_read(0x30, "a name", 0);
        front.mem_free_lock(block);
        front.midi_out_open(0);
        front.release_sample_handle(handle);
        front.wave_out_close(opened);
        front.shutdown();
    });
    let mut named = Vec::new();
    for line in &lines {
        assert!(line.starts_with("imp.call c=MSS32!"), "{line}");
        assert!(line.ends_with(" by=sound-mix"), "{line}");
        let name = line["imp.call c=MSS32!".len()..].split(' ').next().expect("a line has a name");
        named.push(name.to_owned());
    }
    named.sort_unstable();
    let mut wanted: Vec<String> = FRONTS.iter().map(ToString::to_string).collect();
    wanted.sort_unstable();
    assert_eq!(named, wanted);
}

/// `by` sits at each front, not in a shared row: the source says so.
#[test]
fn each_front_writes_by_where_its_answer_is_computed() {
    let code = include_str!("lib.rs");
    assert_eq!(code.matches("by = \"sound-mix\"; \"imp.call\"").count(), FRONTS.len());
}

#[test]
fn each_stop_panics_after_one_line_naming_it() {
    for name in STOPS {
        let (_seat, front, _driver) = rig(NullBack);
        let lines = stops(|| front.named_stop(name));
        assert_eq!(
            lines,
            [format!("sound.named-stop c=MSS32!{name} why=unreached by=sound-mix")]
        );
    }
}

#[test]
fn a_handle_not_live_stops_loudly() {
    let (_seat, front, _driver) = rig(NullBack);
    let lines = stops(|| {
        front.sample_status(0x1234);
    });
    assert_eq!(lines, ["sound.not-live c=MSS32!AIL_sample_status a=1234 by=sound-mix"]);
}

#[test]
fn a_released_handle_is_not_live_and_its_slot_comes_back() {
    let (_seat, front, driver) = rig(NullBack);
    let first = front.allocate_sample_handle(driver);
    let second = front.allocate_sample_handle(driver);
    assert_eq!(second - first, 16);
    front.release_sample_handle(first);
    assert_eq!(front.allocate_sample_handle(driver), first);
    front.release_sample_handle(second);
    let lines = stops(|| front.start_sample(second));
    assert_eq!(lines.len(), 1, "{lines:?}");
}

#[test]
fn a_driver_not_open_allocates_nothing() {
    let (_seat, front, driver) = rig(NullBack);
    assert_eq!(front.allocate_sample_handle(0), 0);
    assert_eq!(front.allocate_file_sample(0, 0x10, &pcm(1, 11_025, 8, &[128; 2]), 0), 0);
    front.wave_out_close(driver);
    assert_eq!(front.allocate_sample_handle(driver), 0);
    let (_, reopened) = front.wave_out_open(0, 11_025, 1, 8);
    assert_ne!(reopened, driver, "each open is a new number");
    assert_ne!(front.allocate_sample_handle(reopened), 0);
}

#[test]
fn old_values_come_back_and_user_words_hold() {
    let (_seat, front, driver) = rig(NullBack);
    assert_eq!(front.set_preference(3, 10), 0);
    assert_eq!(front.set_preference(3, 20), 10);
    let handle = front.allocate_sample_handle(driver);
    assert_eq!(front.register_eos_callback(handle, 0x77), 0);
    assert_eq!(front.register_eos_callback(handle, 0x88), 0x77);
    front.set_sample_user_data(handle, 7, 0xabc);
    assert_eq!(front.sample_user_data(handle, 7), 0xabc);
    assert_eq!(front.midi_out_open(0), 6);
    let lines = stops(|| {
        front.sample_user_data(handle, 8);
    });
    assert_eq!(lines, ["sound.named-stop c=MSS32!AIL_sample_user_data why=user-word-past-8 by=sound-mix"]);
}

// 3: Harvested. A logged run's calls in logged order, its driver and handles mapped.

#[test]
fn the_logged_calls_replay_in_their_order_with_their_answers() {
    let (seat, front, driver) = rig(NullBack);
    let short = pcm(1, 11_025, 8, &[128; 11_025]);
    let (mut first, mut second, mut third) = (0, 0, 0);
    let lines = debug_lines(|| {
        first = front.allocate_sample_handle(driver);
        front.init_sample(first);
        assert_eq!(front.set_sample_file(first, 0x1700e48, &short, 0xffff_ffff), 1);
        front.set_sample_volume(first, 0x1e);
        front.set_sample_loop_count(first, 0);
        front.set_sample_loop_count(first, 0);
        front.set_sample_volume(first, 0x1e);
        front.start_sample(first);
        second = front.allocate_sample_handle(driver);
        front.init_sample(second);
        assert_eq!(front.set_sample_file(second, 0x170ba48, &short, 0xffff_ffff), 1);
        front.set_sample_volume(second, 0x32);
        third = front.allocate_sample_handle(driver);
        front.init_sample(third);
        assert_eq!(front.set_sample_file(third, 0x170d180, &short, 0xffff_ffff), 1);
        front.set_sample_volume(third, 0x32);
        seat.hand_out(Some(0x8b_c2f0));
        assert_eq!(front.file_read(0x31_f048, "a-sound", 0), 0x8b_c2f0);
        seat.hand_out(None);
        assert_eq!(front.file_read(0x31_ed38, "no-such-sound", 0), 0);
        front.set_sample_loop_count(third, 1);
        front.set_sample_volume(third, 0x32);
        seat.at(1_000);
        front.start_sample(third);
        assert_eq!(front.sample_status(third), 4);
        seat.at(1_001_000);
        assert_eq!(front.sample_status(third), 2);
        assert_eq!(front.sample_status(first), 4, "loop count 0 is forever");
        front.stop_sample(first);
        front.release_sample_handle(first);
        front.stop_sample(third);
        front.release_sample_handle(third);
    });
    let call = |name: &str, tail: String| format!("imp.call c=MSS32!{name} {tail} by=sound-mix");
    let wanted = [
        call("AIL_allocate_sample_handle", format!("a={driver:x} v={first:x}")),
        call("AIL_init_sample", format!("a={first:x}")),
        call("AIL_set_sample_file", format!("a={first:x},1700e48,ffffffff v=1")),
        call("AIL_set_sample_volume", format!("a={first:x},1e")),
        call("AIL_set_sample_loop_count", format!("a={first:x},0")),
        call("AIL_set_sample_loop_count", format!("a={first:x},0")),
        call("AIL_set_sample_volume", format!("a={first:x},1e")),
        call("AIL_start_sample", format!("a={first:x}")),
        call("AIL_allocate_sample_handle", format!("a={driver:x} v={second:x}")),
        call("AIL_init_sample", format!("a={second:x}")),
        call("AIL_set_sample_file", format!("a={second:x},170ba48,ffffffff v=1")),
        call("AIL_set_sample_volume", format!("a={second:x},32")),
        call("AIL_allocate_sample_handle", format!("a={driver:x} v={third:x}")),
        call("AIL_init_sample", format!("a={third:x}")),
        call("AIL_set_sample_file", format!("a={third:x},170d180,ffffffff v=1")),
        call("AIL_set_sample_volume", format!("a={third:x},32")),
        call("FILE_read", "a=31f048,0 v=8bc2f0 name=a-sound".to_owned()),
        call("FILE_read", "a=31ed38,0 v=0 name=no-such-sound".to_owned()),
        call("AIL_set_sample_loop_count", format!("a={third:x},1")),
        call("AIL_set_sample_volume", format!("a={third:x},32")),
        call("AIL_start_sample", format!("a={third:x}")),
        call("AIL_sample_status", format!("a={third:x} v=4")),
        call("AIL_sample_status", format!("a={third:x} v=2")),
        call("AIL_sample_status", format!("a={first:x} v=4")),
        call("AIL_stop_sample", format!("a={first:x}")),
        call("AIL_release_sample_handle", format!("a={first:x}")),
        call("AIL_stop_sample", format!("a={third:x}")),
        call("AIL_release_sample_handle", format!("a={third:x}")),
    ];
    assert_eq!(lines, wanted);
}

// 4: The clock.

const READINGS: [u64; 5] = [0, 1_000, 90_702, 90_703, 200_000];

/// 1000 frames at 11025 Hz: 90702.947 microseconds, so 90703 is the first reading
/// at or past the end.
fn statuses<O: Output>(output: O) -> (Vec<u32>, Vec<String>) {
    let (seat, front, driver) = rig(output);
    let handle = front.allocate_file_sample(driver, 0x10, &pcm(1, 11_025, 8, &[128; 1000]), 0);
    let mut seen = Vec::new();
    let lines = debug_lines(|| {
        front.start_sample(handle);
        for reading in READINGS {
            seat.at(reading);
            seen.push(front.sample_status(handle));
        }
    });
    (seen, lines)
}

#[test]
fn a_sample_ends_at_the_first_reading_past_its_length() {
    let (seen, lines) = statuses(NullBack);
    assert_eq!(seen, [SMP_PLAYING, SMP_PLAYING, SMP_PLAYING, SMP_DONE, SMP_DONE]);
    assert!(lines.iter().all(|line| !line.contains("frames-dropped")));
}

#[test]
fn a_sample_of_whole_microseconds_ends_on_its_length_exactly() {
    let (seat, front, driver) = rig(NullBack);
    let handle = front.allocate_file_sample(driver, 0x10, &pcm(1, 11_025, 8, &[128; 11_025]), 0);
    front.start_sample(handle);
    seat.at(999_999);
    assert_eq!(front.sample_status(handle), SMP_PLAYING);
    seat.at(1_000_000);
    assert_eq!(front.sample_status(handle), SMP_DONE);
}

/// Hand-worked: 1000 frames at 11025 are 4000 at the mix's 44100, and the readings
/// cross 44, 3955 and 1 of them.
#[test]
fn a_refusing_back_changes_no_status_and_each_refused_frame_is_reported() {
    let back = Back { room: 0, ..Back::default() };
    let (seen, lines) = statuses(back.clone());
    assert_eq!(seen, statuses(NullBack).0);
    assert!(back.frames().is_empty());
    let dropped: Vec<u64> = lines
        .iter()
        .filter_map(|line| line.strip_prefix("sound.frames-dropped frames="))
        .map(|rest| {
            let number = rest.split(' ').next().expect("a line has a number");
            u64::from_str_radix(number, 16).expect("frames are hex")
        })
        .collect();
    assert_eq!(dropped, [44, 3955, 1]);
}

#[test]
fn a_full_back_takes_its_room_and_the_rest_is_reported() {
    let back = Back { room: 10, ..Back::default() };
    let (seat, front, driver) = rig(back.clone());
    let handle = front.allocate_file_sample(driver, 0x10, &pcm(1, 44_100, 8, &[128; 1000]), 0);
    front.start_sample(handle);
    seat.at(1_000);
    let lines = debug_lines(|| front.pump());
    assert_eq!(back.frames().len(), 20, "10 frames, 2 numbers each");
    assert_eq!(lines, ["sound.frames-dropped frames=22 by=sound-mix"], "44 less the 10");
}

#[test]
fn a_stop_holds_the_clock_and_a_resume_goes_on() {
    let (seat, front, driver) = rig(NullBack);
    let handle = front.allocate_file_sample(driver, 0x10, &pcm(1, 11_025, 8, &[128; 11_025]), 0);
    front.start_sample(handle);
    seat.at(400_000);
    front.stop_sample(handle);
    assert_eq!(front.sample_status(handle), SMP_STOPPED);
    seat.at(5_000_000);
    assert_eq!(front.sample_status(handle), SMP_STOPPED);
    front.resume_sample(handle);
    seat.at(5_599_000);
    assert_eq!(front.sample_status(handle), SMP_PLAYING);
    seat.at(5_600_001);
    assert_eq!(front.sample_status(handle), SMP_DONE);
}

#[test]
fn a_new_rate_moves_the_end_from_where_the_voice_is() {
    let (seat, front, driver) = rig(NullBack);
    let handle = front.allocate_file_sample(driver, 0x10, &pcm(1, 11_025, 8, &[128; 11_025]), 0);
    front.start_sample(handle);
    seat.at(500_000);
    front.set_sample_playback_rate(handle, 22_050);
    seat.at(750_022);
    assert_eq!(front.sample_status(handle), SMP_PLAYING, "5513 frames left at 22050 are 250022.7 us");
    seat.at(750_023);
    assert_eq!(front.sample_status(handle), SMP_DONE);
}

#[test]
fn a_refused_open_warns_once_and_the_program_goes_on() {
    for took in [0, 22_050] {
        let back = Back { took: Some(took), ..Back::default() };
        let seat = Arc::new(Seat::default());
        let front = Front::new(&ROW, seat.clone(), back.clone());
        let mut opened = 0;
        let lines = debug_lines(|| opened = front.wave_out_open(0, 11_025, 1, 8).1);
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert_eq!(lines[1], format!("sound.output-refused took={took:x} by=sound-mix"));
        let handle = front.allocate_file_sample(opened, 0x10, &pcm(1, 11_025, 8, &[128; 11]), 0);
        front.start_sample(handle);
        seat.at(1_000);
        let quiet = debug_lines(|| {
            assert_eq!(front.sample_status(handle), SMP_DONE);
        });
        assert_eq!(quiet.len(), 1, "no dropped line: the warning said it: {quiet:?}");
        assert!(back.frames().is_empty());
    }
}

// 5: The mix.

#[test]
fn each_format_the_archives_hold_parses_and_plays_its_length() {
    let formats = [
        (1, 22_050, 16),
        (1, 11_025, 8),
        (1, 22_050, 8),
        (2, 22_050, 16),
        (1, 44_100, 8),
        (2, 11_025, 8),
    ];
    for (channels, rate_hz, bits) in formats {
        let data = vec![128u8; 100 * usize::from(channels) * usize::from(bits) / 8];
        let file = pcm(channels, rate_hz, bits, &data);
        let sound = wave::parse(&file).expect("a made file parses");
        assert_eq!(
            (sound.format, sound.channels, sound.rate_hz, sound.bits, sound.data.len()),
            (1, channels, rate_hz, bits, data.len())
        );
        let (seat, front, driver) = rig(NullBack);
        let handle = front.allocate_file_sample(driver, 0x10, &file, 0);
        front.start_sample(handle);
        let end = (100 * 1_000_000u64).div_ceil(u64::from(rate_hz));
        seat.at(end - 1);
        assert_eq!(front.sample_status(handle), SMP_PLAYING, "{channels} {rate_hz} {bits}");
        seat.at(end);
        assert_eq!(front.sample_status(handle), SMP_DONE, "{channels} {rate_hz} {bits}");
    }
}

#[test]
fn a_file_the_mix_could_not_play_is_a_named_stop() {
    let not_pcm = wav(2, 1, 22_050, 4, &[0; 8]);
    let wide = pcm(1, 22_050, 24, &[0; 9]);
    let many = pcm(6, 22_050, 16, &[0; 12]);
    for file in [not_pcm.as_slice(), b"not a wave at all", wide.as_slice(), many.as_slice()] {
        let (_seat, front, driver) = rig(NullBack);
        let handle = front.allocate_sample_handle(driver);
        let lines = stops(|| {
            front.set_sample_file(handle, 0x10, file, 0);
        });
        assert_eq!(
            lines,
            ["sound.named-stop c=MSS32!AIL_set_sample_file why=not-pcm-wave by=sound-mix"]
        );
    }
    assert_eq!(wave::parse(b"RIFFxxxxWAVE").err(), Some(WaveError::NoFormat));
}

#[test]
fn a_parse_walks_past_other_chunks_and_an_odd_one_pads() {
    let mut file = b"RIFF\0\0\0\0WAVELIST\x03\0\0\0abc\0".to_vec();
    file.extend_from_slice(&pcm(1, 11_025, 8, &[7, 8])[12..]);
    let sound = wave::parse(&file).expect("a file with a LIST chunk parses");
    assert_eq!(sound.data, [7, 8]);
}

/// Hand-worked: `(byte - 128) << 8` is 0, 16384, -16384, 32512; 11025 into 44100
/// holds each for 4 frames; volume 127 at pan 64 is unity both ways. 363 us is 16
/// frames, and past it nothing more is written.
#[test]
fn a_hand_worked_mix_comes_out_to_the_sample() {
    let back = Back::default();
    let (seat, front, driver) = rig(back.clone());
    let handle = front.allocate_file_sample(driver, 0x10, &pcm(1, 11_025, 8, &[128, 192, 64, 255]), 0);
    front.start_sample(handle);
    seat.at(363);
    front.pump();
    seat.at(1_000);
    front.pump();
    let mut wanted = held(0, 0, 4);
    wanted.extend(held(16_384, 16_384, 4));
    wanted.extend(held(-16_384, -16_384, 4));
    wanted.extend(held(32_512, 32_512, 4));
    assert_eq!(back.frames(), wanted);
}

/// Hand-worked: pan 127 leaves left 0 and right at gain 64 * 64 over 127 * 64, so
/// 16384 * 4096 / 8128 = 8256, 32512 gives 16384, and -16384 truncates to -8256.
#[test]
fn volume_and_pan_scale_by_hand_worked_numbers() {
    let back = Back::default();
    let (seat, front, driver) = rig(back.clone());
    let handle = front.allocate_file_sample(driver, 0x10, &pcm(1, 11_025, 8, &[128, 192, 64, 255]), 0);
    front.set_sample_volume(handle, 64);
    front.set_sample_pan(handle, 127);
    front.start_sample(handle);
    seat.at(363);
    front.pump();
    let mut wanted = held(0, 0, 4);
    wanted.extend(held(0, 8_256, 4));
    wanted.extend(held(0, -8_256, 4));
    wanted.extend(held(0, 16_384, 4));
    assert_eq!(back.frames(), wanted);
}

/// Hand-worked: 16-bit at the mix's rate gives 1000, -2000; 8-bit stereo at half it
/// gives left 256 and right -256 for both frames. 46 us is 2 frames.
#[test]
fn two_voices_of_different_shapes_sum() {
    let back = Back::default();
    let (seat, front, driver) = rig(back.clone());
    let a = front.allocate_file_sample(driver, 0x10, &pcm(1, 44_100, 16, &little(&[1_000, -2_000])), 0);
    let b = front.allocate_file_sample(driver, 0x20, &pcm(2, 22_050, 8, &[129, 127]), 0);
    front.start_sample(a);
    front.start_sample(b);
    seat.at(46);
    front.pump();
    assert_eq!(back.frames(), [1_256, 744, -1_744, -2_256]);
}

#[test]
fn a_sum_past_the_width_clamps_and_never_wraps() {
    for (sample, clamped) in [(30_000, i16::MAX), (-30_000, i16::MIN)] {
        let back = Back::default();
        let (seat, front, driver) = rig(back.clone());
        let file = pcm(1, 44_100, 16, &little(&[sample]));
        let a = front.allocate_file_sample(driver, 0x10, &file, 0);
        let b = front.allocate_file_sample(driver, 0x20, &file, 0);
        front.start_sample(a);
        front.start_sample(b);
        seat.at(23);
        front.pump();
        assert_eq!(back.frames(), [clamped, clamped]);
    }
}

/// A set of voices and a door, moved through set readings.
#[allow(clippy::cast_possible_truncation)]
fn reference_mix() -> Vec<i16> {
    let tone: Vec<u8> = (0..4_000u32).map(|at| (at * 7 % 256) as u8).collect();
    let wide: Vec<i16> = (0..3_000i32).map(|at| (at * 11 % 30_000) as i16).collect();
    let back = Back::default();
    let (seat, front, driver) = rig(back.clone());
    let a = front.allocate_file_sample(driver, 0x10, &pcm(1, 11_025, 8, &tone), 0);
    let b = front.allocate_file_sample(driver, 0x20, &pcm(1, 22_050, 16, &little(&wide)), 0);
    let c = front.allocate_file_sample(driver, 0x30, &pcm(2, 22_050, 8, &tone), 0);
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
    back.frames()
}

#[test]
fn two_runs_of_the_reference_mix_hash_the_same() {
    assert_eq!(
        hex(&sha256(b"abc")),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        "the test's own hash, against FIPS 180's example"
    );
    let first = reference_mix();
    assert!(first.len() > 20_000, "the reference mix wrote {}", first.len());
    assert!(first.iter().any(|&sample| sample != 0));
    let bytes = |frames: &[i16]| sha256(&little(frames));
    assert_eq!(hex(&bytes(&first)), hex(&bytes(&reference_mix())));
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// FIPS 180-4's SHA-256, so the test needs no outside crate.
fn sha256(message: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a_2f98, 0x7137_4491, 0xb5c0_fbcf, 0xe9b5_dba5, 0x3956_c25b, 0x59f1_11f1, 0x923f_82a4,
        0xab1c_5ed5, 0xd807_aa98, 0x1283_5b01, 0x2431_85be, 0x550c_7dc3, 0x72be_5d74, 0x80de_b1fe,
        0x9bdc_06a7, 0xc19b_f174, 0xe49b_69c1, 0xefbe_4786, 0x0fc1_9dc6, 0x240c_a1cc, 0x2de9_2c6f,
        0x4a74_84aa, 0x5cb0_a9dc, 0x76f9_88da, 0x983e_5152, 0xa831_c66d, 0xb003_27c8, 0xbf59_7fc7,
        0xc6e0_0bf3, 0xd5a7_9147, 0x06ca_6351, 0x1429_2967, 0x27b7_0a85, 0x2e1b_2138, 0x4d2c_6dfc,
        0x5338_0d13, 0x650a_7354, 0x766a_0abb, 0x81c2_c92e, 0x9272_2c85, 0xa2bf_e8a1, 0xa81a_664b,
        0xc24b_8b70, 0xc76c_51a3, 0xd192_e819, 0xd699_0624, 0xf40e_3585, 0x106a_a070, 0x19a4_c116,
        0x1e37_6c08, 0x2748_774c, 0x34b0_bcb5, 0x391c_0cb3, 0x4ed8_aa4a, 0x5b9c_ca4f, 0x682e_6ff3,
        0x748f_82ee, 0x78a5_636f, 0x84c8_7814, 0x8cc7_0208, 0x90be_fffa, 0xa450_6ceb, 0xbef9_a3f7,
        0xc671_78f2,
    ];
    let mut state: [u32; 8] = [
        0x6a09_e667, 0xbb67_ae85, 0x3c6e_f372, 0xa54f_f53a, 0x510e_527f, 0x9b05_688c, 0x1f83_d9ab,
        0x5be0_cd19,
    ];
    let mut padded = message.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&(message.len() as u64 * 8).to_be_bytes());
    for block in padded.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (word, bytes) in w.iter_mut().zip(block.chunks_exact(4)) {
            *word = u32::from_be_bytes(bytes.try_into().expect("chunks of 4"));
        }
        for t in 16..64 {
            let s0 = w[t - 15].rotate_right(7) ^ w[t - 15].rotate_right(18) ^ (w[t - 15] >> 3);
            let s1 = w[t - 2].rotate_right(17) ^ w[t - 2].rotate_right(19) ^ (w[t - 2] >> 10);
            w[t] = w[t - 16].wrapping_add(s0).wrapping_add(w[t - 7]).wrapping_add(s1);
        }
        let mut v = state;
        for t in 0..64 {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7].wrapping_add(s1).wrapping_add(ch).wrapping_add(K[t]).wrapping_add(w[t]);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);
            v = [t1.wrapping_add(t2), v[0], v[1], v[2], v[3].wrapping_add(t1), v[4], v[5], v[6]];
        }
        for (held, moved) in state.iter_mut().zip(v) {
            *held = held.wrapping_add(moved);
        }
    }
    let mut digest = [0u8; 32];
    for (bytes, word) in digest.chunks_exact_mut(4).zip(state) {
        bytes.copy_from_slice(&word.to_be_bytes());
    }
    digest
}

/// Finds every WAVE in the archives named by `SOUND_MIX_ARCHIVES`, colon-separated,
/// and tallies the parse. No archive enters the repo, so this runs by hand.
///
/// The walk steps over a find's header, not its declared size: a size can run past the
/// next WAVE and would hide it. Each size that does is printed with its overrun.
#[test]
#[ignore = "needs a program's archives, named by SOUND_MIX_ARCHIVES"]
fn every_wave_in_the_named_archives_parses_as_a_row_of_the_table() {
    let paths = std::env::var("SOUND_MIX_ARCHIVES").expect("SOUND_MIX_ARCHIVES names the archives");
    let mut tally = std::collections::BTreeMap::new();
    for path in paths.split(':') {
        let bytes = std::fs::read(path).expect("each named archive reads");
        let mut archive_tally = std::collections::BTreeMap::new();
        let mut finds: Vec<(usize, usize)> = Vec::new();
        let mut at = 0;
        while at + 12 <= bytes.len() {
            if &bytes[at..at + 4] == b"RIFF" && &bytes[at + 8..at + 12] == b"WAVE" {
                let size = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().expect("4 bytes"));
                let end = (at + 8 + size as usize).min(bytes.len());
                let sound = wave::parse(&bytes[at..end]).expect("each WAVE found parses");
                let row = (sound.format, sound.channels, sound.rate_hz, sound.bits);
                *tally.entry(row).or_insert(0u32) += 1;
                *archive_tally.entry(row).or_insert(0u32) += 1;
                finds.push((at, end));
                at += 12;
            } else {
                at += 1;
            }
        }
        println!("{path}: {} WAVEs", finds.len());
        for (row, count) in &archive_tally {
            println!("  {row:?} {count}");
        }
        for pair in finds.windows(2) {
            let ((start, end), (next, _)) = (pair[0], pair[1]);
            if end > next {
                println!(
                    "  size runs over: at {start:#x} declared end {end:#x} next {next:#x} over by {}",
                    end - next
                );
            }
        }
    }
    println!("both:");
    for (row, count) in &tally {
        println!("{row:?} {count}");
    }
    println!("total {}", tally.values().sum::<u32>());
    let rows = [
        (1, 1, 22_050, 16),
        (1, 1, 11_025, 8),
        (1, 1, 22_050, 8),
        (1, 2, 22_050, 16),
        (1, 1, 44_100, 8),
        (1, 2, 11_025, 8),
    ];
    assert!(tally.keys().all(|row| rows.contains(row)), "{tally:?}");
}

// 6: The stream.

/// A stream voice with 4 frames in each slot: 138 and 148 are 2560 and 5120.
fn stream_of_two<O: Output>(front: &Front<Arc<Seat>, O>, driver: u32) -> u32 {
    let handle = front.allocate_sample_handle(driver);
    front.init_sample(handle);
    front.set_sample_type(handle, 0, 0);
    assert_eq!(front.sample_buffer_ready(handle), 0);
    front.load_sample_buffer(handle, 0, 0x10, &[138; 4]);
    assert_eq!(front.sample_buffer_ready(handle), 1);
    front.load_sample_buffer(handle, 1, 0x20, &[148; 4]);
    assert_eq!(front.sample_buffer_ready(handle), -1);
    handle
}

#[test]
fn the_double_buffer_hands_its_slots_in_turn() {
    let (seat, front, driver) = rig(NullBack);
    let handle = stream_of_two(&front, driver);
    front.start_sample(handle);
    seat.at(363);
    assert_eq!(front.sample_buffer_ready(handle), 0, "the clock passed the first slot");
    front.load_sample_buffer(handle, 0, 0x30, &[138; 4]);
    assert_eq!(front.sample_buffer_ready(handle), -1, "the second still sounds");
    seat.at(726);
    assert_eq!(front.sample_buffer_ready(handle), 1, "the clock passed the second");
    assert_eq!(front.sample_status(handle), SMP_PLAYING);
}

#[test]
fn one_long_pass_crosses_both_slots_once_each() {
    let back = Back::default();
    let (seat, front, driver) = rig(back.clone());
    let handle = stream_of_two(&front, driver);
    front.start_sample(handle);
    seat.at(726);
    front.pump();
    let mut wanted = held(2_560, 2_560, 16);
    wanted.extend(held(5_120, 5_120, 16));
    assert_eq!(back.frames(), wanted);
    assert_eq!(front.sample_status(handle), SMP_DONE);
}

#[test]
fn a_callers_change_after_the_load_is_not_heard() {
    let back = Back::default();
    let (seat, front, driver) = rig(back.clone());
    let handle = front.allocate_sample_handle(driver);
    let mut buffer = [138u8; 4];
    front.load_sample_buffer(handle, 0, 0x10, &buffer);
    buffer.fill(200);
    assert_eq!(buffer, [200; 4]);
    front.start_sample(handle);
    seat.at(363);
    front.pump();
    assert_eq!(back.frames(), held(2_560, 2_560, 16));
}

#[test]
fn a_stopped_stream_keeps_its_place_in_the_slot() {
    let (seat, front, driver) = rig(NullBack);
    let handle = stream_of_two(&front, driver);
    front.start_sample(handle);
    seat.at(181);
    front.stop_sample(handle);
    seat.at(10_000);
    front.resume_sample(handle);
    seat.at(10_100);
    assert_eq!(front.sample_buffer_ready(handle), -1, "not yet through the first slot");
    seat.at(10_272);
    assert_eq!(front.sample_buffer_ready(handle), -1, "3 frames were left: 272.1 us");
    seat.at(10_273);
    assert_eq!(front.sample_buffer_ready(handle), 0, "through it, with the time before the stop");
}

#[test]
fn a_sample_ends_once_and_fires_once() {
    let (seat, front, driver) = rig(NullBack);
    let handle = front.allocate_file_sample(driver, 0x10, &pcm(1, 11_025, 8, &[128; 11]), 0);
    front.register_eos_callback(handle, 0x77);
    front.start_sample(handle);
    seat.at(997);
    front.pump();
    assert_eq!(seat.fired(), [], "11 frames at 11025 are 997.7 us");
    seat.at(998);
    front.pump();
    assert_eq!(seat.fired(), [(0x77, handle)]);
    seat.at(2_000);
    front.pump();
    front.end_sample(handle);
    assert_eq!(seat.fired(), [(0x77, handle)], "ended already: no second firing");
}

#[test]
fn a_stream_ends_once_and_fires_once() {
    let (seat, front, driver) = rig(NullBack);
    let handle = stream_of_two(&front, driver);
    front.register_eos_callback(handle, 0x77);
    front.start_sample(handle);
    seat.at(725);
    front.pump();
    assert_eq!(seat.fired(), []);
    seat.at(10_000);
    front.pump();
    front.pump();
    assert_eq!(seat.fired(), [(0x77, handle)]);
}

#[test]
fn an_end_of_a_playing_or_stopped_voice_fires_and_a_done_one_does_not() {
    let (seat, front, driver) = rig(NullBack);
    let file = pcm(1, 11_025, 8, &[128; 11_025]);
    let playing = front.allocate_file_sample(driver, 0x10, &file, 0);
    let stopped = front.allocate_file_sample(driver, 0x10, &file, 0);
    let done = front.allocate_file_sample(driver, 0x10, &file, 0);
    for (handle, callback) in [(playing, 1), (stopped, 2), (done, 3)] {
        front.register_eos_callback(handle, callback);
    }
    front.start_sample(playing);
    front.start_sample(stopped);
    front.stop_sample(stopped);
    front.end_sample(playing);
    front.end_sample(stopped);
    front.end_sample(done);
    assert_eq!(seat.fired(), [(1, playing), (2, stopped)]);
}

#[test]
fn a_callback_may_call_back_in_the_lock_being_free() {
    let (seat, front, driver) = rig(NullBack);
    let front = Arc::new(front);
    let handle = front.allocate_file_sample(driver, 0x10, &pcm(1, 11_025, 8, &[128; 11]), 0);
    front.register_eos_callback(handle, 0x77);
    front.set_sample_user_data(handle, 0, 0xfeed);
    front.start_sample(handle);
    let (sent, heard) = channel();
    let again = front.clone();
    *seat.on_fire.lock().expect("no test panics holding this") = Some(Box::new(move |handle| {
        sent.send(again.sample_user_data(handle, 0)).expect("the test is waiting");
    }));
    seat.at(1_000);
    let pumping = front.clone();
    std::thread::spawn(move || pumping.pump());
    let word = heard.recv_timeout(Duration::from_secs(5));
    assert_eq!(word.expect("a hang here means the lock was held across the callback"), 0xfeed);
}

// 7: The door.

/// Hand-worked: door 100, 200, 300, 400 at the mix's rate over a voice of 1000:
/// 46 us is 2 frames summed, then the close, then 92 us is 2 of the door alone.
#[test]
fn a_door_goes_on_past_the_close_and_a_miles_voice_does_not() {
    let back = Back::default();
    let (seat, front, driver) = rig(back.clone());
    let handle = front.allocate_file_sample(driver, 0x10, &pcm(1, 44_100, 16, &little(&[1_000; 2])), 0);
    front.set_sample_loop_count(handle, 0);
    let door = front.stream_open(44_100, 1);
    front.stream_push(door, &[100, 200, 300, 400]);
    front.start_sample(handle);
    seat.at(46);
    front.pump();
    front.wave_out_close(driver);
    assert_eq!(front.sample_status(handle), SMP_DONE, "its handle still answers");
    assert_eq!(back.closes(), 0, "a door lives, so the output stays open");
    seat.at(92);
    front.pump();
    assert_eq!(back.frames(), [1_100, 1_100, 1_200, 1_200, 300, 300, 400, 400]);
    front.stream_close(door);
    assert_eq!(back.closes(), 1, "the last door takes the output with it");
}

/// Hand-worked: stereo at half the mix's rate holds each pair for 2 frames.
#[test]
fn a_door_drains_by_its_own_rate_and_keeps_late_sound() {
    let back = Back::default();
    let seat = Arc::new(Seat::default());
    let front = Front::new(&ROW, seat.clone(), back.clone());
    let door = front.stream_open(22_050, 2);
    front.stream_push(door, &[10, 20, 30, 40]);
    seat.at(91);
    front.pump();
    assert_eq!(back.frames(), [10, 20, 10, 20, 30, 40, 30, 40]);
    front.stream_push(door, &[50, 60]);
    seat.at(137);
    front.pump();
    assert_eq!(back.frames()[8..], [50, 60, 50, 60]);
}

#[test]
fn shutdown_clears_the_door_and_a_closed_door_is_not_live() {
    let back = Back::default();
    let (_seat, front, _driver) = rig(back.clone());
    let door = front.stream_open(44_100, 1);
    front.shutdown();
    assert_eq!(back.closes(), 1);
    let lines = stops(|| front.stream_push(door, &[1]));
    assert_eq!(lines, [format!("sound.not-live c=MSS32!stream_push a={door:x} by=sound-mix")]);
}

#[test]
fn a_door_of_no_rate_or_six_channels_stops() {
    for (rate_hz, channels) in [(0, 1), (44_100, 6)] {
        let (_seat, front, _driver) = rig(NullBack);
        let lines = stops(|| {
            front.stream_open(rate_hz, channels);
        });
        assert_eq!(lines, ["sound.named-stop c=MSS32!stream_open why=door-shape by=sound-mix"]);
    }
}

// 8: The pair.

#[test]
fn the_pair_frees_what_it_handed_out() {
    let (seat, front, _driver) = rig(NullBack);
    seat.hand_out(Some(0x8b_c2f0));
    let block = front.file_read(0x10, "a name", 0);
    assert_eq!(block, 0x8b_c2f0);
    front.mem_free_lock(block);
    front.mem_free_lock(0);
    assert_eq!(*seat.freed.lock().expect("no test panics holding this"), [block]);
}

#[test]
fn a_block_not_handed_out_stops() {
    let (seat, front, _driver) = rig(NullBack);
    let lines = stops(|| front.mem_free_lock(0x1234));
    assert_eq!(lines, ["sound.free-unknown c=MSS32!MEM_free_lock a=1234 by=sound-mix"]);
    assert!(seat.freed.lock().expect("no test panics holding this").is_empty());
}

#[test]
fn a_second_free_stops() {
    let (seat, front, _driver) = rig(NullBack);
    seat.hand_out(Some(0x40));
    let block = front.file_read(0x10, "a name", 0);
    front.mem_free_lock(block);
    let lines = stops(|| front.mem_free_lock(block));
    assert_eq!(lines, ["sound.free-unknown c=MSS32!MEM_free_lock a=40 by=sound-mix"]);
    assert_eq!(seat.freed.lock().expect("no test panics holding this").len(), 1);
}

#[test]
fn a_destination_of_the_callers_own_stops() {
    let (_seat, front, _driver) = rig(NullBack);
    let lines = stops(|| {
        front.file_read(0x10, "a name", 0x20);
    });
    assert_eq!(lines, ["sound.named-stop c=MSS32!FILE_read why=destination-not-0 by=sound-mix"]);
}

// Stops beside the formats.

#[test]
fn a_sample_type_mss_does_not_name_stops() {
    for (format, flags) in [(4, 0), (0, 1)] {
        let (_seat, front, driver) = rig(NullBack);
        let handle = front.allocate_sample_handle(driver);
        let lines = stops(|| front.set_sample_type(handle, format, flags));
        assert_eq!(lines, ["sound.named-stop c=MSS32!AIL_set_sample_type why=format-unnamed by=sound-mix"]);
    }
}

#[test]
fn a_third_slot_stops() {
    let (_seat, front, driver) = rig(NullBack);
    let handle = front.allocate_sample_handle(driver);
    let lines = stops(|| front.load_sample_buffer(handle, 2, 0x10, &[1]));
    assert_eq!(lines, ["sound.named-stop c=MSS32!AIL_load_sample_buffer why=slot-past-2 by=sound-mix"]);
}
