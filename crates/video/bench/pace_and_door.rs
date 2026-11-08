// SPDX-License-Identifier: MIT
//! One clip played through the fronts on the host's own clock, for a scorer in another
//! process to time. This program's `Instant` is what the fronts read, so it cannot judge
//! them: each line below is flushed the moment it happens, and the scorer's stamp on its
//! arrival is the stopwatch (`pace_and_door.py`).
//!
//! ```text
//! pace_and_door <clip> <debug log> <samples out>
//! ```
//!
//! On stdout, each `e` this program's microseconds, read only to size the pipe's lag:
//!
//! - `open <e>`, just before `smack_open`, whose clock read anchors the clip.
//! - `handle <width> <height> <frames> <in track> <in other tracks>`.
//! - `beat <e>`, in a spin, once 1 ms has passed since the last line.
//! - `due <frame> <not due answers> <e>`, the moment `smack_wait` says so.
//! - `end <waits> <clock reads> <plays true> <opens> <pushes> <closes> <samples> <rate> <channels>`.
//!
//! The debug log takes the fronts' lines, 2 a wait, so a spin's million never reach the
//! pipe. The samples file takes every push above the door. Past the door sits a mixer over
//! an output that keeps nothing: no real output is built in.
#![allow(
    unsafe_code,
    reason = "a caller's part: a buffer named by pointer, a handle read through one"
)]

use std::cell::Cell;
use std::env;
use std::fs::{self, File};
use std::io::{self, BufWriter, StdoutLock, Write};
use std::path::Path;
use std::process;
use std::time::{Duration, Instant};

use log::LevelFilter;
use oxide_logline::{Roots, install};
use sound_mix::output::Output;
use sound_mix::table::Row;
use sound_mix::{Front, Host};
use video::{Choices, ClipBytes, Clock, MixDoor, Place, SoundDoor, Track, Videos};

/// The library's bit for a clip's first track.
const TRACK_1_BIT: u32 = 13;
const TRACK_1: u32 = 1 << TRACK_1_BIT;
/// How long a spin goes without a line: far under any period, so the longest gap
/// between the scorer's stamps bounds the longest single call.
const BEAT: Duration = Duration::from_millis(1);

static ROW: Row = Row {
    file: "a program",
    module: "MSS32",
};

fn since(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_micros()).expect("a run is far shorter than u64 microseconds")
}

/// What the fronts read: this host's clock, and a count of its reads.
struct HostClock {
    start: Instant,
    reads: Cell<u64>,
}

impl Clock for HostClock {
    fn now_us(&self) -> u64 {
        self.reads.set(self.reads.get() + 1);
        since(self.start)
    }
}

/// A clip's bytes, by the path it was opened under.
struct Disk;

impl ClipBytes for Disk {
    fn read(&mut self, name: &[u8]) -> Option<Vec<u8>> {
        fs::read(Path::new(std::str::from_utf8(name).ok()?)).ok()
    }
}

/// The mix's clock.
struct Seat(Instant);

impl Host for Seat {
    fn now_us(&self) -> u64 {
        since(self.0)
    }

    fn call_back(&self, _callback: u32, _handle: u32) {}

    fn read_file(&self, _name: &str) -> Option<u32> {
        None
    }

    fn free_block(&self, _block: u32) {}
}

/// Past the door, where this program judges nothing: frames taken and let go.
struct Quiet;

impl Output for Quiet {
    fn open(&mut self, rate_hz: u32) -> u32 {
        rate_hz
    }

    fn room(&self) -> usize {
        usize::MAX
    }

    fn write(&mut self, _frames: &[i16]) {}

    fn close(&mut self) {}
}

/// Above the door: writes every push down and counts every call, then forwards each
/// to the real door, whose body is one line of forwarding.
struct Spy<'a> {
    door: MixDoor<'a, Seat, Quiet>,
    samples_out: File,
    plays_true: Cell<u64>,
    opens: u64,
    pushes: u64,
    closes: u64,
    samples: u64,
    rate_hz: u32,
    channels: u8,
}

impl SoundDoor for Spy<'_> {
    fn plays(&self) -> bool {
        let answer = self.door.plays();
        if answer {
            self.plays_true.set(self.plays_true.get() + 1);
        }
        answer
    }

    fn stream_open(&mut self, track: Track) -> u32 {
        self.opens += 1;
        self.rate_hz = track.rate_hz;
        self.channels = track.channels;
        self.door.stream_open(track)
    }

    fn stream_push(&mut self, stream: u32, samples: &[i16]) {
        self.pushes += 1;
        self.samples += samples.len() as u64;
        let bytes: Vec<u8> = samples.iter().flat_map(|one| one.to_le_bytes()).collect();
        self.samples_out
            .write_all(&bytes)
            .expect("the samples file takes a push");
        self.door.stream_push(stream, samples);
    }

    fn stream_close(&mut self, stream: u32) {
        self.closes += 1;
        self.door.stream_close(stream);
    }
}

