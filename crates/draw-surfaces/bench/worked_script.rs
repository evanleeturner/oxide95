// SPDX-License-Identifier: MIT
//! The worked script: a caller's calls on one picture, shared by file among the 3 programs,
//! each read printed as one JSON line for a separate script to score. It judges nothing.
//!
//! The picture is the computer's own check's, so a red here beside a green there is this
//! crate's or `draw`'s. What the pixels must hold is worked by plain loops, never through
//! `Surfaces`. A plant is chosen at run time, so none sits in the library.
//!
//! At 32 bits the bytes go through the pointer `Lock` lends, as a caller's do. At 64 bits
//! `lpSurface` is no pointer, `draw`'s named wall, so a host writes by `with_surface`.
#![allow(dead_code, reason = "each of the 3 programs takes its own reads")]

use std::alloc::{GlobalAlloc, Layout, System};
use std::fmt::Write as _;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

use draw::easel::{Back, Easel};
use draw::front::Windows;
#[cfg(target_pointer_width = "32")]
use draw_surfaces::sdk::DESC_PITCH;
use draw_surfaces::sdk::{
    DD_OK, DDBLT_COLORFILL, DDBLT_WAIT, DDBLTFAST_WAIT, DDERR_INVALIDMODE, DDSCAPS_BACKBUFFER,
    DDSCAPS_COMPLEX, DDSCAPS_FLIP, DDSCAPS_PRIMARYSURFACE, DDSCAPS_SYSTEMMEMORY,
    DDSD_BACKBUFFERCOUNT, DDSD_CAPS, DDSD_HEIGHT, DDSD_WIDTH, DESC_BACK_COUNT, DESC_BYTES,
    DESC_CAPS, DESC_FLAGS, DESC_HEIGHT, DESC_SIZE, DESC_SURFACE, DESC_WIDTH, DESC_WORDS, Desc,
    PALETTE_ENTRIES,
};
use draw_surfaces::surfaces::Surfaces;
use draw_surfaces::table::Row;

#[path = "../../layer-check/bench/worked_picture.rs"]
#[allow(
    dead_code,
    reason = "the other check's words; this script takes the picture"
)]
mod worked_picture;

pub use worked_picture::{HEIGHT, LEFT, TOP, WIDTH};

const COLUMNS: usize = WIDTH as usize;
const ROWS: usize = HEIGHT as usize;
pub const CELLS: usize = COLUMNS * ROWS;
const BANDS: usize = 4;
/// `SetCooperativeLevel`'s flags and `CreatePalette`'s, the row's.
const LEVEL: u32 = 0x51;
const PALETTE_FLAGS: u32 = 0x44;
/// `Lock`'s flags and `Flip`'s, as a caller waits.
const WAIT: u32 = 1;
const BACKS: u32 = 2;
/// A flip chain's members: `Lock`'s address on a back repeats at this period.
const PLACES: usize = 3;
/// `Blt`'s fill: left, top, right, bottom, over 2 bands, so it cannot land unseen.
const FILL: [u32; 4] = [16, 24, 116, 74];
const FILL_INDEX: u8 = 3;
/// The plain source's sides, and `BltFast`'s rectangle of it and its corner on the back.
const SOURCE_SIDES: (u32, u32) = (64, 32);
const COPY_FROM: [u32; 4] = [8, 4, 56, 28];
const COPY_AT: (u32, u32) = (200, 120);
/// Row 0's pixels that hold a frame's number, one bit each.
const FRAME_BITS: u32 = 32;
const LOST_READS: u32 = 3;
/// The frame live bytes are first read at, past whatever a first few settle.
const LIVE_AT: u32 = 100;
/// The frame `Plant::FlipStops` begins at.
const STOPS_AT: u32 = 1000;
const LEAK_BYTES: usize = 1024;
/// What RSS may grow by, N/10 to N.
const RSS_LIMIT: u64 = 1_048_576;
/// Frames between 2 turns of a window's messages.
const PUMP_EVERY: u32 = 64;
/// The hold for a dump from outside: about 3 s of presents.
const HOLD_TURNS: u32 = 150;
const HOLD_TURN: Duration = Duration::from_millis(20);

