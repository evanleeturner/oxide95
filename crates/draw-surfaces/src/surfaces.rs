// SPDX-License-Identifier: MIT
//! The portable part: one `DirectDraw` object, its surfaces and palettes, in a
//! file's 32-bit numbers over [`Easel`]. A surface is a number and a palette is
//! the easel's handle; no pointer crosses here.
//!
//! Each method writes one `imp.call` line and answers an `HRESULT`, a `Release`
//! its count. A chain's root is its primary, and the primary's pixels are the
//! easel's screen; its backs are bitmaps. A flip moves pixels, never copies them,
//! so the next `Lock` of a back answers another address, as the trace's did.
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::cell::RefCell;

use draw::easel::{Area, Back, Buffer, Easel};
use window::desk::Desk;
use window::{Numbers, Screen};

use crate::sdk::{
    DD_OK, DDBLT_COLORFILL, DDBLT_WAIT, DDBLTFAST_WAIT, DDERR_INVALIDMODE, DDERR_INVALIDOBJECT,
    DDERR_INVALIDPARAMS, DDERR_NOTFLIPPABLE, DDERR_NOTFOUND, DDERR_OUTOFMEMORY, DDERR_UNSUPPORTED,
    DDPF_PALETTEINDEXED8, DDPF_RGB, DDSCAPS_BACKBUFFER, DDSCAPS_COMPLEX, DDSCAPS_FLIP,
    DDSCAPS_FRONTBUFFER, DDSCAPS_PRIMARYSURFACE, DDSCAPS_SYSTEMMEMORY, DDSD_BACKBUFFERCOUNT,
    DDSD_CAPS, DDSD_HEIGHT, DDSD_PITCH, DDSD_PIXELFORMAT, DDSD_WIDTH, DESC_BACK_COUNT, DESC_BYTES,
    DESC_CAPS, DESC_FLAGS, DESC_FORMAT_BITS, DESC_FORMAT_FLAGS, DESC_FORMAT_SIZE, DESC_HEIGHT,
    DESC_PITCH, DESC_SIZE, DESC_SURFACE, DESC_WIDTH, Desc, FORMAT_BYTES,
};
use crate::table::Row;

/// A palette's entries a `draw.table` line, the layer's.
const TABLE_LINE: u32 = 16;
/// Every surface's depth.
const BITS: u32 = 8;
/// A flip chain's first back, as Wine answered it (`0x81c`).
const FIRST_BACK: u32 = DDSCAPS_BACKBUFFER | OTHER_BACK;
/// A flip chain's later backs.
const OTHER_BACK: u32 = DDSCAPS_COMPLEX | DDSCAPS_FLIP | DDSCAPS_SYSTEMMEMORY;

/// Where a surface's pixels are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pixels {
    Screen,
    Bitmap(u32),
}

struct Surface {
    pixels: Pixels,
    /// The context a blit reaches it through.
    dc: u32,
    caps: u32,
    refs: u32,
    /// A root's backs, the next to show first.
    chain: Vec<u32>,
    /// A chain's back: it goes with its root, never alone.
    member: bool,
    palette: u32,
}

struct State {
    next: u32,
    refs: u32,
    surfaces: BTreeMap<u32, Surface>,
    /// Each palette's handle and its count; an attached one is counted, as COM's.
    palettes: BTreeMap<u32, u32>,
}

/// A whole picture's area, or `rect` as Windows holds one: right and bottom
/// exclusive, so `(0,0)-(319,199)` leaves the last row and column.
fn area(rect: Option<[i32; 4]>, (width, height): (u32, u32)) -> Area {
    rect.map_or(
        Area::new(0, 0, width, height),
        |[left, top, right, bottom]| Area {
            x: left,
            y: top,
            width: right.saturating_sub(left),
            height: bottom.saturating_sub(top),
        },
    )
}

