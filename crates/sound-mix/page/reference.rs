// SPDX-License-Identifier: MIT
//! The thin wasm that runs the reference script on the page: the same 3 voices and the same
//! worked answer as every other route, so the page's stream is judged by what the test's is.
//! The script's clock stands still until the gesture; the first frame after that sets frame 0,
//! so a bench can finish laying its path before the first frame moves.
//!
//! Frames come back by [`frames_ptr`] and the count [`read_frames`] answers: stereo `i16`, good
//! until the next call.
#![expect(
    unsafe_code,
    reason = "`#[unsafe(no_mangle)]` is how a wasm export keeps its name"
)]

use core::cell::{Cell, OnceCell, RefCell};

use sound_mix::RING_FRAMES;
use sound_mix::page::Page;

#[path = "../bench/reference.rs"]
mod reference;

use reference::{Outcome, Plant, Script};

thread_local! {
    static PROGRAM: OnceCell<Program> = const { OnceCell::new() };
}

struct Program {
    page: Page,
    script: Script<Page>,
    /// The page's time at the first frame after the gesture; `None` holds the clock at 0.
    anchor_ms: Cell<Option<f64>>,
    gestured: Cell<bool>,
    clock_ms: Cell<f64>,
    frames: RefCell<Vec<i16>>,
    worked: Vec<i16>,
    sentence: RefCell<String>,
}

/// Every export but [`boot`] needs the program [`boot`] made, and the page calls that first.
fn with<T>(call: impl FnOnce(&Program) -> T) -> T {
    PROGRAM.with(|program| call(program.get().expect("the page calls boot first")))
}

fn outcome() -> Outcome {
    with(|program| program.script.outcome())
}

/// Loads the script with the plant `code` names: 0 none, then silence, flip, swap and rate. An
/// unknown code is no plant, so the bench's own reading of the stream shows what ran.
#[unsafe(no_mangle)]
pub extern "C" fn boot(code: u32) {
    let plant = match code {
        1 => Plant::Silence,
        2 => Plant::Flip,
        3 => Plant::Swap,
        4 => Plant::Rate,
        _ => Plant::Clean,
    };
    let page = Page::new();
    let script = Script::load(page.clone(), plant);
    PROGRAM.with(|program| {
        program.get_or_init(|| Program {
            page,
            script,
            anchor_ms: Cell::new(None),
            gestured: Cell::new(false),
            clock_ms: Cell::new(0.0),
            frames: RefCell::new(vec![0; RING_FRAMES * 2]),
            worked: reference::worked(),
            sentence: RefCell::new(String::new()),
        });
    });
}

/// The context runs: frames are kept from this call on, and the voices start.
#[unsafe(no_mangle)]
pub extern "C" fn gesture() {
    with(|program| {
        program.page.start(program.script.front());
        program.script.start();
        program.gestured.set(true);
    });
}

/// The page's frame: the clock moves by the page's time since its first frame, and the mix with it.
#[unsafe(no_mangle)]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub extern "C" fn tick(now_ms: f64) {
    with(|program| {
        if !program.gestured.get() {
            return;
        }
        let anchor = program.anchor_ms.get().unwrap_or(now_ms);
        program.anchor_ms.set(Some(anchor));
        let clock = now_ms - anchor;
        program.clock_ms.set(clock);
        program.script.run_to((clock * 1_000.0) as u64);
    });
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

/// The rate the mix opened at.
#[unsafe(no_mangle)]
pub extern "C" fn rate() -> u32 {
    with(|program| program.page.rate_hz())
}

/// The time the script's clock was last given, in ms. It runs past the script's end, where the
/// script clamps it.
#[unsafe(no_mangle)]
pub extern "C" fn clock_ms() -> f64 {
    with(|program| program.clock_ms.get())
}

/// The page's own count of the frames it kept. A float, since a wasm `i64` needs a `BigInt`.
#[unsafe(no_mangle)]
#[allow(clippy::cast_precision_loss)]
pub extern "C" fn kept() -> f64 {
    with(|program| program.page.kept() as f64)
}

/// The bench's count of the same frames, from the room each write found. A float, as [`kept`] is.
#[unsafe(no_mangle)]
#[allow(clippy::cast_precision_loss)]
pub extern "C" fn against_kept() -> f64 {
    outcome().kept as f64
}

/// The frames the witness saw. A float, as [`kept`] is.
#[unsafe(no_mangle)]
#[allow(clippy::cast_precision_loss)]
pub extern "C" fn witness_frames() -> f64 {
    outcome().frames as f64
}

/// The witness's hash. A wasm `i64`, which the page reads as a signed `BigInt`, so the page takes
/// it through `BigInt.asUintN(64, hash)`.
#[unsafe(no_mangle)]
pub extern "C" fn witness_hash() -> u64 {
    outcome().hash
}

/// The first frame unlike the worked answer, -1 with none. A float, as [`kept`] is.
#[unsafe(no_mangle)]
#[allow(clippy::cast_precision_loss)]
pub extern "C" fn first_wrong() -> f64 {
    outcome().first_wrong.map_or(-1.0, |frame| frame as f64)
}

/// The frames unlike the worked answer. A float, as [`kept`] is.
#[unsafe(no_mangle)]
#[allow(clippy::cast_precision_loss)]
pub extern "C" fn wrong() -> f64 {
    outcome().wrong as f64
}

/// The block the first wrong frame fell in, -1 with none. A float, as [`kept`] is.
#[unsafe(no_mangle)]
#[allow(clippy::cast_precision_loss)]
pub extern "C" fn first_wrong_block() -> f64 {
    outcome().block().map_or(-1.0, |block| block as f64)
}

/// Makes the red sentence and answers its length in bytes, 0 with nothing wrong. Call this, then
/// [`sentence_ptr`].
#[unsafe(no_mangle)]
#[allow(clippy::cast_possible_truncation)]
pub extern "C" fn sentence_len() -> u32 {
    with(|program| {
        let mut sentence = program.sentence.borrow_mut();
        *sentence = program.script.outcome().sentence().unwrap_or_default();
        sentence.len() as u32
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn sentence_ptr() -> *const u8 {
    with(|program| program.sentence.borrow().as_ptr())
}

/// The worked answer, interleaved stereo, no plant in it, for the bench's tap to be compared to.
#[unsafe(no_mangle)]
pub extern "C" fn worked_ptr() -> *const i16 {
    with(|program| program.worked.as_ptr())
}

/// The worked answer's frames.
#[unsafe(no_mangle)]
pub extern "C" fn worked_frames() -> u32 {
    reference::FRAMES
}
