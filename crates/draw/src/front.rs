// SPDX-License-Identifier: MIT
//! The 23 fronts: one file's drawing calls over its program's [`Easel`] and
//! its [`Windows`], each answering in the file's numbers with one `imp.call`
//! line, a palette's or a table's entries on `draw.table` lines behind it. Each
//! doc names its files: the shell, the game file, the launcher.
//!
//! A line equals the layer's less `from` and `tid`, handles apart.
//!
//! A front reached while the easel's state is lent out, [`Easel::with_buffer`]'s
//! and [`Easel::with_screen`]'s closures being the only lenders read, changes
//! nothing: one `draw.frozen` line, and the answer the call gives when it draws.
//! A front that makes a handle has none to give, so it answers 0.

use window::desk::Desk;
use window::{Numbers, Screen};

use crate::easel::{Area, Back, Buffer, Easel, Palette, signed, unsigned};
use crate::table::Row;

/// `SRCCOPY`: the one raster operation served, 263 of 263 logged.
const SRCCOPY: u32 = 0x00cc_0020;
/// `DIB_RGB_COLORS`: a section's table holds colours.
const DIB_RGB_COLORS: u32 = 0;
/// A table's entries a `draw.table` line, the layer's.
const TABLE_LINE: u32 = 16;

/// A `BITMAPINFOHEADER`'s 10 words, as the layer logs them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header(pub [u32; 10]);

impl Header {
    const fn width(self) -> u32 {
        self.0[1]
    }

    /// The rows, whichever way they lie: a negative `biHeight` is top-down.
    const fn height(self) -> u32 {
        signed(self.0[2]).unsigned_abs()
    }

    const fn bottom_up(self) -> bool {
        signed(self.0[2]) > 0
    }

    /// `biBitCount`, word 3's high half.
    const fn depth(self) -> u32 {
        self.0[3] >> 16
    }

    /// The picture's shape, 8 bits being all this seam serves.
    const fn shape(self) -> Option<(u32, u32, bool)> {
        if self.depth() == 8 {
            Some((self.width(), self.height(), self.bottom_up()))
        } else {
            None
        }
    }
}

/// `entries` cut to `count`, the number the caller said.
fn upto(entries: &[u32], count: u32) -> &[u32] {
    &entries[..entries.len().min(count as usize)]
}

/// What a front asks of the windows it draws in: a desk's, or under Wine
/// the real ones, [`crate::user32::Real`].
pub trait Windows {
    /// Writes one line around whatever the target must keep across it.
    fn quiet(&self, line: impl FnOnce());
    /// `window`'s inside, wide then high; `None` for no such window.
    fn size(&self, window: u32) -> Option<(u32, u32)>;
    /// `UpdateWindow`'s part: `false` for no such window. A real window's
    /// procedure may run inside this call, so no borrow crosses it.
    fn update(&self, window: u32) -> bool;
    /// Marks `window`, or the rectangle in it, as painted; `false` for no such
    /// window. A real window left unmarked gets `WM_PAINT` on every peek:
    /// 39,657 of 39,657 under Wine 9.0, against 1 once marked.
    fn validate(&self, window: u32, rectangle: Option<[u32; 4]>) -> bool;
}

/// A desk keeps a shown window's paint waiting until it is marked, as a real
/// window does; a part of a window validated ends its whole paint.
impl<S: Screen> Windows for Desk<S> {
    fn quiet(&self, line: impl FnOnce()) {
        self.screen().quiet(line);
    }

    fn size(&self, window: u32) -> Option<(u32, u32)> {
        self.state().window(window).map(|kept| {
            let rect = kept.rect;
            (
                unsigned(rect.right.wrapping_sub(rect.left)),
                unsigned(rect.bottom.wrapping_sub(rect.top)),
            )
        })
    }

    fn update(&self, window: u32) -> bool {
        self.state().window(window).is_some()
    }

    fn validate(&self, window: u32, _rectangle: Option<[u32; 4]>) -> bool {
        self.mark_painted(window)
    }
}

/// One file's drawing calls: its row, its program's easel, and its windows.
pub struct Front<'e, B, W> {
    file: &'static Row,
    easel: &'e Easel<B>,
    windows: &'e W,
    from: Option<u32>,
}

impl<'e, B: Back, W: Windows> Front<'e, B, W> {
    pub const fn new(file: &'static Row, easel: &'e Easel<B>, windows: &'e W) -> Self {
        Self {
            file,
            easel,
            windows,
            from: None,
        }
    }