static ROW: Row = Row {
    file: "worked",
    levels: &[LEVEL, 8],
    modes: &[(WIDTH, HEIGHT, 8)],
    refused: DDERR_INVALIDMODE,
    chain: BACKS,
    palette_flags: PALETTE_FLAGS,
};

/// The bytes live now, all the backs alike: a leak has no other read on the page.
static LIVE: AtomicUsize = AtomicUsize::new(0);

struct Counting;

#[expect(
    unsafe_code,
    reason = "a global allocator's trait is unsafe by its shape"
)]
// SAFETY: every call is `System`'s own with the caller's arguments; the count touches no
// memory of theirs.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        LIVE.fetch_add(layout.size(), Ordering::Relaxed);
        // SAFETY: the caller's layout, passed on whole.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, at: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        // SAFETY: the caller's block and layout, passed on whole.
        unsafe { System.dealloc(at, layout) }
    }
}

#[global_allocator]
static COUNTING: Counting = Counting;

/// Which check a run is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Check {
    /// DS1: create, mode, chain, palette, `Lock`, bytes, `Unlock`, `Flip`.
    Picture,
    /// DS2: DS1's picture, then `Blt`'s fill and `BltFast`'s copy.
    Blits,
    /// DS3: the loop as flown, for many frames.
    Loop,
}

impl Check {
    pub const ALL: [Self; 3] = [Self::Picture, Self::Blits, Self::Loop];

    const fn word(self) -> &'static str {
        match self {
            Self::Picture => "DS1",
            Self::Blits => "DS2",
            Self::Loop => "DS3",
        }
    }

    fn named(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|check| check.word() == word)
    }

    /// The words a red of this check says, with the link it reads at.
    fn red(self, link: &str) -> String {
        match self {
            Self::Picture => format!(
                "The computer shows a plain picture right and this one wrong at {link}. \
                 `draw-surfaces` or `draw`, not the game."
            ),
            Self::Blits => {
                "`Blt` or `BltFast` lands in the wrong place. `draw-surfaces`, not the game."
                    .to_owned()
            }
            Self::Loop => {
                "The picture goes wrong only after many frames. `draw-surfaces`' chain, not the \
                 game."
                    .to_owned()
            }
        }
    }
}

/// A wrong answer planted in the caller, never in the library.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Plant {
    None,
    /// 2 bytes of one palette entry swapped, in the copy handed to `CreatePalette`.
    Palette,
    /// `BltFast` asked for one row low.
    Copy,
    /// From `STOPS_AT` on, a `Flip` that presents but never turns the chain.
    FlipStops,
    /// `LEAK_BYTES` kept every frame past `LIVE_AT`.
    Leak,
}

impl Plant {
    pub const ALL: [Self; 5] = [
        Self::None,
        Self::Palette,
        Self::Copy,
        Self::FlipStops,
        Self::Leak,
    ];

    const fn word(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Palette => "palette",
            Self::Copy => "copy",
            Self::FlipStops => "flip-stops",
            Self::Leak => "leak",
        }
    }

    fn named(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|plant| plant.word() == word)
    }
}

/// A run's terms, from a program's command line or a page's export.
#[derive(Clone, Copy)]
pub struct Terms {
    pub arm: &'static str,
    pub check: Check,
    pub plant: Plant,
    pub window: u32,
    pub frames: u32,
    /// This process's resident bytes, where the back can read them.
    pub rss: fn() -> Option<u64>,
}

impl Terms {
    /// Reads `<DS1|DS2|DS3> <plant> <frames>` from the command line.
    pub fn from_args(
        arm: &'static str,
        window: u32,
        rss: fn() -> Option<u64>,
    ) -> Result<Self, String> {
        let words: Vec<String> = std::env::args().skip(1).collect();
        let [check, plant, frames] = words.as_slice() else {
            return Err(format!(
                "3 words asked: check, plant, frames; got {words:?}"
            ));
        };
        Ok(Self {
            arm,
            check: Check::named(check).ok_or_else(|| format!("no check {check}"))?,
            plant: Plant::named(plant).ok_or_else(|| format!("no plant {plant}"))?,
            window,
            frames: frames.parse().map_err(|_| format!("no frames {frames}"))?,
            rss,
        })
    }
}

