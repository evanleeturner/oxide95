// SPDX-License-Identifier: MIT
//! The thin wasm a page's script runs: the worked script over `draw`'s page back, each
//! export one step, so the frames can go by batches and the script reads the picture from
//! outside. It holds no check. Lines come back by [`lines_ptr`] and [`lines_len`], UTF-8,
//! good until the next call.
#![expect(
    unsafe_code,
    reason = "`#[unsafe(no_mangle)]` is how a wasm export keeps its name"
)]

use core::cell::RefCell;

use draw::easel::Easel;
use draw::page::Page;
use window::desk::Desk;
use window::page::Page as Host;

#[path = "../bench/worked_script.rs"]
mod worked_script;

use worked_script::{CELLS, Check, HEIGHT, Plant, Script, Terms, WIDTH};

const WINDOW: u32 = 1;
const RGBA_BYTES: usize = CELLS * 4;
/// The links only the page's script can read: an export's number is an index here.
const OUTSIDE_LINKS: [&str; 4] = ["page-read", "canvas", "screenshot", "memory-bytes"];

type Rig = Script<'static, &'static Page, Desk<Host>>;

struct Run {
    page: &'static Page,
    script: Rig,
    rgba: Vec<u8>,
    reference: Vec<u8>,
}

thread_local! {
    static RUN: RefCell<Option<Run>> = const { RefCell::new(None) };
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
