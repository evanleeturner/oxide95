// SPDX-License-Identifier: MIT
//! The portable part every target shares: the buffers, the palettes, the
//! contexts and the objects they hold, and the screen. A blit, a fill of bits
//! and the palette's lookup are here; a back sees [`Shown`] alone, once a
//! present.
//!
//! A palette entry is kept as `0RGB`, Windows' `RGBQUAD` as memory holds it,
//! so the two conversions a back needs are one read away.
use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;
use core::cell::RefCell;
use core::ops::Range;

pub use window::Fit;

/// `DEFAULT_PALETTE`, a context's palette before any is selected.
const STOCK_PALETTE: u32 = 15;
/// `STOCK_LAST`: the highest index `GetStockObject` knows.
const STOCK_LAST: u32 = 19;
/// One past `STOCK_LAST`: a memory context's first bitmap, which no caller
/// can ask for by index.
const STOCK_BITMAP: u32 = STOCK_LAST + 1;
/// `BLACK_BRUSH`, a context's other slot before any is selected.
const STOCK_BRUSH: u32 = 4;
/// Where stock handles begin; handles made begin past them all.
const STOCK_BASE: u32 = 0x80;
/// The first handle made.
const FIRST: u32 = 0x100;
/// The most bytes a picture holds: a wild header is refused, never allocated.
const MOST_BYTES: u64 = 1 << 30;

/// The handle of the stock object at `index`.
const fn stock(index: u32) -> u32 {
    STOCK_BASE + index
}

/// One palette: 256 entries, each `0RGB`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Palette(pub [u32; 256]);

impl Palette {
    /// All black.
    #[must_use]
    pub const fn black() -> Self {
        Self([0; 256])
    }

    /// `entries` of Windows' `PALETTEENTRY`, each 32 bits as memory holds it
    /// (red in the low byte, its flags high), set from `start`; the count set.
    pub fn set_entries(&mut self, start: u32, entries: &[u32]) -> u32 {
        self.set(start, entries, |word| {
            ((word & 0xff) << 16) | (word & 0xff00) | ((word >> 16) & 0xff)
        })
    }

    /// `quads` of Windows' `RGBQUAD`, each 32 bits as memory holds it (blue
    /// in the low byte), set from `start`; the count set.
    pub fn set_quads(&mut self, start: u32, quads: &[u32]) -> u32 {
        self.set(start, quads, |word| word & 0x00ff_ffff)
    }

    fn set(&mut self, start: u32, words: &[u32], convert: impl Fn(u32) -> u32) -> u32 {
        let mut count = 0;
        let slots = self.0.iter_mut().skip(start as usize);
        for (slot, word) in slots.zip(words) {
            *slot = convert(*word);
            count += 1;
        }
        count
    }

    /// The entry at `index` as `u32` `0RGB`: softbuffer's pixel.
    #[must_use]
    pub const fn zero_rgb(&self, index: u8) -> u32 {
        self.0[index as usize]
    }

    /// The entry at `index` as RGBA bytes, opaque: `ImageData`'s pixel.
    #[must_use]
    pub const fn rgba(&self, index: u8) -> [u8; 4] {
        let [blue, green, red, _] = self.0[index as usize].to_le_bytes();
        [red, green, blue, 0xff]
    }

    /// The entry at `index` as Windows' `PALETTEENTRY`, flags 0.
    #[must_use]
    pub const fn entry(&self, index: u8) -> u32 {
        let [blue, green, red, _] = self.0[index as usize].to_le_bytes();
        u32::from_le_bytes([red, green, blue, 0])
    }
}

/// A picture's pixels: 8 bits each, `pitch` bytes a row, the rows in memory
/// top first unless `bottom_up`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Buffer {
    pub width: u32,
    pub height: u32,
    pub pitch: u32,
    pub bottom_up: bool,
    pub pixels: Vec<u8>,
    /// Its own colours, a DIB section's; `None` reads through the palette
    /// last realized.
    pub table: Option<Palette>,
}