/// The picture as a palette's 4 colours and each pixel's index into them, 1 to 4.
fn indexed_picture() -> ([u32; BANDS], Vec<u8>) {
    let words = worked_picture::picture();
    let mut colours = [0; BANDS];
    for (band, colour) in colours.iter_mut().enumerate() {
        *colour = words[band * COLUMNS / BANDS];
    }
    let cells = words
        .iter()
        .map(|word| {
            let band = colours.iter().position(|colour| colour == word);
            band.map_or(0, |band| u8::try_from(band + 1).unwrap_or(0))
        })
        .collect();
    (colours, cells)
}

/// A palette entry as a caller's memory holds it, red in the low byte, from a `0RGB` word.
const fn entry(word: u32) -> u32 {
    word.swap_bytes() >> 8
}

/// The source's pixels: diagonal, so any wrong row or column reads different.
fn source_index(x: u32, y: u32) -> u8 {
    u8::try_from(1 + (x + y) % 4).unwrap_or(0)
}

fn rect(sides: [u32; 4]) -> [i32; 4] {
    sides.map(|side| i32::try_from(side).unwrap_or(0))
}

/// A frame's number into row 0, index 1 for a bit clear and 2 for a bit set.
fn stamp(cells: &mut [u8], number: u32) {
    for bit in 0..FRAME_BITS {
        cells[bit as usize] = 1 + u8::from((number >> bit) & 1 == 1);
    }
}

/// Rows of `width` into what `Lock` lent, through its pointer: a real one at this width.
#[cfg(target_pointer_width = "32")]
#[expect(unsafe_code, reason = "a caller writes through the pointer it is lent")]
fn put_rows<B: Back, W: Windows>(
    _: &Surfaces<'_, B, W>,
    _: u32,
    lent: &Desc,
    rows: &[u8],
    width: usize,
) {
    let (address, pitch) = (lent[DESC_SURFACE] as usize, lent[DESC_PITCH] as usize);
    for (y, row) in rows.chunks_exact(width).enumerate() {
        let at = std::ptr::with_exposed_provenance_mut::<u8>(address + y * pitch);
        // SAFETY: the pixels `Lock` lent, `pitch` bytes a row, at least `width`, and the
        // caller's rows are no more than the surface holds.
        unsafe { std::ptr::copy_nonoverlapping(row.as_ptr(), at, width) };
    }
}

/// Rows of `width` into a surface through the library: `lpSurface` is no pointer here.
#[cfg(not(target_pointer_width = "32"))]
fn put_rows<B: Back, W: Windows>(
    surfaces: &Surfaces<'_, B, W>,
    surface: u32,
    _: &Desc,
    rows: &[u8],
    width: usize,
) {
    let _ = surfaces.with_surface(surface, |buffer| {
        let pitch = buffer.pitch as usize;
        for (y, row) in rows.chunks_exact(width).enumerate() {
            buffer.pixels[y * pitch..y * pitch + width].copy_from_slice(row);
        }
    });
}

/// One run: the calls made, the answers counted, the reads kept as lines.
pub struct Script<'e, B, W> {
    terms: Terms,
    easel: &'e Easel<B>,
    surfaces: Surfaces<'e, B, W>,
    primary: u32,
    back: u32,
    source: u32,
    colours: [u32; BANDS],
    /// The picture's indexes, and a frame's, written whole each time.
    picture: Vec<u8>,
    frame_cells: Vec<u8>,
    /// What the pixels must hold at the last read.
    worked: Vec<u8>,
    calls: u64,
    not_zero: u64,
    frame: u32,
    places: [u32; PLACES],
    wrong_place: u64,
    lost: u64,
    live_at: usize,
    rss_at: Option<u64>,
    lines: String,
}

impl<'e, B: Back, W: Windows> Script<'e, B, W> {
    /// Every call up to the picture: create, level, mode, chain, palette, plain source.
    pub fn new(terms: Terms, easel: &'e Easel<B>, desk: &'e W) -> Self {
        let (colours, picture) = indexed_picture();
        let mut script = Self {
            terms,
            easel,
            surfaces: Surfaces::new(&ROW, easel, desk),
            primary: 0,
            back: 0,
            source: 0,
            colours,
            frame_cells: picture.clone(),
            worked: picture.clone(),
            picture,
            calls: 0,
            not_zero: 0,
            frame: 0,
            places: [0; PLACES],
            wrong_place: 0,
            lost: 0,
            live_at: 0,
            rss_at: None,
            lines: String::new(),
        };
        script.set_up();
        script
    }