    /// The same front, its lines naming `from`, the address the call returns
    /// to, as the layer's own lines do. A desk has no caller to name.
    #[must_use]
    pub const fn from(mut self, from: u32) -> Self {
        self.from = Some(from);
        self
    }

    /// Whether the easel's state is lent out now, `answer` then being the
    /// call's whole answer; its one `draw.frozen` line written.
    fn frozen(&self, call: &str, answer: u32) -> bool {
        let held = self.easel.held();
        if held {
            let v = oxide_trace::__shown!(answer);
            self.windows.quiet(|| {
                log::warn!(c = call, v:% = v, by = "draw"; "draw.frozen");
            });
        }
        held
    }

    /// Writes the call's one line: `o` only when the call wrote, as the
    /// layer's lines show it.
    fn line(&self, call: &str, arguments: &[u32], wrote: &[u32], answer: u32) {
        let (a, o) = (Numbers(arguments), Numbers(wrote));
        let v = oxide_trace::__shown!(answer);
        self.windows.quiet(|| match (wrote.is_empty(), self.from) {
            (true, None) => log::debug!(c = call, a:% = a, v:% = v, by = "draw"; "imp.call"),
            (false, None) => {
                log::debug!(c = call, a:% = a, o:% = o, v:% = v, by = "draw"; "imp.call");
            }
            (true, Some(from)) => {
                let from = oxide_trace::__shown!(from);
                log::debug!(c = call, a:% = a, v:% = v, from:% = from, by = "draw"; "imp.call");
            }
            (false, Some(from)) => {
                let from = oxide_trace::__shown!(from);
                log::debug!(c = call, a:% = a, o:% = o, v:% = v, from:% = from, by = "draw"; "imp.call");
            }
        });
    }

    /// The `draw.table` lines of `entries`, numbered from `start`.
    fn table(&self, call: &str, start: u32, entries: &[u32]) {
        self.windows.quiet(|| {
            for (at, chunk) in (0_u32..).zip(entries.chunks(TABLE_LINE as usize)) {
                let first = start.wrapping_add(at * TABLE_LINE);
                let first = oxide_trace::__shown!(first);
                log::debug!(c = call, i:% = first, a:% = Numbers(chunk); "draw.table");
            }
        });
    }

    /// `CreatePalette`: a handle, kept by the caller for a later select and
    /// delete. `a` the `LOGPALETTE`'s first word; `entries` its `PALETTEENTRY`s.
    /// Callers: the shell, the game file.
    #[must_use]
    pub fn create_palette(&self, first_word: u32, entries: &[u32]) -> u32 {
        let call = self.file.calls.create_palette;
        if self.frozen(call, 0) {
            return 0;
        }
        let handle = self.easel.create_palette(entries);
        self.line(call, &[first_word], &[], handle);
        self.table(call, 0, entries);
        handle
    }

    /// `SelectPalette`: the old palette, which the shell selects back through
    /// `SelectObject`. Callers: the shell, the game file.
    #[must_use]
    pub fn select_palette(&self, dc: u32, palette: u32, background: u32) -> u32 {
        let call = self.file.calls.select_palette;
        if self.frozen(call, 0) {
            return 0;
        }
        let old = self.easel.select_palette(dc, palette);
        self.line(call, &[dc, palette, background], &[], old);
        old
    }

    /// `SetPaletteEntries`: the count set, `0x100` measured; the shell reads
    /// nothing of it. Callers: the shell, the game file.
    #[must_use]
    pub fn set_palette_entries(
        &self,
        palette: u32,
        start: u32,
        count: u32,
        entries: &[u32],
    ) -> u32 {
        let call = self.file.calls.set_palette_entries;
        if self.frozen(call, count) {
            return count;
        }
        let entries = upto(entries, count);
        let set = self.easel.set_palette_entries(palette, start, entries);
        self.line(call, &[palette, start, count], &[], set);
        self.table(call, start, entries);
        set
    }

    /// `RealizePalette`: 0, measured 35 of 35: no hardware palette to load, so
    /// `dc`'s palette becomes the colours read through. Callers: the shell,
    /// the game file.
    #[must_use]
    pub fn realize_palette(&self, dc: u32) -> u32 {
        let call = self.file.calls.realize_palette;
        if self.frozen(call, 0) {
            return 0;
        }
        self.easel.realize(dc);
        self.line(call, &[dc], &[], 0);
        0
    }

