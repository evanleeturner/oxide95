// SPDX-License-Identifier: MIT
//! The thin wasm a page's script runs: the worked script over `draw`'s page back, each
//! export one step, so the frames can go by batches and the script reads the picture from
//! outside. It holds no check. Lines come back by [`lines_ptr`] and [`lines_len`], UTF-8,
//! good until the next call.
//!
//! Beside the script stands a second desk with the keys over it, leaked at first use: a page's
//! listener hands it what the browser raised, and the script reads the front's answers from
//! outside. It shares nothing with [`setup`], which a key's read never calls.
#![expect(
    unsafe_code,
    reason = "`#[unsafe(no_mangle)]` is how a wasm export keeps its name"
)]

use core::cell::RefCell;

use draw::easel::Easel;
use draw::page::Page;
use window::Event;
use window::desk::Desk;
use window::page::Page as Host;
use window_input::Input;
use window_input::front::Front;
use window_input::remap::{self, Chord, Remap};
use window_input::table::{Calls, Row};

#[path = "../bench/worked_script.rs"]
mod worked_script;

use worked_script::{CELLS, Check, HEIGHT, Plant, Script, Terms, WIDTH};

const WINDOW: u32 = 1;
const RGBA_BYTES: usize = CELLS * 4;
/// The links only the page's script can read: an export's number is an index here.
const OUTSIDE_LINKS: [&str; 4] = ["page-read", "canvas", "screenshot", "memory-bytes"];
/// [`remap_read`]'s answer: the key in the low byte, then these 2 bits.
const CTRL_BIT: u32 = 8;
const ALT_BIT: u32 = 9;
/// The one remap row a page reads, and its planted wrong form, by [`remap_read`]'s `plant`.
const ROWS: [Remap; 2] = [
    Remap {
        kept: Chord::ctrl(b'W'),
        pressed: Chord::ctrl(b'E'),
    },
    Remap {
        kept: Chord::ctrl(b'T'),
        pressed: Chord::ctrl(b'E'),
    },
];

static CALLS: Calls = Calls {
    get_cursor_pos: "GetCursorPos",
    set_cursor_pos: "SetCursorPos",
    show_cursor: "ShowCursor",
    load_cursor: "LoadCursorA",
    clip_cursor: "ClipCursor",
    get_async_key_state: "GetAsyncKeyState",
    get_key_state: "GetKeyState",
    map_virtual_key: "MapVirtualKeyA",
};
static ROW: Row = Row {
    file: "surfaces_page",
    calls: &CALLS,
};

type Rig = Script<'static, &'static Page, Desk<Host>>;

/// The keys' desk and its front.
struct KeyDesk {
    desk: &'static Desk<Input<Host>>,
    front: Front<'static, Host>,
}

struct Run {
    page: &'static Page,
    script: Rig,
    rgba: Vec<u8>,
    reference: Vec<u8>,
}

thread_local! {
    static RUN: RefCell<Option<Run>> = const { RefCell::new(None) };
    static KEYS: KeyDesk = {
        let desk: &'static Desk<Input<Host>> =
            Box::leak(Box::new(Desk::new(Input::new(Host::new(640, 480)))));
        KeyDesk { desk, front: Front::new(&ROW, desk) }
    };
}

/// A virtual key as an event holds it; a script's number past a byte is a fault of the script.
fn key_byte(key: u32) -> u8 {
    u8::try_from(key).expect("invariant: a virtual key is under 256")
}

fn with<T>(step: impl FnOnce(&mut Run) -> T) -> T {
    RUN.with(|run| {
        step(
            run.borrow_mut()
                .as_mut()
                .expect("invariant: setup is first"),
        )
    })
}

/// Every call up to the picture; `check` and `plant` are indexes into the script's lists.
#[unsafe(no_mangle)]
pub extern "C" fn setup(check: u32, plant: u32, frames: u32) {
    let page: &'static Page = Box::leak(Box::new(Page::new()));
    let easel: &'static Easel<&'static Page> = Box::leak(Box::new(Easel::new(page, WIDTH, HEIGHT)));
    let desk: &'static Desk<Host> = Box::leak(Box::new(Desk::new(Host::new(640, 480))));
    let terms = Terms {
        arm: "page",
        check: Check::ALL[check as usize],
        plant: Plant::ALL[plant as usize],
        window: WINDOW,
        frames,
        rss: || None,
    };
    let run = Run {
        page,
        script: Script::new(terms, easel, desk),
        rgba: vec![0; RGBA_BYTES],
        reference: Vec::new(),
    };
    RUN.with(|cell| *cell.borrow_mut() = Some(run));
}