impl Buffer {
    /// All index 0; each row rounded up to 4 bytes, as a DIB's; `None` past
    /// `MOST_BYTES`.
    #[must_use]
    pub fn sized(width: u32, height: u32, bottom_up: bool) -> Option<Self> {
        let pitch = (u64::from(width) + 3) & !3;
        let bytes = pitch * u64::from(height);
        if bytes > MOST_BYTES {
            return None;
        }
        Some(Self {
            width,
            height,
            pitch: u32::try_from(pitch).ok()?,
            bottom_up,
            pixels: vec![0; usize::try_from(bytes).ok()?],
            table: None,
        })
    }

    /// [`Self::sized`], or a picture of no pixels past `MOST_BYTES`.
    #[must_use]
    pub fn new(width: u32, height: u32, bottom_up: bool) -> Self {
        Self::sized(width, height, bottom_up).unwrap_or(Self {
            width: 0,
            height: 0,
            pitch: 0,
            bottom_up,
            pixels: Vec::new(),
            table: None,
        })
    }

    /// Where the picture's row `y` starts in `pixels`.
    fn row(&self, y: u32) -> usize {
        let row = if self.bottom_up {
            self.height - 1 - y
        } else {
            y
        };
        (row * self.pitch) as usize
    }

    /// The pixel at `x`, `y` of the picture, whichever way its rows lie.
    #[must_use]
    pub fn pixel(&self, x: u32, y: u32) -> u8 {
        self.pixels[self.row(y) + x as usize]
    }

    /// Sets the pixel at `x`, `y`.
    fn put(&mut self, x: u32, y: u32, index: u8) {
        let at = self.row(y) + x as usize;
        self.pixels[at] = index;
    }

    /// The low 32 bits of the pixels' address, the number a program is
    /// answered. On a 32-bit target, the page's and Windows', it is the
    /// pointer; on a 64-bit host, a number that is not 0 and nothing more:
    /// the ports build for 32 bits alone, and a host test writes through
    /// [`Easel::with_buffer`].
    #[must_use]
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the answer is the address's low 32 bits, at every width"
    )]
    pub fn address(&self) -> u32 {
        self.pixels.as_ptr().addr() as u32
    }
}

/// A number's bits as a signed number, two's complement: `cast_signed` is
/// newer than the crate's Rust.
pub(crate) const fn signed(value: u32) -> i32 {
    i32::from_ne_bytes(value.to_ne_bytes())
}

/// A signed number's bits as a number, the file's way of reading an answer.
pub(crate) const fn unsigned(value: i32) -> u32 {
    u32::from_ne_bytes(value.to_ne_bytes())
}

/// A rectangle in a picture's pixels, its corner and its size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Area {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Area {
    #[must_use]
    pub const fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x: signed(x),
            y: signed(y),
            width: signed(width),
            height: signed(height),
        }
    }
}

/// The buffer last shown, and the palette its indexes read through: all that
/// crosses to a back. Named apart from the call fronts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shown {
    pub width: u32,
    pub height: u32,
    pub pitch: u32,
    pub pixels: Vec<u8>,
    pub palette: Palette,
}

/// How a back fills a surface sized other than the picture.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Scale {
    /// The largest whole factor, centred, black around: [`Fit::whole`].
    #[default]
    Whole,
    /// The picture at its own size from the top left corner, the rest black:
    /// [`Shown::to_zero_rgb`].
    Corner,
}

/// What [`Shown::to_zero_rgb_sized`] wrote: its words' width and height,
/// and the fit they were drawn at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Painted {
    pub width: u32,
    pub height: u32,
    /// [`Fit::whole`] of the picture on the surface; `None` when no whole
    /// factor fits it or its size is not known.
    pub fit: Option<Fit>,
}