/// `index` over what lands of `to` on `buffer`, a top-down buffer's: a wild
/// rectangle costs what lands and no more.
fn fill(buffer: &mut Buffer, to: Area, index: u8) {
    let clip = |at: i32, size: i32, whole: u32| {
        let (at, size) = (i64::from(at), i64::from(size));
        let whole = i64::from(whole);
        let first = at.clamp(0, whole);
        let last = at.saturating_add(size).clamp(first, whole);
        let at = |edge: i64| usize::try_from(edge).unwrap_or(0);
        (at(first), at(last))
    };
    let (left, right) = clip(to.x, to.width, buffer.width);
    let (top, bottom) = clip(to.y, to.height, buffer.height);
    let pitch = buffer.pitch as usize;
    for row in top..bottom {
        buffer.pixels[row * pitch + left..row * pitch + right].fill(index);
    }
}

/// A number's bits as a signed number: `cast_signed` is newer than the crate's
/// Rust.
const fn signed(value: u32) -> i32 {
    i32::from_ne_bytes(value.to_ne_bytes())
}

/// `rect` as the numbers a line shows, a null one 4 zeros: a program's own
/// `(0,0)-(0,0)` rectangle lands nothing, so the two never need telling apart in
/// a run read back.
fn rect_words(rect: Option<[i32; 4]>) -> [u32; 4] {
    rect.unwrap_or([0; 4])
        .map(|edge| u32::from_ne_bytes(edge.to_ne_bytes()))
}

/// One program's `DirectDraw` object, over its easel and its desk.
pub struct Surfaces<'e, B, S> {
    row: &'static Row,
    easel: &'e Easel<B>,
    desk: &'e Desk<S>,
    state: RefCell<State>,
}

impl<'e, B: Back, S: Screen> Surfaces<'e, B, S> {
    #[must_use]
    pub const fn new(row: &'static Row, easel: &'e Easel<B>, desk: &'e Desk<S>) -> Self {
        Self {
            row,
            easel,
            desk,
            state: RefCell::new(State {
                next: 1,
                refs: 0,
                surfaces: BTreeMap::new(),
                palettes: BTreeMap::new(),
            }),
        }
    }

    /// Writes the call's one line: `a` when it takes numbers, `o` only when it
    /// wrote, as `draw`'s fronts show them.
    fn line(&self, call: &str, arguments: &[u32], wrote: &[u32], answer: u32) {
        let (a, o) = (Numbers(arguments), Numbers(wrote));
        let v = oxide_trace::__shown!(answer);
        self.desk.screen().quiet(|| {
            if !wrote.is_empty() {
                log::debug!(c = call, a:% = a, o:% = o, v:% = v, by = "draw-surfaces"; "imp.call");
            } else if arguments.is_empty() {
                log::debug!(c = call, v:% = v, by = "draw-surfaces"; "imp.call");
            } else {
                log::debug!(c = call, a:% = a, v:% = v, by = "draw-surfaces"; "imp.call");
            }
        });
    }

    /// The `draw.table` lines of `entries`, numbered from `start`.
    fn table(&self, call: &str, start: u32, entries: &[u32]) {
        self.desk.screen().quiet(|| {
            for (at, chunk) in (0_u32..).zip(entries.chunks(TABLE_LINE as usize)) {
                let first = start.wrapping_add(at * TABLE_LINE);
                let first = oxide_trace::__shown!(first);
                log::debug!(c = call, i:% = first, a:% = Numbers(chunk); "draw.table");
            }
        });
    }

    fn lend<T>(&self, pixels: Pixels, run: impl FnOnce(&mut Buffer) -> T) -> Option<T> {
        match pixels {
            Pixels::Screen => Some(self.easel.with_screen(run)),
            Pixels::Bitmap(handle) => self.easel.with_buffer(handle, run),
        }
    }

    /// A surface's pixels and its context; `None` for no such.
    fn found(&self, surface: u32) -> Option<(Pixels, u32)> {
        let state = self.state.borrow();
        let kept = state.surfaces.get(&surface)?;
        Some((kept.pixels, kept.dc))
    }

    fn size(&self, pixels: Pixels) -> Option<(u32, u32)> {
        self.lend(pixels, |buffer| (buffer.width, buffer.height))
    }

    fn keep(&self, surface: Surface) -> u32 {
        let state = &mut *self.state.borrow_mut();
        let id = state.next;
        state.next += 1;
        state.surfaces.insert(id, surface);
        id
    }

