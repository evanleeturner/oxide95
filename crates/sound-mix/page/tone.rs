// SPDX-License-Identifier: MIT
//! The thin wasm the page's proof runs: one planted program, two voices built in code,
//! so no sound's bytes lie here and the page needs no player's disc. The short voice
//! must end unheard before any gesture; the looped one is what a gesture lets out.
//! Every export is one call on the crate, and the clock is the page's own.
//!
//! Frames come back by [`frames_ptr`] and the count [`read_frames`] answers: stereo
//! `i16`, good until the next call.
#![expect(
    unsafe_code,
    reason = "`#[unsafe(no_mangle)]` is how a wasm export keeps its name"
)]

use core::cell::{Cell, RefCell};

use sound_mix::page::Page;
use sound_mix::table::Row;
use sound_mix::{Front, Host, RING_FRAMES, SMP_DONE};

static ROW: Row = Row {
    file: "planted",
    module: "planted",
};

const MIX_HZ: u32 = 44_100;
/// 441 Hz is exactly 100 frames a cycle, so a loop of whole cycles joins with no click.
const FRAMES_A_CYCLE: u32 = 100;
const SHORT_FRAMES: u32 = MIX_HZ / 5;
const LOOP_FRAMES: u32 = 44 * FRAMES_A_CYCLE;
/// About a quarter of the full scale: a proof tone, not a loud one.
const LOUDNESS: f64 = 8_192.0;

/// The page's clock. A thread local, since a [`Host`] is handed in by value.
struct PageClock;

thread_local! {
    static NOW_US: Cell<u64> = const { Cell::new(0) };
    static PROGRAM: &'static Program = Box::leak(Box::new(Program::new()));
}

impl Host for PageClock {
    fn now_us(&self) -> u64 {
        NOW_US.with(Cell::get)
    }

    fn call_back(&self, _callback: u32, _handle: u32) {}

    fn read_file(&self, _name: &str) -> Option<u32> {
        None
    }

    fn free_block(&self, _block: u32) {}
}

struct Program {
    page: Page,
    front: Front<PageClock, Page>,
    short: Cell<u32>,
    looped: Cell<u32>,
    frames: RefCell<Vec<i16>>,
}

impl Program {
    fn new() -> Self {
        let page = Page::new();
        let front = Front::new(&ROW, PageClock, page.clone());
        Self {
            page,
            front,
            short: Cell::new(0),
            looped: Cell::new(0),
            frames: RefCell::new(vec![0; RING_FRAMES * 2]),
        }
    }
}

fn with<T>(call: impl FnOnce(&Program) -> T) -> T {
    PROGRAM.with(|program| call(program))
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn set_clock(now_ms: f64) {
    NOW_US.with(|now| now.set((now_ms * 1_000.0) as u64));
}

/// A mono 16-bit file of the tone, `frames` long, at the mix's rate.
#[allow(clippy::cast_possible_truncation)]
fn tone_file(frames: u32) -> Vec<u8> {
    let bytes = frames * 2;
    let mut file = Vec::with_capacity(44 + bytes as usize);
    file.extend_from_slice(b"RIFF");
    file.extend_from_slice(&(36 + bytes).to_le_bytes());
    file.extend_from_slice(b"WAVEfmt ");
    file.extend_from_slice(&16u32.to_le_bytes());
    file.extend_from_slice(&1u16.to_le_bytes());
    file.extend_from_slice(&1u16.to_le_bytes());
    file.extend_from_slice(&MIX_HZ.to_le_bytes());
    file.extend_from_slice(&(MIX_HZ * 2).to_le_bytes());
    file.extend_from_slice(&2u16.to_le_bytes());
    file.extend_from_slice(&16u16.to_le_bytes());
    file.extend_from_slice(b"data");
    file.extend_from_slice(&bytes.to_le_bytes());
    for frame in 0..frames {
        let turn = f64::from(frame % FRAMES_A_CYCLE) / f64::from(FRAMES_A_CYCLE);
        let value = (turn * core::f64::consts::TAU).sin() * LOUDNESS;
        file.extend_from_slice(&(value as i16).to_le_bytes());
    }
    file
}

/// Opens the mix and starts both voices at the page's `now_ms`.
#[unsafe(no_mangle)]
pub extern "C" fn boot(now_ms: f64) {
    set_clock(now_ms);
    with(|program| {
        let front = &program.front;
        let (_, driver) = front.wave_out_open(0, MIX_HZ, 2, 16);
        let short = front.allocate_file_sample(driver, 0x10, &tone_file(SHORT_FRAMES), 0);
        let looped = front.allocate_file_sample(driver, 0x20, &tone_file(LOOP_FRAMES), 0);
        front.set_sample_loop_count(looped, 0);
        front.start_sample(short);
        front.start_sample(looped);
        program.short.set(short);
        program.looped.set(looped);
    });
}

/// The page's frame: the clock moves, and the mix with it.
#[unsafe(no_mangle)]
pub extern "C" fn tick(now_ms: f64) {
    set_clock(now_ms);
    with(|program| program.front.pump());
}

/// The context runs: the mix's frames are kept from this call on.
#[unsafe(no_mangle)]
pub extern "C" fn gesture() {
    with(|program| program.page.start(&program.front));
}

/// Reads what the ring holds; the frames, at [`frames_ptr`].
#[unsafe(no_mangle)]
#[allow(clippy::cast_possible_truncation)]
pub extern "C" fn read_frames() -> u32 {
    with(|program| program.page.read(&mut program.frames.borrow_mut()) as u32)
}

#[unsafe(no_mangle)]
pub extern "C" fn frames_ptr() -> *const i16 {
    with(|program| program.frames.borrow().as_ptr())
}

/// The rate the mix opened at; 0 before [`boot`].
#[unsafe(no_mangle)]
pub extern "C" fn rate() -> u32 {
    with(|program| program.page.rate_hz())
}

/// The frames kept since the gesture. A float, since a wasm `i64` needs a `BigInt`.
#[unsafe(no_mangle)]
#[allow(clippy::cast_precision_loss)]
pub extern "C" fn kept() -> f64 {
    with(|program| program.page.kept() as f64)
}

/// The clock the program last read, in ms.
#[unsafe(no_mangle)]
#[allow(clippy::cast_precision_loss)]
pub extern "C" fn clock_ms() -> f64 {
    NOW_US.with(Cell::get) as f64 / 1_000.0
}

/// 1 once the short voice has ended, by the clock alone.
#[unsafe(no_mangle)]
pub extern "C" fn short_ended() -> u32 {
    with(|program| u32::from(program.front.sample_status(program.short.get()) == SMP_DONE))
}

/// Stops the looped voice, so nothing sounds and the frames end.
#[unsafe(no_mangle)]
pub extern "C" fn hush() {
    with(|program| program.front.stop_sample(program.looped.get()));
}