/// `count` frames of the loop.
#[unsafe(no_mangle)]
pub extern "C" fn frames(count: u32) {
    with(|run| run.script.frames(count));
}

/// The script's reads, and what the picture must be.
#[unsafe(no_mangle)]
pub extern "C" fn finish() {
    with(|run| {
        run.script.finish();
        run.reference = run.script.reference_rgba();
    });
}

/// `Page::read` into the buffer [`rgba_ptr`] names: 1 for a new picture.
#[unsafe(no_mangle)]
pub extern "C" fn read() -> u32 {
    with(|run| u32::from(run.page.read(&mut run.rgba)))
}

#[unsafe(no_mangle)]
pub extern "C" fn rgba_ptr() -> *const u8 {
    with(|run| run.rgba.as_ptr())
}

#[unsafe(no_mangle)]
pub extern "C" fn reference_ptr() -> *const u8 {
    with(|run| run.reference.as_ptr())
}

/// One read the page's script made, as a line like the rest.
#[unsafe(no_mangle)]
pub extern "C" fn line(link: u32, differ: u32, of: u32) {
    with(|run| {
        run.script.line(
            OUTSIDE_LINKS[link as usize],
            u64::from(differ),
            u64::from(of),
        );
    });
}

/// The frames' time, as the page's script read it.
#[unsafe(no_mangle)]
pub extern "C" fn rate(milliseconds: u32) {
    with(|run| run.script.rate(u64::from(milliseconds)));
}

#[unsafe(no_mangle)]
pub extern "C" fn lines_ptr() -> *const u8 {
    with(|run| run.script.lines().as_ptr())
}

#[unsafe(no_mangle)]
pub extern "C" fn lines_len() -> usize {
    with(|run| run.script.lines().len())
}

/// The browser raised `key`, down when `down` is not 0: queued for the keys' next pump.
#[unsafe(no_mangle)]
pub extern "C" fn key_hand(key: u32, down: u32) {
    let event = Event::Key {
        window: WINDOW,
        key: key_byte(key),
        down: down != 0,
    };
    KEYS.with(|keys| keys.desk.screen().back().hand(event));
}

/// The page took or lost the user's attention, as `key_hand` queues a key.
#[unsafe(no_mangle)]
pub extern "C" fn key_active(active: u32) {
    let event = Event::Active {
        window: WINDOW,
        active: active != 0,
    };
    KEYS.with(|keys| keys.desk.screen().back().hand(event));
}

/// One pump of the keys' desk.
#[unsafe(no_mangle)]
pub extern "C" fn key_pump() {
    KEYS.with(|keys| keys.desk.pump());
}

/// The front's `GetAsyncKeyState` for `key`, which pumps first.
#[unsafe(no_mangle)]
pub extern "C" fn key_answer(key: u32) -> u32 {
    KEYS.with(|keys| keys.front.get_async_key_state(key))
}

/// 1 while `key`'s toggle is set, as the last pump left it.
#[unsafe(no_mangle)]
pub extern "C" fn key_toggled(key: u32) -> u32 {
    let at = usize::from(key_byte(key));
    KEYS.with(|keys| u32::from(keys.desk.screen().keys().toggled[at]))
}

/// The chord [`remap::read`] gives for the one pressed, under the row `plant` names: the key,
/// Ctrl at bit 8, Alt at bit 9. It reads the row alone and hands nothing to the keys.
#[unsafe(no_mangle)]
pub extern "C" fn remap_read(plant: u32, key: u32, ctrl: u32, alt: u32) -> u32 {
    let pressed = Chord {
        key: key_byte(key),
        ctrl: ctrl != 0,
        alt: alt != 0,
    };
    let read = remap::read(&ROWS[plant as usize..=plant as usize], pressed);
    u32::from(read.key) | u32::from(read.ctrl) << CTRL_BIT | u32::from(read.alt) << ALT_BIT
}