    /// A bitmap surface and its context; `None` past the easel's biggest picture.
    fn bitmap(&self, (width, height): (u32, u32), caps: u32, member: bool) -> Option<u32> {
        let buffer = Buffer::sized(width, height, false)?;
        let handle = self.easel.create_bitmap(buffer);
        let dc = self.easel.memory_context();
        self.easel.select(dc, handle);
        Some(self.keep(Surface {
            pixels: Pixels::Bitmap(handle),
            dc,
            caps,
            refs: 1,
            chain: Vec::new(),
            member,
            palette: 0,
        }))
    }

    /// The primary and its `count` backs, the screen's size each.
    fn primary(&self, caps: u32, count: u32) -> Result<u32, u32> {
        if count > self.row.chain {
            return Err(DDERR_OUTOFMEMORY);
        }
        let size = self
            .easel
            .with_screen(|screen| (screen.width, screen.height));
        let mut chain = Vec::new();
        for at in 0..count {
            let back = if at == 0 { FIRST_BACK } else { OTHER_BACK };
            let member = self.bitmap(size, back, true);
            chain.push(member.ok_or(DDERR_OUTOFMEMORY)?);
        }
        let front = if count > 0 { DDSCAPS_FRONTBUFFER } else { 0 };
        Ok(self.keep(Surface {
            pixels: Pixels::Screen,
            dc: self.easel.window_context(0),
            caps: caps | front,
            refs: 1,
            chain,
            member: false,
            palette: 0,
        }))
    }

    /// Lets one surface go and takes its palette's count down.
    fn free(&self, id: u32) {
        let gone = self.state.borrow_mut().surfaces.remove(&id);
        let Some(gone) = gone else {
            return;
        };
        self.easel.delete_context(gone.dc);
        if let Pixels::Bitmap(handle) = gone.pixels {
            self.easel.delete_object(handle);
        }
        self.drop_palette(gone.palette);
        for member in gone.chain {
            self.free(member);
        }
    }

    /// One count off `palette`; it goes at 0. The count left, 0 for no such.
    fn drop_palette(&self, palette: u32) -> u32 {
        let left = {
            let state = &mut *self.state.borrow_mut();
            let Some(refs) = state.palettes.get_mut(&palette) else {
                return 0;
            };
            *refs = refs.saturating_sub(1);
            let left = *refs;
            if left == 0 {
                state.palettes.remove(&palette);
            }
            left
        };
        if left == 0 {
            self.easel.delete_object(palette);
        }
        left
    }

    /// What a program's write through the bits pointer stands for where that
    /// pointer is not one: the surface's pixels lent to `run`. `None` for no such.
    pub fn with_surface<T>(&self, surface: u32, run: impl FnOnce(&mut Buffer) -> T) -> Option<T> {
        let (pixels, _) = self.found(surface)?;
        self.lend(pixels, run)
    }

    /// `DirectDrawCreate`: 0, the object's count 1. `guid` and `outer` are shown
    /// and not read: a caller naming a device gets this one.
    #[must_use]
    pub fn direct_draw_create(&self, guid: u32, outer: u32) -> u32 {
        self.state.borrow_mut().refs = 1;
        self.line("DDRAW!DirectDrawCreate", &[guid, outer], &[], DD_OK);
        DD_OK
    }

    /// The object's `Release`: the count left; at 0 every surface and palette
    /// goes with it. No run reached it: the reading of its callers, who read
    /// nothing back.
    #[must_use]
    pub fn release(&self) -> u32 {
        let left = {
            let mut state = self.state.borrow_mut();
            state.refs = state.refs.saturating_sub(1);
            state.refs
        };
        if left == 0 {
            let roots: Vec<u32> = {
                let state = self.state.borrow();
                let roots = state.surfaces.iter().filter(|(_, kept)| !kept.member);
                roots.map(|(id, _)| *id).collect()
            };
            for root in roots {
                self.free(root);
            }
            let palettes = core::mem::take(&mut self.state.borrow_mut().palettes);
            for palette in palettes.into_keys() {
                self.easel.delete_object(palette);
            }
        }
        self.line("IDirectDraw::Release", &[], &[], left);
        left
    }