impl Shown {
    /// The picture as `0RGB` into rows `width` words wide, what fits from the
    /// top left corner, the rest of `into` 0: a window sized other than the
    /// picture, never scaled and never sheared.
    pub fn to_zero_rgb(&self, into: &mut [u32], width: u32) {
        into.fill(0);
        let rows = self.pixels.chunks_exact(self.pitch.max(1) as usize);
        for (slots, row) in into.chunks_exact_mut(width.max(1) as usize).zip(rows) {
            for (slot, index) in slots.iter_mut().zip(&row[..self.width as usize]) {
                *slot = self.palette.zero_rgb(*index);
            }
        }
    }

    /// The picture as `0RGB` into rows `width` words wide, filled by `scale`;
    /// the surface's height is the whole rows `into` holds. The fit drawn, or
    /// `None` when [`Scale::Corner`] was asked or no whole factor fits (a
    /// surface smaller than the picture on either side), each of which fills
    /// as [`Shown::to_zero_rgb`]. Every word outside the drawn box is 0; a
    /// surface the picture's own size gets the words `to_zero_rgb` writes.
    pub fn to_zero_rgb_scaled(&self, into: &mut [u32], width: u32, scale: Scale) -> Option<Fit> {
        let height = u32::try_from(into.len() / width.max(1) as usize).unwrap_or(u32::MAX);
        let fit = match scale {
            Scale::Whole => Fit::whole((self.width, self.height), (width, height)),
            Scale::Corner => None,
        };
        let Some(fit) = fit else {
            self.to_zero_rgb(into, width);
            return None;
        };
        let (line, factor) = (width as usize, fit.factor as usize);
        let (left, drawn) = (fit.x as usize, factor * self.width as usize);
        let (above, mut rest) = into.split_at_mut(line * fit.y as usize);
        above.fill(0);
        let rows = self.pixels.chunks_exact(self.pitch.max(1) as usize);
        for row in rows.take(self.height as usize) {
            let (block, after) = rest.split_at_mut(line * factor);
            let (first, copies) = block.split_at_mut(line);
            let (strip, picture) = first.split_at_mut(left);
            let (picture, right) = picture.split_at_mut(drawn);
            strip.fill(0);
            right.fill(0);
            for (slots, index) in picture
                .chunks_exact_mut(factor)
                .zip(&row[..self.width as usize])
            {
                slots.fill(self.palette.zero_rgb(*index));
            }
            for copy in copies.chunks_exact_mut(line) {
                copy.copy_from_slice(first);
            }
            rest = after;
        }
        rest.fill(0);
        Some(fit)
    }

    /// The picture as `0RGB` into `into`, sized here, for a back whose host
    /// takes words of their own size: on a `surface` sized other than the
    /// picture that a whole factor fits, the surface's words at
    /// [`Scale::Whole`]; else the picture's own words as
    /// [`Shown::to_zero_rgb`] writes them, which the host clips to a smaller
    /// surface. `surface` is `None` when its size is not known.
    pub fn to_zero_rgb_sized(&self, into: &mut Vec<u32>, surface: Option<(u32, u32)>) -> Painted {
        let picture = (self.width, self.height);
        let fit = surface.and_then(|surface| Fit::whole(picture, surface));
        match (surface, fit) {
            (Some((width, height)), Some(_)) if (width, height) != picture => {
                into.resize(width as usize * height as usize, 0);
                let fit = self.to_zero_rgb_scaled(into, width, Scale::Whole);
                Painted { width, height, fit }
            }
            _ => {
                into.resize(self.width as usize * self.height as usize, 0);
                self.to_zero_rgb(into, self.width);
                Painted {
                    width: self.width,
                    height: self.height,
                    fit,
                }
            }
        }
    }

    /// The picture as RGBA, 4 bytes a pixel, row by row with no gap, into `into`;
    /// a smaller `into` takes what fits, a larger keeps its rest.
    pub fn to_rgba(&self, into: &mut [u8]) {
        let rows = self.pixels.chunks_exact(self.pitch.max(1) as usize);
        let indexes = rows.flat_map(|row| &row[..self.width as usize]);
        for (slot, index) in into.chunks_exact_mut(4).zip(indexes) {
            slot.copy_from_slice(&self.palette.rgba(*index));
        }
    }
}