    /// `SetDIBColorTable`: the count set, `0x100` the rebuilt line; the shell
    /// keeps it and returns 1 regardless. Callers: the shell, the game file.
    #[must_use]
    pub fn set_dib_color_table(&self, dc: u32, start: u32, count: u32, quads: &[u32]) -> u32 {
        let call = self.file.calls.set_dib_color_table;
        if self.frozen(call, count) {
            return count;
        }
        let quads = upto(quads, count);
        let set = self.easel.set_table(dc, start, quads);
        self.line(call, &[dc, start, count], &[], set);
        self.table(call, start, quads);
        set
    }

    /// `GetSystemPaletteEntries`: the count written into `into`. The shell
    /// asks only when `BITSPIXEL` is 8, so a row answering 32 never reaches
    /// it. Callers: the shell.
    pub fn get_system_palette_entries(
        &self,
        dc: u32,
        start: u32,
        count: u32,
        into: &mut [u32],
    ) -> u32 {
        let call = self.file.calls.get_system_palette_entries;
        if self.frozen(call, 0) {
            return 0;
        }
        let room = into.len().min(count as usize);
        let written = self.easel.system_entries(start, &mut into[..room]);
        self.line(call, &[dc, start, count], &[], written);
        self.table(call, start, &into[..written as usize]);
        written
    }