    /// `SetCooperativeLevel`: 0, or `DDERR_INVALIDPARAMS` for flags no row holds.
    /// The level changes nothing here: this is every target's only screen.
    #[must_use]
    pub fn set_cooperative_level(&self, window: u32, flags: u32) -> u32 {
        let known = self.row.levels.contains(&flags);
        let answer = if known { DD_OK } else { DDERR_INVALIDPARAMS };
        let call = "IDirectDraw::SetCooperativeLevel";
        self.line(call, &[window, flags], &[], answer);
        answer
    }

    /// `SetDisplayMode`: 0 and a new screen, all index 0; `DDERR_INVALIDMODE`,
    /// the screen as it was, for a mode no row lists, every row's being 8 bits.
    #[must_use]
    pub fn set_display_mode(&self, width: u32, height: u32, bits: u32) -> u32 {
        let known = bits == BITS && self.row.modes.contains(&(width, height, bits));
        let made = known && self.easel.resize_screen(width, height);
        let answer = if made { DD_OK } else { DDERR_INVALIDMODE };
        let call = "IDirectDraw::SetDisplayMode";
        self.line(call, &[width, height, bits], &[], answer);
        answer
    }

    /// `RestoreDisplayMode`: 0, the screen as it is: no earlier mode is kept. No
    /// run reached it: the reading of its one caller, who reads nothing back.
    #[must_use]
    pub fn restore_display_mode(&self) -> u32 {
        self.line("IDirectDraw::RestoreDisplayMode", &[], &[], DD_OK);
        DD_OK
    }

    /// `GetCaps`: 0, each record's `dwCaps` 0 and its `dwSize` and the rest left:
    /// the bank-switched bit a caller refuses stays clear. `driver` and `hel` are
    /// each a record's first two words, `dwSize` then `dwCaps`.
    #[must_use]
    pub fn get_caps(&self, driver: Option<&mut [u32; 2]>, hel: Option<&mut [u32; 2]>) -> u32 {
        let size = driver.as_ref().map_or(0, |caps| caps[0]);
        for caps in [driver, hel].into_iter().flatten() {
            caps[1] = 0;
        }
        self.line("IDirectDraw::GetCaps", &[size], &[0], DD_OK);
        DD_OK
    }

    /// `CreateSurface`: 0 and `out` the new surface; `out` untouched and
    /// `DDERR_INVALIDPARAMS` for a wrong `dwSize` or an unsized plain surface,
    /// `DDERR_OUTOFMEMORY` for more backs than the row's chain (a caller retries
    /// with fewer) or a picture past the easel's biggest.
    #[must_use]
    pub fn create_surface(&self, desc: &Desc, out: &mut u32) -> u32 {
        let (flags, caps) = (desc[DESC_FLAGS], desc[DESC_CAPS]);
        let numbers = [
            flags,
            desc[DESC_HEIGHT],
            desc[DESC_WIDTH],
            desc[DESC_BACK_COUNT],
            caps,
        ];
        let call = "IDirectDraw::CreateSurface";
        let made = if desc[DESC_SIZE] != DESC_BYTES {
            Err(DDERR_INVALIDPARAMS)
        } else if caps & DDSCAPS_PRIMARYSURFACE != 0 {
            let asked = flags & DDSD_BACKBUFFERCOUNT != 0;
            self.primary(caps, if asked { desc[DESC_BACK_COUNT] } else { 0 })
        } else if flags & (DDSD_HEIGHT | DDSD_WIDTH) == DDSD_HEIGHT | DDSD_WIDTH {
            let size = (desc[DESC_WIDTH], desc[DESC_HEIGHT]);
            self.bitmap(size, caps, false).ok_or(DDERR_OUTOFMEMORY)
        } else {
            Err(DDERR_INVALIDPARAMS)
        };
        match made {
            Ok(id) => {
                *out = id;
                self.line(call, &numbers, &[id], DD_OK);
                DD_OK
            }
            Err(answer) => {
                self.line(call, &numbers, &[], answer);
                answer
            }
        }
    }