/// Where the program's pictures are shown: the seam between the calls and a
/// target. The page's, softbuffer's, and under Wine a real window's.
pub trait Back {
    /// `shown` is the picture now: the back shows it or keeps it to show.
    fn present(&self, shown: &Shown);
}

/// A borrowed back: each of a program's files keeps its own front and all
/// share the one back.
impl<B: Back> Back for &B {
    fn present(&self, shown: &Shown) {
        (**self).present(shown);
    }
}

/// One context: the window it draws on, or a memory context's buffer, and
/// what it has selected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Context {
    /// `Some` a window's (0 the whole screen), whose pixels are the screen.
    window: Option<u32>,
    bitmap: u32,
    palette: u32,
    other: u32,
}

/// What a handle names, past the contexts and the stock.
#[derive(Clone, Debug)]
enum Object {
    Palette(Palette),
    Bitmap(Buffer),
}

struct State {
    next: u32,
    contexts: BTreeMap<u32, Context>,
    objects: BTreeMap<u32, Object>,
    /// The pixels every window's context draws on.
    screen: Buffer,
    /// The colours last realized: what a table-less picture reads through.
    realized: Palette,
    /// The colours the screen is showing.
    colors: Palette,
}

impl State {
    fn handle(&mut self) -> u32 {
        let handle = self.next;
        self.next += 1;
        handle
    }

    /// The pixels `dc` draws on; `None` for a memory context holding no
    /// buffer, or no such context.
    fn pixels(&self, dc: u32) -> Option<&Buffer> {
        let context = self.contexts.get(&dc)?;
        if context.window.is_some() {
            return Some(&self.screen);
        }
        match self.objects.get(&context.bitmap) {
            Some(Object::Bitmap(buffer)) => Some(buffer),
            _ => None,
        }
    }

    fn pixels_mut(&mut self, dc: u32) -> Option<&mut Buffer> {
        let context = self.contexts.get(&dc)?;
        if context.window.is_some() {
            return Some(&mut self.screen);
        }
        match self.objects.get_mut(&context.bitmap) {
            Some(Object::Bitmap(buffer)) => Some(buffer),
            _ => None,
        }
    }

    fn is_window(&self, dc: u32) -> bool {
        self.contexts.get(&dc).is_some_and(|c| c.window.is_some())
    }

    fn selected(&self, object: u32) -> bool {
        self.contexts
            .values()
            .any(|c| c.bitmap == object || c.palette == object || c.other == object)
    }
}

/// The columns and rows of `to`, counted from its corner, that land on a
/// picture `width` by `height`: a wild rectangle costs what lands and no more,
/// and a negative size, a mirror, lands nothing.
fn landing(to: Area, width: u32, height: u32) -> (Range<i64>, Range<i64>) {
    let cut = |at: i32, size: i32, whole: u32| {
        let (at, size) = (i64::from(at), i64::from(size));
        (-at).max(0)..size.min(i64::from(whole) - at)
    };
    (cut(to.x, to.width, width), cut(to.y, to.height, height))
}

/// The source pixels `from` of `source`, scaled by nearest pixel to `to`'s
/// size, for what lands on a picture of `size`; a pixel outside reads `None`.
fn scaled(source: &Buffer, from: Area, to: Area, size: (u32, u32)) -> Vec<Option<u8>> {
    let inside = |x: i64, y: i64| {
        (0..i64::from(source.width)).contains(&x) && (0..i64::from(source.height)).contains(&y)
    };
    let (columns, rows) = landing(to, size.0, size.1);
    let mut taken = Vec::new();
    for row in rows {
        let y = i64::from(from.y) + row * i64::from(from.height) / i64::from(to.height);
        for column in columns.clone() {
            let x = i64::from(from.x) + column * i64::from(from.width) / i64::from(to.width);
            taken.push(inside(x, y).then(|| {
                let (x, y) = (u32::try_from(x).unwrap_or(0), u32::try_from(y).unwrap_or(0));
                source.pixel(x, y)
            }));
        }
    }
    taken
}