/// The scorer's pipe: a line is on its way before this returns.
struct Pipe(StdoutLock<'static>);

impl Pipe {
    fn say(&mut self, line: &str) {
        writeln!(self.0, "{line}").expect("the scorer's pipe takes a line");
        self.0.flush().expect("the scorer's pipe flushes");
    }
}

/// Plays the clip as a caller's loop plays it: spin on `smack_wait`, paint, step on.
/// Every frame is painted, as a frame stepped past is never heard.
fn play(videos: &mut Videos<HostClock, Disk, Spy<'_>>, clip: &[u8], start: Instant) {
    let mut pipe = Pipe(io::stdout().lock());
    pipe.say(&format!("open {}", since(start)));
    let handle = videos.smack_open(clip, TRACK_1, 0);
    if handle.is_null() {
        pipe.say("refused");
        process::exit(3);
    }
    // SAFETY: not null, so it is the block `smack_open` gave, live until `smack_close`.
    let (width, height, frames) = unsafe {
        let block = &*handle;
        (block.width, block.height, block.frames)
    };
    let cells = usize::try_from(u64::from(width) * u64::from(height)).expect("a clip fits");
    let mut pixels = vec![0_u8; cells];
    let place = Place {
        x: 0,
        y: 0,
        pitch: width,
        height,
    };
    // SAFETY: `pixels` holds pitch * height bytes, and neither it nor its allocation
    // moves before `smack_close`.
    unsafe {
        videos.smack_to_buffer(handle, place, pixels.as_mut_ptr(), 0);
    }
    let in_track = videos.smack_sound_in_track(handle, TRACK_1);
    let in_others: u32 = (0..32)
        .filter(|bit| *bit != TRACK_1_BIT)
        .map(|bit| videos.smack_sound_in_track(handle, 1 << bit))
        .sum();
    pipe.say(&format!(
        "handle {width} {height} {frames} {in_track} {in_others}"
    ));

    let mut waits = 0_u64;
    let mut last_line = Instant::now();
    for frame in 0..frames {
        let mut not_due = 0_u64;
        while videos.smack_wait(handle) != 0 {
            not_due += 1;
            if last_line.elapsed() >= BEAT {
                pipe.say(&format!("beat {}", since(start)));
                last_line = Instant::now();
            }
        }
        pipe.say(&format!("due {frame} {not_due} {}", since(start)));
        last_line = Instant::now();
        waits += not_due + 1;
        videos.smack_do_frame(handle);
        videos.smack_next_frame(handle);
    }
    videos.smack_close(handle);

    let reads = videos.clock_mut().reads.get();
    let spy = videos.sound();
    pipe.say(&format!(
        "end {waits} {reads} {} {} {} {} {} {} {}",
        spy.plays_true.get(),
        spy.opens,
        spy.pushes,
        spy.closes,
        spy.samples,
        spy.rate_hz,
        spy.channels,
    ));
}

fn main() {
    let arguments: Vec<String> = env::args().collect();
    let [_, clip, debug_log, samples_out] = arguments.as_slice() else {
        eprintln!("usage: pace_and_door <clip> <debug log> <samples out>");
        process::exit(2);
    };
    let here = env::current_dir().expect("the run has a folder");
    let roots = Roots {
        job: &here,
        base: &here,
        cwd: &here,
    };
    let log_file = BufWriter::new(File::create(debug_log).expect("the debug log opens"));
    install("pace_and_door", &roots, log_file, LevelFilter::Debug).expect("the first logger");

    let start = Instant::now();
    let mix = Front::new(&ROW, Seat(start), Quiet);
    let spy = Spy {
        door: MixDoor::new(&mix),
        samples_out: File::create(samples_out).expect("the samples file opens"),
        plays_true: Cell::new(0),
        opens: 0,
        pushes: 0,
        closes: 0,
        samples: 0,
        rate_hz: 0,
        channels: 0,
    };
    let clock = HostClock {
        start,
        reads: Cell::new(0),
    };
    let mut videos = Videos::new(clock, Disk, spy, Choices::default());
    play(&mut videos, clip.as_bytes(), start);
    log::logger().flush();
}