    /// `CreatePalette`: 0 and `out` the palette, its count 1;
    /// `DDERR_UNSUPPORTED`, `out` untouched, for flags no row holds. `entries`
    /// are each 32 bits as the caller's memory holds them.
    #[must_use]
    pub fn create_palette(&self, flags: u32, entries: &[u32], out: &mut u32) -> u32 {
        let call = "IDirectDraw::CreatePalette";
        if flags != self.row.palette_flags {
            self.line(call, &[flags], &[], DDERR_UNSUPPORTED);
            return DDERR_UNSUPPORTED;
        }
        let palette = self.easel.create_palette(entries);
        self.state.borrow_mut().palettes.insert(palette, 1);
        *out = palette;
        self.line(call, &[flags], &[palette], DD_OK);
        self.table(call, 0, entries);
        DD_OK
    }

    /// `SetEntries`: 0, the entries set from `start`; `DDERR_INVALIDOBJECT` for no
    /// such palette. On the palette attached to the primary it also shows the
    /// colours and presents, since a fade flips nothing: it moves by this alone.
    #[must_use]
    pub fn set_entries(&self, palette: u32, flags: u32, start: u32, entries: &[u32]) -> u32 {
        let call = "IDirectDrawPalette::SetEntries";
        let count = u32::try_from(entries.len()).unwrap_or(u32::MAX);
        let numbers = [palette, flags, start, count];
        let shows = {
            let state = self.state.borrow();
            if !state.palettes.contains_key(&palette) {
                drop(state);
                self.line(call, &numbers, &[], DDERR_INVALIDOBJECT);
                return DDERR_INVALIDOBJECT;
            }
            let mut kept = state.surfaces.values();
            kept.any(|surface| surface.pixels == Pixels::Screen && surface.palette == palette)
        };
        self.easel.set_palette_entries(palette, start, entries);
        if shows {
            self.easel.show_palette(palette);
            self.easel.present();
        }
        self.line(call, &numbers, &[], DD_OK);
        self.table(call, start, entries);
        DD_OK
    }

    /// A palette's `Release`: the count left, 0 for no such; it goes at 0. An
    /// attached one holds a count, so the game file's `Release` leaves it until
    /// its primary goes, as the trace's did.
    #[must_use]
    pub fn release_palette(&self, palette: u32) -> u32 {
        let left = self.drop_palette(palette);
        self.line("IDirectDrawPalette::Release", &[palette], &[], left);
        left
    }

    /// `GetAttachedSurface`: 0 and `out` the chain's next member, for
    /// `DDSCAPS_BACKBUFFER` and `DDSCAPS_FLIP` alike, its count up by 1, as Wine's
    /// answered both with the one interface; `DDERR_NOTFOUND`, `out` untouched,
    /// for other caps or no chain.
    #[must_use]
    pub fn get_attached_surface(&self, surface: u32, caps: u32, out: &mut u32) -> u32 {
        let call = "IDirectDrawSurface::GetAttachedSurface";
        let asked = caps == DDSCAPS_BACKBUFFER || caps == DDSCAPS_FLIP;
        let next = {
            let state = self.state.borrow();
            let chain = state.surfaces.get(&surface).map(|kept| kept.chain.first());
            chain.flatten().copied().filter(|_| asked)
        };
        let Some(next) = next else {
            self.line(call, &[surface, caps], &[], DDERR_NOTFOUND);
            return DDERR_NOTFOUND;
        };
        if let Some(kept) = self.state.borrow_mut().surfaces.get_mut(&next) {
            kept.refs += 1;
        }
        *out = next;
        self.line(call, &[surface, caps], &[next], DD_OK);
        DD_OK
    }