/// `taken`, what lands of `to` row by row, onto `target`.
fn paste(target: &mut Buffer, to: Area, taken: &[Option<u8>]) {
    let (columns, rows) = landing(to, target.width, target.height);
    let mut pixels = taken.iter();
    for row in rows {
        for column in columns.clone() {
            let index = pixels.next().copied().flatten();
            let (x, y) = (i64::from(to.x) + column, i64::from(to.y) + row);
            if let (Some(index), Ok(x), Ok(y)) = (index, u32::try_from(x), u32::try_from(y)) {
                target.put(x, y, index);
            }
        }
    }
}

/// One program's drawing: its objects, its contexts, its screen, and the back
/// that shows it.
pub struct Easel<B> {
    back: B,
    state: RefCell<State>,
}

impl<B: Back> Easel<B> {
    /// A screen `width` by `height`, all index 0, all black.
    pub fn new(back: B, width: u32, height: u32) -> Self {
        Self {
            back,
            state: RefCell::new(State {
                next: FIRST,
                contexts: BTreeMap::new(),
                objects: BTreeMap::new(),
                screen: Buffer::new(width, height, false),
                realized: Palette::black(),
                colors: Palette::black(),
            }),
        }
    }

    /// The back this easel shows on, for an owner that must aim it.
    #[must_use]
    pub const fn back(&self) -> &B {
        &self.back
    }

    /// Whether the state is lent out now, a front having come back in from
    /// inside [`Self::with_buffer`] or [`Self::with_screen`]. One thread's
    /// answer: this type is not `Sync`, so a second thread is its owner's to
    /// keep out.
    #[must_use]
    pub fn held(&self) -> bool {
        self.state.try_borrow_mut().is_err()
    }

    /// The buffer's fields and pixels `run` is handed, a bitmap's by its
    /// handle: what a program's write through the bits pointer stands for
    /// where that pointer is not one. `None` for no such bitmap.
    pub fn with_buffer<T>(&self, bitmap: u32, run: impl FnOnce(&mut Buffer) -> T) -> Option<T> {
        match self.state.borrow_mut().objects.get_mut(&bitmap) {
            Some(Object::Bitmap(buffer)) => Some(run(buffer)),
            _ => None,
        }
    }

    /// The screen's buffer lent to `run`, as [`Self::with_buffer`] lends a
    /// bitmap's. `run` must not call this easel. Presents nothing.
    pub fn with_screen<T>(&self, run: impl FnOnce(&mut Buffer) -> T) -> T {
        run(&mut self.state.borrow_mut().screen)
    }

    /// A new screen `width` by `height`, all index 0, as a mode's change
    /// leaves it; `false`, the screen as it was, past `MOST_BYTES`. Presents
    /// nothing, and contexts on the screen draw on the new one.
    pub fn resize_screen(&self, width: u32, height: u32) -> bool {
        let Some(screen) = Buffer::sized(width, height, false) else {
            return false;
        };
        self.state.borrow_mut().screen = screen;
        true
    }

    /// Swaps the screen's pixels with the bitmap's, no copy, so each pixels'
    /// address moves with them; `false`, nothing moved, for no such bitmap or
    /// one of another shape. Presents nothing; a table stays where it was.
    pub fn swap_screen(&self, bitmap: u32) -> bool {
        let state = &mut *self.state.borrow_mut();
        let Some(Object::Bitmap(buffer)) = state.objects.get_mut(&bitmap) else {
            return false;
        };
        let screen = &mut state.screen;
        let same = (buffer.width, buffer.height, buffer.pitch, buffer.bottom_up)
            == (screen.width, screen.height, screen.pitch, screen.bottom_up);
        if same {
            core::mem::swap(&mut buffer.pixels, &mut screen.pixels);
        }
        same
    }