    /// `CreateDIBSection`: a handle and `bits` the pixels' number, both read
    /// not 0 by every caller; 0, `bits` untouched, unless 8 bits. A usage 1
    /// `table` reads as the identity, the one measured, so no index map is
    /// kept. Callers: the shell, the game file.
    #[expect(
        clippy::too_many_arguments,
        reason = "CreateDIBSection's own, the header read"
    )]
    pub fn create_dib_section(
        &self,
        dc: u32,
        usage: u32,
        section: u32,
        offset: u32,
        header: Header,
        table: &[u32],
        bits: &mut u32,
    ) -> u32 {
        let call = self.file.calls.create_dib_section;
        if self.frozen(call, 0) {
            return 0;
        }
        let h = header.0;
        let numbers = [
            dc, usage, section, offset, h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7], h[8], h[9],
        ];
        let made = header
            .shape()
            .and_then(|(width, height, bottom_up)| Buffer::sized(width, height, bottom_up));
        let Some(mut buffer) = made else {
            self.line(call, &numbers, &[], 0);
            self.table(call, 0, table);
            return 0;
        };
        if usage == DIB_RGB_COLORS && !table.is_empty() {
            let mut colours = Palette::black();
            colours.set_quads(0, table);
            buffer.table = Some(colours);
        }
        let address = buffer.address();
        let handle = self.easel.create_bitmap(buffer);
        *bits = address;
        self.line(call, &numbers, &[address], handle);
        self.table(call, 0, table);
        handle
    }

    /// `BitBlt`: 1, measured 266 of 266; a raster operation not `SRCCOPY`
    /// draws nothing and answers 0. The shell compares the answer with the
    /// height and logs on 0. Callers: the shell.
    #[expect(clippy::too_many_arguments, reason = "BitBlt's own 9")]
    #[must_use]
    pub fn bit_blt(
        &self,
        target: u32,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        source: u32,
        from_x: u32,
        from_y: u32,
        rop: u32,
    ) -> u32 {
        if self.frozen(self.file.calls.bit_blt, 1) {
            return 1;
        }
        let to = Area::new(x, y, width, height);
        let from = Area::new(from_x, from_y, width, height);
        let drawn = rop == SRCCOPY && self.easel.blit(target, to, source, from).is_some();
        let answer = u32::from(drawn);
        let numbers = [target, x, y, width, height, source, from_x, from_y, rop];
        self.line(self.file.calls.bit_blt, &numbers, &[], answer);
        answer
    }

    /// `SetDIBitsToDevice`: `lines`, the height `0x1e0`, 32 of 32, which the
    /// game file compares with the height; 0 unless 8 bits. `bits` is taken
    /// from the picture's first row: `start` 0 is the one measured. Callers:
    /// the game file.
    #[expect(clippy::too_many_arguments, reason = "SetDIBitsToDevice's own 12")]
    #[must_use]
    pub fn set_dib_bits_to_device(
        &self,
        dc: u32,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        from_x: u32,
        from_y: u32,
        start: u32,
        lines: u32,
        usage: u32,
        header: Header,
        bits: &[u8],
    ) -> u32 {
        if self.frozen(self.file.calls.set_dib_bits_to_device, lines) {
            return lines;
        }
        let to = Area::new(x, y, width, height);
        let from = Area::new(from_x, from_y, width, height);
        let drawn = header
            .shape()
            .and_then(|shape| self.easel.draw_bits(dc, to, shape, bits, from));
        let answer = if drawn.is_some() { lines } else { 0 };
        let h = header.0;
        let numbers = [
            dc, x, y, width, height, from_x, from_y, start, lines, usage, h[0], h[1], h[2], h[3],
            h[4], h[5], h[6], h[7], h[8], h[9],
        ];
        self.line(
            self.file.calls.set_dib_bits_to_device,
            &numbers,
            &[],
            answer,
        );
        answer
    }

    /// `StretchDIBits`: the source's height, which its one caller compares; 0
    /// unless 8 bits and `SRCCOPY`. No run reached it: the answer is the
    /// reading of that caller's code. Callers: the game file.
    #[expect(clippy::too_many_arguments, reason = "StretchDIBits' own 13")]
    #[must_use]
    pub fn stretch_dib_bits(
        &self,
        dc: u32,
        (x, y, width, height): (u32, u32, u32, u32),
        (from_x, from_y, from_width, from_height): (u32, u32, u32, u32),
        usage: u32,
        rop: u32,
        header: Header,
        bits: &[u8],
    ) -> u32 {
        if self.frozen(self.file.calls.stretch_dib_bits, header.height()) {
            return header.height();
        }
        let to = Area::new(x, y, width, height);
        let from = Area::new(from_x, from_y, from_width, from_height);
        let drawn = header
            .shape()
            .filter(|_| rop == SRCCOPY)
            .and_then(|shape| self.easel.draw_bits(dc, to, shape, bits, from));
        let answer = if drawn.is_some() { header.height() } else { 0 };
        let h = header.0;
        let numbers = [
            dc,
            x,
            y,
            width,
            height,
            from_x,
            from_y,
            from_width,
            from_height,
            usage,
            rop,
            h[0],
            h[1],
            h[2],
            h[3],
            h[4],
            h[5],
            h[6],
            h[7],
            h[8],
            h[9],
        ];
        self.line(self.file.calls.stretch_dib_bits, &numbers, &[], answer);
        answer
    }

    /// `StretchBlt`: 1; 0 for a raster operation not `SRCCOPY`, which logs
    /// `DISPDIB_StretchBlit` in the game file. No run reached it. Callers:
    /// the game file; the shell imports it, 0 calls in the exports.
    #[must_use]
    pub fn stretch_blt(
        &self,
        target: u32,
        (x, y, width, height): (u32, u32, u32, u32),
        source: u32,
        (from_x, from_y, from_width, from_height): (u32, u32, u32, u32),
        rop: u32,
    ) -> u32 {
        if self.frozen(self.file.calls.stretch_blt, 1) {
            return 1;
        }
        let to = Area::new(x, y, width, height);
        let from = Area::new(from_x, from_y, from_width, from_height);
        let drawn = rop == SRCCOPY && self.easel.blit(target, to, source, from).is_some();
        let answer = u32::from(drawn);
        let numbers = [
            target,
            x,
            y,
            width,
            height,
            source,
            from_x,
            from_y,
            from_width,
            from_height,
            rop,
        ];
        self.line(self.file.calls.stretch_blt, &numbers, &[], answer);
        answer
    }

    /// `CreateCompatibleDC`: a memory context, the handle kept. Every context
    /// being 8 bits, `dc` chooses nothing. Callers: the shell, the game file.
    #[must_use]
    pub fn create_compatible_dc(&self, dc: u32) -> u32 {
        let call = self.file.calls.create_compatible_dc;
        if self.frozen(call, 0) {
            return 0;
        }
        let handle = self.easel.memory_context();
        self.line(call, &[dc], &[], handle);
        handle
    }

    /// `DeleteDC`: 1, 0 for no such; the shell reads nothing of it. Callers:
    /// the shell, the game file.
    #[must_use]
    pub fn delete_dc(&self, dc: u32) -> u32 {
        let call = self.file.calls.delete_dc;
        if self.frozen(call, 1) {
            return 1;
        }
        let answer = u32::from(self.easel.delete_context(dc));
        self.line(call, &[dc], &[], answer);
        answer
    }

    /// `DeleteObject`: 1, 0 for no such or one still selected; the shell reads
    /// nothing of it. Callers: the shell, the game file.
    #[must_use]
    pub fn delete_object(&self, object: u32) -> u32 {
        let call = self.file.calls.delete_object;
        if self.frozen(call, 1) {
            return 1;
        }
        let answer = u32::from(self.easel.delete_object(object));
        self.line(call, &[object], &[], answer);
        answer
    }

    /// `SelectObject`: the old object, kept and selected back; a palette goes
    /// to its own slot, since the shell selects one back this way. Callers:
    /// the shell, the game file.
    #[must_use]
    pub fn select_object(&self, dc: u32, object: u32) -> u32 {
        let call = self.file.calls.select_object;
        if self.frozen(call, 0) {
            return 0;
        }
        let old = self.easel.select(dc, object);
        self.line(call, &[dc, object], &[], old);
        old
    }

    /// `GetDeviceCaps`: the row's answer at `index`, else 0. Callers: the
    /// shell, the game file, the launcher.
    #[must_use]
    pub fn get_device_caps(&self, dc: u32, index: u32) -> u32 {
        let found = self.file.caps.iter().find(|(at, _)| *at == index);
        let answer = found.map_or(0, |(_, answer)| unsigned(*answer));
        self.line(self.file.calls.get_device_caps, &[dc, index], &[], answer);
        answer
    }

    /// `GetStockObject`: the handle at `index`, 0 past the last. Callers: the
    /// launcher, index 4 for its class's background.
    #[must_use]
    pub fn get_stock_object(&self, index: u32) -> u32 {
        let answer = Easel::<B>::stock_object(index);
        self.line(self.file.calls.get_stock_object, &[index], &[], answer);
        answer
    }

    /// `GetDC`: a context on `window`, 0 the whole screen, kept or handed on.
    /// Callers: the shell, the game file, the launcher (window 0).
    #[must_use]
    pub fn get_dc(&self, window: u32) -> u32 {
        let call = self.file.calls.get_dc;
        if self.frozen(call, 0) {
            return 0;
        }
        let handle = self.easel.window_context(window);
        self.line(call, &[window], &[], handle);
        handle
    }

    /// `ReleaseDC`: 1, 0 for no such; the shell reads nothing of it. Callers:
    /// the shell, the game file.
    #[must_use]
    pub fn release_dc(&self, window: u32, dc: u32) -> u32 {
        let call = self.file.calls.release_dc;
        if self.frozen(call, 1) {
            return 1;
        }
        let answer = u32::from(self.easel.delete_context(dc));
        self.line(call, &[window, dc], &[], answer);
        answer
    }

    /// `UpdateWindow`: 1 and a present, 0 for no such window; the shell reads
    /// nothing of it. Callers: the shell, the game file, the launcher.
    #[must_use]
    pub fn update_window(&self, window: u32) -> u32 {
        let call = self.file.calls.update_window;
        if self.frozen(call, 1) {
            return 1;
        }
        let known = self.windows.update(window);
        if known {
            self.easel.present();
        }
        let answer = u32::from(known);
        self.line(call, &[window], &[], answer);
        answer
    }

    /// `BeginPaint`: a context, `paint` its first 6 words (the context, no
    /// erase, `rcPaint` the whole window); 0, `paint` untouched, for no such
    /// window. The shell hands `rcPaint` on and never reads the context. The
    /// window is marked painted here: no real `BeginPaint` is called.
    /// Callers: the shell, on message `0xf`.
    pub fn begin_paint(&self, window: u32, paint: &mut [u32; 6]) -> u32 {
        let call = self.file.calls.begin_paint;
        let Some((right, bottom)) = self.windows.size(window) else {
            self.line(call, &[window], &[], 0);
            return 0;
        };
        self.windows.validate(window, None);
        if self.frozen(call, 0) {
            return 0;
        }
        let context = self.easel.window_context(window);
        *paint = [context, 0, 0, 0, right, bottom];
        self.line(call, &[window], paint, context);
        context
    }

    /// `EndPaint`: 1, 0 for no such context; `a` the window and the record's
    /// first 6 words. Callers: the shell.
    #[must_use]
    pub fn end_paint(&self, window: u32, paint: &[u32; 6]) -> u32 {
        if self.frozen(self.file.calls.end_paint, 1) {
            return 1;
        }
        let answer = u32::from(self.easel.delete_context(paint[0]));
        let p = paint;
        let numbers = [window, p[0], p[1], p[2], p[3], p[4], p[5]];
        self.line(self.file.calls.end_paint, &numbers, &[], answer);
        answer
    }

    /// `ValidateRect`: 1, 0 for no such window; `a` the window and the
    /// rectangle's 4, a null rectangle none. Callers: the game file, window and
    /// null.
    #[must_use]
    pub fn validate_rect(&self, window: u32, rectangle: Option<[u32; 4]>) -> u32 {
        let answer = u32::from(self.windows.validate(window, rectangle));
        let call = self.file.calls.validate_rect;
        match rectangle {
            Some([left, top, right, bottom]) => {
                self.line(call, &[window, left, top, right, bottom], &[], answer);
            }
            None => self.line(call, &[window], &[], answer),
        }
        answer
    }
}