    /// `GetSurfaceDesc`: 0 and the size, pitch, 8-bit format and caps written, the
    /// other fields left; `DDERR_INVALIDPARAMS` for a wrong `dwSize`,
    /// `DDERR_INVALIDOBJECT` for no such surface. A caller reads the pitch alone.
    #[must_use]
    pub fn get_surface_desc(&self, surface: u32, desc: &mut Desc) -> u32 {
        let call = "IDirectDrawSurface::GetSurfaceDesc";
        if desc[DESC_SIZE] != DESC_BYTES {
            self.line(call, &[surface], &[], DDERR_INVALIDPARAMS);
            return DDERR_INVALIDPARAMS;
        }
        let kept = {
            let state = self.state.borrow();
            let kept = state.surfaces.get(&surface);
            kept.map(|kept| (kept.pixels, kept.caps, kept.chain.len()))
        };
        let shape = kept.and_then(|(pixels, caps, backs)| {
            let lent = self.lend(pixels, |b| (b.width, b.height, b.pitch))?;
            Some((lent, caps, u32::try_from(backs).unwrap_or(u32::MAX)))
        });
        let Some(((width, height, pitch), caps, backs)) = shape else {
            self.line(call, &[surface], &[], DDERR_INVALIDOBJECT);
            return DDERR_INVALIDOBJECT;
        };
        let counted = if backs > 0 { DDSD_BACKBUFFERCOUNT } else { 0 };
        desc[DESC_FLAGS] =
            DDSD_CAPS | DDSD_HEIGHT | DDSD_WIDTH | DDSD_PITCH | DDSD_PIXELFORMAT | counted;
        desc[DESC_HEIGHT] = height;
        desc[DESC_WIDTH] = width;
        desc[DESC_PITCH] = pitch;
        desc[DESC_BACK_COUNT] = backs;
        desc[DESC_FORMAT_SIZE] = FORMAT_BYTES;
        desc[DESC_FORMAT_FLAGS] = DDPF_PALETTEINDEXED8 | DDPF_RGB;
        desc[DESC_FORMAT_BITS] = BITS;
        desc[DESC_CAPS] = caps;
        self.line(call, &[surface], &[width, height, pitch, caps], DD_OK);
        DD_OK
    }

    /// `Lock`: 0, `lpSurface` and `lPitch` written and every other field left, a
    /// caller reading those two alone. `DDERR_UNSUPPORTED` for a rectangle, none
    /// measured; `DDERR_INVALIDOBJECT` for no such surface. `dwSize` is not read.
    #[must_use]
    pub fn lock(&self, surface: u32, rect: Option<[i32; 4]>, desc: &mut Desc, flags: u32) -> u32 {
        let call = "IDirectDrawSurface::Lock";
        let numbers = [surface, flags];
        if rect.is_some() {
            self.line(call, &numbers, &[], DDERR_UNSUPPORTED);
            return DDERR_UNSUPPORTED;
        }
        let lent = self
            .found(surface)
            .and_then(|(pixels, _)| self.lend(pixels, |buffer| (buffer.address(), buffer.pitch)));
        let Some((address, pitch)) = lent else {
            self.line(call, &numbers, &[], DDERR_INVALIDOBJECT);
            return DDERR_INVALIDOBJECT;
        };
        desc[DESC_SURFACE] = address;
        desc[DESC_PITCH] = pitch;
        self.line(call, &numbers, &[address, pitch], DD_OK);
        DD_OK
    }

    /// `Unlock`: 0, the writes already in the pixels. No present: a caller's next
    /// `Flip` shows them. `DDERR_INVALIDOBJECT` for no such surface.
    #[must_use]
    pub fn unlock(&self, surface: u32) -> u32 {
        let known = self.found(surface).is_some();
        let answer = if known { DD_OK } else { DDERR_INVALIDOBJECT };
        self.line("IDirectDrawSurface::Unlock", &[surface], &[], answer);
        answer
    }

    /// `IsLost`: 0: nothing here is ever lost, so `Restore` is never reached.
    #[must_use]
    pub fn is_lost(&self, surface: u32) -> u32 {
        self.line("IDirectDrawSurface::IsLost", &[surface], &[], DD_OK);
        DD_OK
    }

    /// `Restore`: 0, nothing having been lost. No run reached it: a caller's
    /// `IsLost` never answers lost.
    #[must_use]
    pub fn restore(&self, surface: u32) -> u32 {
        self.line("IDirectDrawSurface::Restore", &[surface], &[], DD_OK);
        DD_OK
    }

    /// `Flip`: 0 and a present. With no `over` the chain turns: the primary shows
    /// the first back's pixels, each back the next's, the last the primary's old.
    /// With `over`, the primary and it swap alone. `DDERR_NOTFLIPPABLE` with no
    /// chain, `DDERR_INVALIDPARAMS` for an `over` outside it. The last flip wins;
    /// a present no one read is the back's `draw.drop` line.
    #[must_use]
    pub fn flip(&self, surface: u32, over: u32, flags: u32) -> u32 {
        let answer = self.turn(surface, over);
        if answer == DD_OK {
            self.easel.present();
        }
        self.line(
            "IDirectDrawSurface::Flip",
            &[surface, over, flags],
            &[],
            answer,
        );
        answer
    }