    /// `palette` becomes the colours the screen shows and a table-less picture
    /// reads through, as a hardware palette's load; `false` for no such
    /// palette. Presents nothing: what the screen shows takes the new colours
    /// at the next present.
    pub fn show_palette(&self, palette: u32) -> bool {
        let state = &mut *self.state.borrow_mut();
        let Some(Object::Palette(kept)) = state.objects.get(&palette) else {
            return false;
        };
        state.realized = kept.clone();
        state.colors = kept.clone();
        true
    }

    /// What the screen shows now.
    #[must_use]
    pub fn shown(&self) -> Shown {
        let state = self.state.borrow();
        Shown {
            width: state.screen.width,
            height: state.screen.height,
            pitch: state.screen.pitch,
            pixels: state.screen.pixels.clone(),
            palette: state.colors.clone(),
        }
    }

    /// Hands what the screen shows to the back.
    pub fn present(&self) {
        self.back.present(&self.shown());
    }

    fn context(&self, window: Option<u32>) -> u32 {
        let state = &mut *self.state.borrow_mut();
        let handle = state.handle();
        state.contexts.insert(
            handle,
            Context {
                window,
                bitmap: stock(STOCK_BITMAP),
                palette: stock(STOCK_PALETTE),
                other: stock(STOCK_BRUSH),
            },
        );
        handle
    }

    /// A context on `window`, 0 the whole screen.
    pub fn window_context(&self, window: u32) -> u32 {
        self.context(Some(window))
    }

    /// A memory context, every context being 8 bits.
    pub fn memory_context(&self) -> u32 {
        self.context(None)
    }

    /// Lets `dc` go; whether it was one.
    pub fn delete_context(&self, dc: u32) -> bool {
        self.state.borrow_mut().contexts.remove(&dc).is_some()
    }

    /// A palette of `entries`; its handle.
    pub fn create_palette(&self, entries: &[u32]) -> u32 {
        let mut palette = Palette::black();
        palette.set_entries(0, entries);
        let state = &mut *self.state.borrow_mut();
        let handle = state.handle();
        state.objects.insert(handle, Object::Palette(palette));
        handle
    }

    /// A bitmap, its handle.
    pub fn create_bitmap(&self, buffer: Buffer) -> u32 {
        let state = &mut *self.state.borrow_mut();
        let handle = state.handle();
        state.objects.insert(handle, Object::Bitmap(buffer));
        handle
    }

    /// Selects `palette` into `dc`; the one it had, 0 for no such context.
    pub fn select_palette(&self, dc: u32, palette: u32) -> u32 {
        self.state
            .borrow_mut()
            .contexts
            .get_mut(&dc)
            .map_or(0, |context| {
                core::mem::replace(&mut context.palette, palette)
            })
    }

    /// `entries` set in `palette` from `start`; the count set, 0 for no such
    /// palette.
    pub fn set_palette_entries(&self, palette: u32, start: u32, entries: &[u32]) -> u32 {
        match self.state.borrow_mut().objects.get_mut(&palette) {
            Some(Object::Palette(kept)) => kept.set_entries(start, entries),
            _ => 0,
        }
    }

    /// `dc`'s palette becomes the colours a table-less picture reads through,
    /// the page and the desktop having no hardware palette to load.
    pub fn realize(&self, dc: u32) {
        let state = &mut *self.state.borrow_mut();
        let palette = state.contexts.get(&dc).map(|context| context.palette);
        if let Some(Object::Palette(kept)) = palette.and_then(|at| state.objects.get(&at)) {
            state.realized = kept.clone();
        }
    }

    /// `quads` set in the table of the buffer `dc` holds, from `start`; the
    /// count set, 0 for a context holding none or a window's, as Windows
    /// answers one with no section selected.
    pub fn set_table(&self, dc: u32, start: u32, quads: &[u32]) -> u32 {
        let mut state = self.state.borrow_mut();
        if state.is_window(dc) {
            return 0;
        }
        state.pixels_mut(dc).map_or(0, |buffer| {
            buffer
                .table
                .get_or_insert_with(Palette::black)
                .set_quads(start, quads)
        })
    }

