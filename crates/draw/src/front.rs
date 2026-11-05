// SPDX-License-Identifier: MIT
//! The 23 fronts: one file's drawing calls over its program's [`Easel`] and
//! [`Desk`], each answering in the file's numbers with one `imp.call` line,
//! a palette's or a table's entries on `draw.table` lines behind it. Each doc
//! names its files: the shell, the game file, the launcher.
//!
//! A line equals the layer's less `from` and `tid`, handles apart.

use window::desk::Desk;
use window::{Numbers, Screen};

use crate::easel::{Area, Back, Buffer, Easel, Palette, signed, unsigned};
use crate::table::Row;

/// `SRCCOPY`: the one raster operation served, 263 of 263 logged.
const SRCCOPY: u32 = 0x00cc_0020;
/// `DIB_RGB_COLORS`: a section's table holds colours.
const DIB_RGB_COLORS: u32 = 0;
/// A table's entries a `draw.table` line, the layer's.
const TABLE_LINE: usize = 16;

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

/// One file's drawing calls: its row, its program's easel, and its desk.
pub struct Front<'e, B, S> {
    file: &'static Row,
    easel: &'e Easel<B>,
    desk: &'e Desk<S>,
}

impl<'e, B: Back, S: Screen> Front<'e, B, S> {
    pub const fn new(file: &'static Row, easel: &'e Easel<B>, desk: &'e Desk<S>) -> Self {
        Self { file, easel, desk }
    }

    /// Writes the call's one line: `o` only when the call wrote, as the
    /// layer's lines show it.
    fn line(&self, call: &str, arguments: &[u32], wrote: &[u32], answer: u32) {
        let (a, o) = (Numbers(arguments), Numbers(wrote));
        let v = oxide_trace::__shown!(answer);
        self.desk.screen().quiet(|| {
            if wrote.is_empty() {
                log::debug!(c = call, a:% = a, v:% = v; "imp.call");
            } else {
                log::debug!(c = call, a:% = a, o:% = o, v:% = v; "imp.call");
            }
        });
    }

    /// The `draw.table` lines of `entries`, numbered from `start`.
    fn table(&self, call: &str, start: u32, entries: &[u32]) {
        self.desk.screen().quiet(|| {
            for (at, chunk) in (0_u32..).zip(entries.chunks(TABLE_LINE)) {
                let first = start.wrapping_add(at * TABLE_LINE as u32);
                let first = oxide_trace::__shown!(first);
                log::debug!(c = call, i:% = first, a:% = Numbers(chunk); "draw.table");
            }
        });
    }

    /// `CreatePalette`: a handle, kept by the caller for a later select and
    /// delete. `a` the `LOGPALETTE`'s first word; `entries` its `PALETTEENTRY`s.
    /// Callers: the shell, the game file.
    pub fn create_palette(&self, first_word: u32, entries: &[u32]) -> u32 {
        let call = self.file.calls.create_palette;
        let handle = self.easel.create_palette(entries);
        self.line(call, &[first_word], &[], handle);
        self.table(call, 0, entries);
        handle
    }

    /// `SelectPalette`: the old palette, which the shell selects back through
    /// `SelectObject`. Callers: the shell, the game file.
    pub fn select_palette(&self, dc: u32, palette: u32, background: u32) -> u32 {
        let old = self.easel.select_palette(dc, palette);
        let call = self.file.calls.select_palette;
        self.line(call, &[dc, palette, background], &[], old);
        old
    }

    /// `SetPaletteEntries`: the count set, `0x100` measured; the shell reads
    /// nothing of it. Callers: the shell, the game file.
    pub fn set_palette_entries(
        &self,
        palette: u32,
        start: u32,
        count: u32,
        entries: &[u32],
    ) -> u32 {
        let call = self.file.calls.set_palette_entries;
        let entries = upto(entries, count);
        let set = self.easel.set_palette_entries(palette, start, entries);
        self.line(call, &[palette, start, count], &[], set);
        self.table(call, start, entries);
        set
    }

    /// `RealizePalette`: 0, measured 35 of 35: no hardware palette to load, so
    /// `dc`'s palette becomes the colours read through. Callers: the shell,
    /// the game file.
    pub fn realize_palette(&self, dc: u32) -> u32 {
        self.easel.realize(dc);
        self.line(self.file.calls.realize_palette, &[dc], &[], 0);
        0
    }