    fn answered(&mut self, answer: u32) {
        self.calls += 1;
        self.not_zero += u64::from(answer != DD_OK);
    }

    fn set_up(&mut self) {
        let window = self.terms.window;
        self.answered(self.surfaces.direct_draw_create(0, 0));
        self.answered(self.surfaces.set_cooperative_level(window, LEVEL));
        self.answered(self.surfaces.set_display_mode(WIDTH, HEIGHT, 8));
        let mut desc: Desc = [0; DESC_WORDS];
        desc[DESC_SIZE] = DESC_BYTES;
        desc[DESC_FLAGS] = DDSD_CAPS | DDSD_BACKBUFFERCOUNT;
        desc[DESC_BACK_COUNT] = BACKS;
        desc[DESC_CAPS] = DDSCAPS_PRIMARYSURFACE | DDSCAPS_FLIP | DDSCAPS_COMPLEX;
        let (mut primary, mut back, mut palette, mut source) = (0, 0, 0, 0);
        self.answered(self.surfaces.create_surface(&desc, &mut primary));
        self.answered(
            self.surfaces
                .get_attached_surface(primary, DDSCAPS_BACKBUFFER, &mut back),
        );
        let mut entries = [0_u32; PALETTE_ENTRIES];
        for (slot, colour) in entries[1..].iter_mut().zip(self.colours) {
            *slot = entry(colour);
        }
        if self.terms.plant == Plant::Palette {
            entries[1] = self.colours[0];
        }
        self.answered(
            self.surfaces
                .create_palette(PALETTE_FLAGS, &entries, &mut palette),
        );
        self.answered(self.surfaces.set_palette(primary, palette));
        let mut plain: Desc = [0; DESC_WORDS];
        plain[DESC_SIZE] = DESC_BYTES;
        plain[DESC_FLAGS] = DDSD_CAPS | DDSD_HEIGHT | DDSD_WIDTH;
        plain[DESC_WIDTH] = SOURCE_SIDES.0;
        plain[DESC_HEIGHT] = SOURCE_SIDES.1;
        plain[DESC_CAPS] = DDSCAPS_SYSTEMMEMORY;
        self.answered(self.surfaces.create_surface(&plain, &mut source));
        self.primary = primary;
        self.back = back;
        self.source = source;
        let cells: Vec<u8> = (0..SOURCE_SIDES.1)
            .flat_map(|y| (0..SOURCE_SIDES.0).map(move |x| source_index(x, y)))
            .collect();
        self.write(source, &cells, SOURCE_SIDES.0 as usize);
    }

    /// `Lock`, the rows, `Unlock`: `Lock`'s address.
    fn write(&mut self, surface: u32, rows: &[u8], width: usize) -> u32 {
        let mut lent: Desc = [0; DESC_WORDS];
        lent[DESC_SIZE] = DESC_BYTES;
        self.answered(self.surfaces.lock(surface, None, &mut lent, WAIT));
        put_rows(&self.surfaces, surface, &lent, rows, width);
        self.answered(self.surfaces.unlock(surface));
        lent[DESC_SURFACE]
    }

    /// `Blt`'s fill and `BltFast`'s copy, on the back.
    fn blits(&mut self) {
        let colour = u32::from(FILL_INDEX);
        self.answered(self.surfaces.blt(
            self.back,
            Some(rect(FILL)),
            (0, None),
            DDBLT_COLORFILL | DDBLT_WAIT,
            colour,
        ));
        let low = u32::from(self.terms.plant == Plant::Copy);
        self.answered(self.surfaces.blt_fast(
            (self.back, COPY_AT.0, COPY_AT.1 + low),
            self.source,
            Some(rect(COPY_FROM)),
            DDBLTFAST_WAIT,
        ));
    }