    fn turn(&self, surface: u32, over: u32) -> u32 {
        let state = self.state.borrow();
        let Some(root) = state.surfaces.get(&surface) else {
            return DDERR_INVALIDOBJECT;
        };
        if root.chain.is_empty() {
            return DDERR_NOTFLIPPABLE;
        }
        if over != 0 && !root.chain.contains(&over) {
            return DDERR_INVALIDPARAMS;
        }
        // Walking back from the last, the one swap with the screen is all a turn
        // needs: `[P, A, B]` swaps with B then A, leaving `[A, B, P]`.
        let turned = if over == 0 {
            &root.chain[..]
        } else {
            core::slice::from_ref(&over)
        };
        for member in turned.iter().rev() {
            if let Some(Surface {
                pixels: Pixels::Bitmap(handle),
                ..
            }) = state.surfaces.get(member)
            {
                self.easel.swap_screen(*handle);
            }
        }
        DD_OK
    }

    /// `Blt`: a colour fill, `colour`'s low byte over `to` clipped, or a copy of
    /// `from` of `source` scaled onto `to`, each null rectangle the whole picture.
    /// A target that is the primary presents. `DDERR_UNSUPPORTED` for any other
    /// flag, a colour key or an effect; `DDERR_INVALIDOBJECT` for no such surface.
    #[must_use]
    pub fn blt(
        &self,
        target: u32,
        to: Option<[i32; 4]>,
        source: (u32, Option<[i32; 4]>),
        flags: u32,
        colour: u32,
    ) -> u32 {
        let answer = self.place(target, to, source, flags, colour);
        let [left, top, right, bottom] = rect_words(to);
        let [from_left, from_top, from_right, from_bottom] = rect_words(source.1);
        let numbers = [
            target,
            left,
            top,
            right,
            bottom,
            source.0,
            from_left,
            from_top,
            from_right,
            from_bottom,
            flags,
            colour,
        ];
        self.line("IDirectDrawSurface::Blt", &numbers, &[], answer);
        answer
    }

    fn place(
        &self,
        target: u32,
        to: Option<[i32; 4]>,
        source: (u32, Option<[i32; 4]>),
        flags: u32,
        colour: u32,
    ) -> u32 {
        let Some((pixels, dc)) = self.found(target) else {
            return DDERR_INVALIDOBJECT;
        };
        if flags & !DDBLT_WAIT == DDBLT_COLORFILL {
            let index = colour.to_le_bytes()[0];
            let filled = self.lend(pixels, |buffer| {
                let whole = (buffer.width, buffer.height);
                fill(buffer, area(to, whole), index);
            });
            if filled.is_none() {
                return DDERR_INVALIDOBJECT;
            }
            if pixels == Pixels::Screen {
                self.easel.present();
            }
            return DD_OK;
        }
        if flags & !DDBLT_WAIT != 0 {
            return DDERR_UNSUPPORTED;
        }
        self.copy(dc, pixels, to, source)
    }

    fn copy(
        &self,
        target: u32,
        pixels: Pixels,
        to: Option<[i32; 4]>,
        (source, from): (u32, Option<[i32; 4]>),
    ) -> u32 {
        let Some((from_pixels, from_dc)) = self.found(source) else {
            return DDERR_INVALIDOBJECT;
        };
        let sizes = self.size(pixels).zip(self.size(from_pixels));
        let Some((whole, from_whole)) = sizes else {
            return DDERR_INVALIDOBJECT;
        };
        let (to, from) = (area(to, whole), area(from, from_whole));
        match self.easel.blit(target, to, from_dc, from) {
            Some(()) => DD_OK,
            None => DDERR_INVALIDOBJECT,
        }
    }