    /// `SetDIBColorTable`: the count set, `0x100` the rebuilt line; the shell
    /// keeps it and returns 1 regardless. Callers: the shell, the game file.
    pub fn set_dib_color_table(&self, dc: u32, start: u32, count: u32, quads: &[u32]) -> u32 {
        let call = self.file.calls.set_dib_color_table;
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
    pub fn stretch_blt(
        &self,
        target: u32,
        (x, y, width, height): (u32, u32, u32, u32),
        source: u32,
        (from_x, from_y, from_width, from_height): (u32, u32, u32, u32),
        rop: u32,
    ) -> u32 {
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
    pub fn create_compatible_dc(&self, dc: u32) -> u32 {
        let handle = self.easel.memory_context();
        self.line(self.file.calls.create_compatible_dc, &[dc], &[], handle);
        handle
    }

    /// `DeleteDC`: 1, 0 for no such; the shell reads nothing of it. Callers:
    /// the shell, the game file.
    pub fn delete_dc(&self, dc: u32) -> u32 {
        let answer = u32::from(self.easel.delete_context(dc));
        self.line(self.file.calls.delete_dc, &[dc], &[], answer);
        answer
    }

    /// `DeleteObject`: 1, 0 for no such or one still selected; the shell reads
    /// nothing of it. Callers: the shell, the game file.
    pub fn delete_object(&self, object: u32) -> u32 {
        let answer = u32::from(self.easel.delete_object(object));
        self.line(self.file.calls.delete_object, &[object], &[], answer);
        answer
    }

    /// `SelectObject`: the old object, kept and selected back; a palette goes
    /// to its own slot, since the shell selects one back this way. Callers:
    /// the shell, the game file.
    pub fn select_object(&self, dc: u32, object: u32) -> u32 {
        let old = self.easel.select(dc, object);
        self.line(self.file.calls.select_object, &[dc, object], &[], old);
        old
    }

    /// `GetDeviceCaps`: the row's answer at `index`, else 0. Callers: the
    /// shell, the game file, the launcher.
    pub fn get_device_caps(&self, dc: u32, index: u32) -> u32 {
        let found = self.file.caps.iter().find(|(at, _)| *at == index);
        let answer = found.map_or(0, |(_, answer)| unsigned(*answer));
        self.line(self.file.calls.get_device_caps, &[dc, index], &[], answer);
        answer
    }

    /// `GetStockObject`: the handle at `index`, 0 past the last. Callers: the
    /// launcher, index 4 for its class's background.
    pub fn get_stock_object(&self, index: u32) -> u32 {
        let answer = Easel::<B>::stock_object(index);
        self.line(self.file.calls.get_stock_object, &[index], &[], answer);
        answer
    }

    /// `GetDC`: a context on `window`, 0 the whole screen, kept or handed on.
    /// Callers: the shell, the game file, the launcher (window 0).
    pub fn get_dc(&self, window: u32) -> u32 {
        let handle = self.easel.window_context(window);
        self.line(self.file.calls.get_dc, &[window], &[], handle);
        handle
    }

    /// `ReleaseDC`: 1, 0 for no such; the shell reads nothing of it. Callers:
    /// the shell, the game file.
    pub fn release_dc(&self, window: u32, dc: u32) -> u32 {
        let answer = u32::from(self.easel.delete_context(dc));
        self.line(self.file.calls.release_dc, &[window, dc], &[], answer);
        answer
    }

    /// `UpdateWindow`: 1 and a present, 0 for no such window; the shell reads
    /// nothing of it. Callers: the shell, the game file, the launcher.
    pub fn update_window(&self, window: u32) -> u32 {
        let known = self.desk.state().window(window).is_some();
        if known {
            self.easel.present();
        }
        let answer = u32::from(known);
        self.line(self.file.calls.update_window, &[window], &[], answer);
        answer
    }

    /// `BeginPaint`: a context, `paint` its first 6 words (the context, no
    /// erase, `rcPaint` the whole window); 0, `paint` untouched, for no such
    /// window. The shell hands `rcPaint` on and never reads the context.
    /// Callers: the shell, on message `0xf`.
    pub fn begin_paint(&self, window: u32, paint: &mut [u32; 6]) -> u32 {
        let call = self.file.calls.begin_paint;
        let rect = self.desk.state().window(window).map(|kept| kept.rect);
        let Some(rect) = rect else {
            self.line(call, &[window], &[], 0);
            return 0;
        };
        let context = self.easel.window_context(window);
        let right = unsigned(rect.right.wrapping_sub(rect.left));
        let bottom = unsigned(rect.bottom.wrapping_sub(rect.top));
        *paint = [context, 0, 0, 0, right, bottom];
        self.line(call, &[window], paint, context);
        context
    }

    /// `EndPaint`: 1, 0 for no such context; `a` the window and the record's
    /// first 6 words. Callers: the shell.
    pub fn end_paint(&self, window: u32, paint: &[u32; 6]) -> u32 {
        let answer = u32::from(self.easel.delete_context(paint[0]));
        let p = paint;
        let numbers = [window, p[0], p[1], p[2], p[3], p[4], p[5]];
        self.line(self.file.calls.end_paint, &numbers, &[], answer);
        answer
    }

    /// `ValidateRect`: 1, 0 for no such window; `a` the window and the
    /// rectangle's 4, a null rectangle none. Nothing here repaints, so nothing
    /// is marked. Callers: the game file, window and null.
    pub fn validate_rect(&self, window: u32, rectangle: Option<[u32; 4]>) -> u32 {
        let answer = u32::from(self.desk.state().window(window).is_some());
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