    /// What the pixels must hold, by plain loops: the picture, a frame's number on the
    /// loop, the fill and the copy past DS1.
    fn work_out(&mut self, number: Option<u32>) {
        let mut cells = self.picture.clone();
        if let Some(number) = number {
            stamp(&mut cells, number);
        }
        if self.terms.check != Check::Picture {
            let [left, top, right, bottom] = FILL;
            for y in top..bottom {
                for x in left..right {
                    cells[y as usize * COLUMNS + x as usize] = FILL_INDEX;
                }
            }
            let [from_left, from_top, from_right, from_bottom] = COPY_FROM;
            for y in from_top..from_bottom {
                for x in from_left..from_right {
                    let to_y = COPY_AT.1 + y - from_top;
                    let to_x = COPY_AT.0 + x - from_left;
                    cells[to_y as usize * COLUMNS + to_x as usize] = source_index(x, y);
                }
            }
        }
        self.worked = cells;
    }

    fn differ_in_back(&self) -> u64 {
        let differ = self.surfaces.with_surface(self.back, |buffer| {
            let places = (0..HEIGHT).flat_map(|y| (0..WIDTH).map(move |x| (x, y)));
            places
                .zip(&self.worked)
                .filter(|((x, y), cell)| buffer.pixel(*x, *y) != **cell)
                .count()
        });
        differ.map_or(CELLS as u64, |differ| differ as u64)
    }

    fn differ_on_show(&self) -> u64 {
        let mut words = vec![0_u32; CELLS];
        self.easel.shown().to_zero_rgb(&mut words, WIDTH);
        words
            .iter()
            .zip(&self.worked)
            .filter(|(word, cell)| **word != self.colours[usize::from(**cell) - 1])
            .count() as u64
    }

    /// One link's read as one JSON line. It says `differ` and `of`; whether that is red is
    /// the scorer's call, and `red` the words if so.
    pub fn line(&mut self, link: &str, differ: u64, of: u64) {
        let red = self.terms.check.red(link);
        let _ = writeln!(
            self.lines,
            "{{\"arm\":\"{}\",\"check\":\"{}\",\"plant\":\"{}\",\"link\":\"{link}\",\
             \"differ\":{differ},\"of\":{of},\"red\":\"{red}\"}}",
            self.terms.arm,
            self.terms.check.word(),
            self.terms.plant.word(),
        );
    }

    /// The frames a run took, in milliseconds, for a rate.
    pub fn rate(&mut self, milliseconds: u64) {
        let _ = writeln!(
            self.lines,
            "{{\"arm\":\"{}\",\"check\":\"{}\",\"plant\":\"{}\",\"link\":\"rate\",\
             \"frames\":{},\"ms\":{milliseconds}}}",
            self.terms.arm,
            self.terms.check.word(),
            self.terms.plant.word(),
            self.frame,
        );
    }

    /// What the last picture must be, as runs of `0RGB` words, for a read from outside.
    fn reference_line(&mut self) {
        let mut runs: Vec<(u32, u32)> = Vec::new();
        for cell in &self.worked {
            let word = self.colours[usize::from(*cell) - 1];
            match runs.last_mut() {
                Some((last, count)) if *last == word => *count += 1,
                _ => runs.push((word, 1)),
            }
        }
        let runs: Vec<String> = runs
            .iter()
            .map(|(word, count)| format!("[{word},{count}]"))
            .collect();
        let _ = writeln!(
            self.lines,
            "{{\"arm\":\"{}\",\"check\":\"{}\",\"plant\":\"{}\",\"link\":\"reference\",\
             \"left\":{LEFT},\"top\":{TOP},\"width\":{WIDTH},\"height\":{HEIGHT},\"runs\":[{}]}}",
            self.terms.arm,
            self.terms.check.word(),
            self.terms.plant.word(),
            runs.join(","),
        );
    }

    /// The same picture as RGBA, for a page's own reads.
    pub fn reference_rgba(&self) -> Vec<u8> {
        self.worked
            .iter()
            .flat_map(|cell| {
                let word = self.colours[usize::from(*cell) - 1].to_be_bytes();
                [word[1], word[2], word[3], u8::MAX]
            })
            .collect()
    }