    /// `BltFast`: `from` of `source` at `x`, `y`, unscaled; onto itself nothing
    /// and 0, the 1,626 the trace held. A target that is the primary presents.
    /// `DDERR_UNSUPPORTED` for any flag but `DDBLTFAST_WAIT`, a colour key;
    /// `DDERR_INVALIDOBJECT` for no such surface.
    #[must_use]
    pub fn blt_fast(
        &self,
        (target, x, y): (u32, u32, u32),
        source: u32,
        from: Option<[i32; 4]>,
        flags: u32,
    ) -> u32 {
        let answer = self.place_fast((target, x, y), source, from, flags);
        let [left, top, right, bottom] = rect_words(from);
        let numbers = [target, x, y, source, left, top, right, bottom, flags];
        self.line("IDirectDrawSurface::BltFast", &numbers, &[], answer);
        answer
    }

    fn place_fast(
        &self,
        (target, x, y): (u32, u32, u32),
        source: u32,
        from: Option<[i32; 4]>,
        flags: u32,
    ) -> u32 {
        if flags & !DDBLTFAST_WAIT != 0 {
            return DDERR_UNSUPPORTED;
        }
        let found = self.found(target).zip(self.found(source));
        let Some(((_, dc), (from_pixels, from_dc))) = found else {
            return DDERR_INVALIDOBJECT;
        };
        if target == source {
            return DD_OK;
        }
        let Some(from_whole) = self.size(from_pixels) else {
            return DDERR_INVALIDOBJECT;
        };
        let from = area(from, from_whole);
        let to = Area {
            x: signed(x),
            y: signed(y),
            ..from
        };
        match self.easel.blit(dc, to, from_dc, from) {
            Some(()) => DD_OK,
            None => DDERR_INVALIDOBJECT,
        }
    }

    /// `SetPalette`: 0, `palette` attached and counted, the one it had let go; on
    /// the primary its colours show at once and present. `palette` 0 detaches.
    /// `DDERR_INVALIDOBJECT` for no such surface or palette.
    #[must_use]
    pub fn set_palette(&self, surface: u32, palette: u32) -> u32 {
        let answer = self.attach(surface, palette);
        self.line(
            "IDirectDrawSurface::SetPalette",
            &[surface, palette],
            &[],
            answer,
        );
        answer
    }

    fn attach(&self, surface: u32, palette: u32) -> u32 {
        let (old, shows) = {
            let state = &mut *self.state.borrow_mut();
            if palette != 0 && !state.palettes.contains_key(&palette) {
                return DDERR_INVALIDOBJECT;
            }
            let Some(kept) = state.surfaces.get_mut(&surface) else {
                return DDERR_INVALIDOBJECT;
            };
            let old = core::mem::replace(&mut kept.palette, palette);
            let shows = palette != 0 && kept.pixels == Pixels::Screen;
            if let Some(refs) = state.palettes.get_mut(&palette) {
                *refs += 1;
            }
            (old, shows)
        };
        self.drop_palette(old);
        if shows {
            self.easel.show_palette(palette);
            self.easel.present();
        }
        DD_OK
    }

    /// A surface's `Release`: the count left, 0 for no such. A chain's back never
    /// goes alone; a root at 0 frees its whole chain, as a caller's one `Release`
    /// of the primary expects; a plain surface at 0 goes.
    #[must_use]
    pub fn release_surface(&self, surface: u32) -> u32 {
        let (left, goes) = {
            let mut state = self.state.borrow_mut();
            state.surfaces.get_mut(&surface).map_or((0, false), |kept| {
                kept.refs = kept.refs.saturating_sub(1);
                (kept.refs, kept.refs == 0 && !kept.member)
            })
        };
        if goes {
            self.free(surface);
        }
        self.line("IDirectDrawSurface::Release", &[surface], &[], left);
        left
    }

    /// The answer of a slot with no front: `DDERR_UNSUPPORTED` and one
    /// `surfaces.absent` line, the slot's offset as `ddraw.h` numbers it.
    #[must_use]
    pub fn absent(&self, interface: &str, slot: u32) -> u32 {
        let i = oxide_trace::__shown!(slot);
        self.desk.screen().quiet(|| {
            log::debug!(c = interface, i:% = i; "surfaces.absent");
        });
        DDERR_UNSUPPORTED
    }
}