    /// The realized colours from `start`, as Windows' `PALETTEENTRY`, into
    /// `into`; the count written.
    pub fn system_entries(&self, start: u32, into: &mut [u32]) -> u32 {
        let state = self.state.borrow();
        let mut count = 0;
        for (slot, at) in into.iter_mut().zip(start..256) {
            *slot = state.realized.entry(u8::try_from(at).unwrap_or(0));
            count += 1;
        }
        count
    }

    /// Selects `object` into `dc`, a bitmap, a palette or a stock object each
    /// in its own slot; the one it had, 0 for no such context, or for no such
    /// object.
    pub fn select(&self, dc: u32, object: u32) -> u32 {
        let state = &mut *self.state.borrow_mut();
        let kind = state
            .objects
            .get(&object)
            .map(|kept| matches!(kept, Object::Bitmap(_)));
        let is_stock = (STOCK_BASE..FIRST).contains(&object);
        let Some(context) = state.contexts.get_mut(&dc) else {
            return 0;
        };
        match (kind, is_stock) {
            (Some(true), _) => core::mem::replace(&mut context.bitmap, object),
            (Some(false), _) => core::mem::replace(&mut context.palette, object),
            (None, true) if object == stock(STOCK_BITMAP) => {
                core::mem::replace(&mut context.bitmap, object)
            }
            (None, true) if object == stock(STOCK_PALETTE) => {
                core::mem::replace(&mut context.palette, object)
            }
            (None, true) => core::mem::replace(&mut context.other, object),
            (None, false) => 0,
        }
    }

    /// Lets `object` go; true for a stock object, never let go, and for one
    /// let go; false for one still selected, as Windows refuses, or no such.
    pub fn delete_object(&self, object: u32) -> bool {
        if (STOCK_BASE..FIRST).contains(&object) {
            return true;
        }
        let mut state = self.state.borrow_mut();
        !state.selected(object) && state.objects.remove(&object).is_some()
    }

    /// The stock object at `index`, 0 past `STOCK_LAST`.
    #[must_use]
    pub const fn stock_object(index: u32) -> u32 {
        if index > STOCK_LAST { 0 } else { stock(index) }
    }

    /// `from` of `source`'s pixels scaled onto `to` of `target`'s, each clipped
    /// to its picture; `Some(())` when both contexts hold pixels. A window's
    /// target is presented, its colours the source's table or the realized.
    pub fn blit(&self, target: u32, to: Area, source: u32, from: Area) -> Option<()> {
        let (taken, table) = {
            let state = self.state.borrow();
            let buffer = state.pixels(source)?;
            let onto = state.pixels(target)?;
            let size = (onto.width, onto.height);
            (scaled(buffer, from, to, size), buffer.table.clone())
        };
        self.land(target, to, &taken, table)
    }

    /// `bits`, a picture of `width` by `height` with no table, its rows top
    /// first unless `bottom_up`, `from` of it onto `to` of `target`'s.
    pub fn draw_bits(
        &self,
        target: u32,
        to: Area,
        (width, height, bottom_up): (u32, u32, bool),
        bits: &[u8],
        from: Area,
    ) -> Option<()> {
        let mut source = Buffer::sized(width, height, bottom_up)?;
        let length = source.pixels.len().min(bits.len());
        source.pixels[..length].copy_from_slice(&bits[..length]);
        let size = {
            let state = self.state.borrow();
            let onto = state.pixels(target)?;
            (onto.width, onto.height)
        };
        self.land(target, to, &scaled(&source, from, to, size), None)
    }

    fn land(
        &self,
        target: u32,
        to: Area,
        taken: &[Option<u8>],
        table: Option<Palette>,
    ) -> Option<()> {
        let shows = {
            let state = &mut *self.state.borrow_mut();
            paste(state.pixels_mut(target)?, to, taken);
            let shows = state.is_window(target);
            if shows {
                state.colors = table.unwrap_or_else(|| state.realized.clone());
            }
            shows
        };
        if shows {
            self.present();
        }
        Some(())
    }
}