    fn one_frame(&mut self) {
        let number = self.frame;
        for _ in 0..LOST_READS {
            self.lost += u64::from(self.surfaces.is_lost(self.primary) != DD_OK);
            self.calls += 1;
        }
        let mut cells = std::mem::take(&mut self.frame_cells);
        stamp(&mut cells, number);
        let address = self.write(self.back, &cells, COLUMNS);
        self.frame_cells = cells;
        self.note_place(number, address);
        self.blits();
        if self.terms.plant == Plant::FlipStops && number >= STOPS_AT {
            self.easel.present();
        } else {
            self.answered(self.surfaces.flip(self.primary, 0, WAIT));
        }
        if number == LIVE_AT {
            self.live_at = LIVE.load(Ordering::Relaxed);
        }
        if number == self.terms.frames / 10 {
            self.rss_at = (self.terms.rss)();
        }
        if self.terms.plant == Plant::Leak && number > LIVE_AT {
            std::hint::black_box(Box::leak(vec![0_u8; LEAK_BYTES].into_boxed_slice()));
        }
        self.frame += 1;
    }

    /// `Lock`'s address is the chain's place: 3 different, then the same 3 in turn.
    fn note_place(&mut self, number: u32, address: u32) {
        let turn = number as usize % PLACES;
        if (number as usize) < PLACES {
            if self.places[..turn].contains(&address) {
                self.wrong_place += 1;
            }
            self.places[turn] = address;
        } else if self.places[turn] != address {
            self.wrong_place += 1;
        }
    }

    /// `count` frames of the loop.
    pub fn frames(&mut self, count: u32) {
        for _ in 0..count {
            self.one_frame();
        }
    }

    /// The reads of a run: DS1's and DS2's whole script, or the loop's end.
    pub fn finish(&mut self) {
        if self.terms.check == Check::Loop {
            self.finish_loop();
        } else {
            self.once();
        }
    }

    fn once(&mut self) {
        let rows = std::mem::take(&mut self.frame_cells);
        self.write(self.back, &rows, COLUMNS);
        self.frame_cells = rows;
        if self.terms.check == Check::Blits {
            self.blits();
        }
        self.work_out(None);
        let in_back = self.differ_in_back();
        self.answered(self.surfaces.flip(self.primary, 0, WAIT));
        let on_show = self.differ_on_show();
        let (calls, not_zero) = (self.calls, self.not_zero);
        self.line("calls", not_zero, calls);
        self.line("buffer", in_back, CELLS as u64);
        self.line("shown", on_show, CELLS as u64);
        self.reference_line();
    }

    fn finish_loop(&mut self) {
        // Read before a line is written, as a line allocates.
        let live = LIVE.load(Ordering::Relaxed);
        let rss = (self.terms.rss)();
        let frames = u64::from(self.frame);
        self.work_out(Some(self.frame.saturating_sub(1)));
        let on_show = self.differ_on_show();
        let (calls, not_zero, lost, wrong) =
            (self.calls, self.not_zero, self.lost, self.wrong_place);
        self.line("calls", not_zero, calls);
        self.line("is-lost", lost, frames * u64::from(LOST_READS));
        self.line("chain-place", wrong, frames);
        self.line(
            "live-bytes",
            live.abs_diff(self.live_at) as u64,
            self.live_at as u64,
        );
        match (self.rss_at, rss) {
            (Some(at), Some(rss)) => self.line("rss", rss.saturating_sub(at), RSS_LIMIT),
            _ => {
                let _ = writeln!(
                    self.lines,
                    "{{\"arm\":\"{}\",\"check\":\"{}\",\"plant\":\"{}\",\"link\":\"rss\",\
                     \"none\":true}}",
                    self.terms.arm,
                    self.terms.check.word(),
                    self.terms.plant.word(),
                );
            }
        }
        self.line("shown", on_show, CELLS as u64);
        self.reference_line();
    }

    /// A program's whole run: the loop by batches, `pump` between them, then the reads.
    pub fn drive(&mut self, mut pump: impl FnMut()) {
        if self.terms.check == Check::Loop {
            let mut left = self.terms.frames;
            while left > 0 {
                let batch = left.min(PUMP_EVERY);
                self.frames(batch);
                pump();
                left -= batch;
            }
        }
        self.finish();
    }

    /// The last picture held up for a dump from outside, bounded.
    pub fn hold(&self, mut pump: impl FnMut()) {
        for _ in 0..HOLD_TURNS {
            self.easel.present();
            pump();
            thread::sleep(HOLD_TURN);
        }
    }

    pub fn lines(&self) -> &str {
        &self.lines
    }
}
