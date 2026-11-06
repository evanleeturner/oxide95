// SPDX-License-Identifier: MIT
//! The fronts on the page's back, on the host. The trace's replay is a game's
//! and lives in its rows crate; these hold what no run shows, pixels above all,
//! since the kept trace's flips are of a black screen: they are planted here, and
//! each test's doc says when an answer is the reading of a caller's code.
use core::mem::{offset_of, size_of};

use draw::easel::Easel;
use draw::page::Page;
use window::desk::Desk;
use window::page::Page as Host;

use crate::com::{Com, Interface, ObjectTable, PaletteTable, SurfaceTable, sdk_offset};
use crate::sdk::{
    DD_OK, DDBLT_COLORFILL, DDBLT_WAIT, DDBLTFAST_WAIT, DDERR_INVALIDMODE, DDERR_INVALIDOBJECT,
    DDERR_INVALIDPARAMS, DDERR_NOTFLIPPABLE, DDERR_NOTFOUND, DDERR_OUTOFMEMORY, DDERR_UNSUPPORTED,
    DDSCAPS_BACKBUFFER, DDSCAPS_FLIP, DESC_BACK_COUNT, DESC_BYTES, DESC_CAPS, DESC_FLAGS,
    DESC_FORMAT_BITS, DESC_HEIGHT, DESC_PITCH, DESC_SIZE, DESC_SURFACE, DESC_WIDTH, DESC_WORDS,
    Desc, FX_COLOUR, FX_WORDS,
};
use crate::surfaces::Surfaces;
use crate::table::Row;
use crate::test_log::debug_lines;

static ROW: Row = Row {
    file: "test",
    levels: &[0x51, 8],
    modes: &[(320, 200, 8), (64, 32, 8)],
    chain: 2,
    palette_flags: 0x44,
};

type Rig = Surfaces<'static, &'static Page, Host>;

struct Parts {
    page: &'static Page,
    easel: &'static Easel<&'static Page>,
    surfaces: Rig,
}

fn parts() -> Parts {
    let page: &'static Page = Box::leak(Box::new(Page::new()));
    let easel: &'static Easel<&'static Page> = Box::leak(Box::new(Easel::new(page, 64, 32)));
    let desk: &'static Desk<Host> = Box::leak(Box::new(Desk::new(Host::new(64, 32))));
    Parts {
        page,
        easel,
        surfaces: Surfaces::new(&ROW, easel, desk),
    }
}

/// The description a chain's caller writes, `count` backs.
fn chain_desc(count: u32) -> Desc {
    let mut desc = [0; DESC_WORDS];
    desc[DESC_SIZE] = DESC_BYTES;
    desc[DESC_FLAGS] = 0x20;
    desc[DESC_BACK_COUNT] = count;
    desc[DESC_CAPS] = 0x218;
    desc
}

/// The primary and its first back, the route a caller takes.
fn chain(surfaces: &Rig) -> (u32, u32) {
    let (mut primary, mut back) = (0, 0);
    assert_eq!(surfaces.create_surface(&chain_desc(2), &mut primary), DD_OK);
    assert_eq!(
        surfaces.get_attached_surface(primary, DDSCAPS_BACKBUFFER, &mut back),
        DD_OK
    );
    (primary, back)
}

fn plant(surfaces: &Rig, surface: u32, index: u8) {
    surfaces
        .with_surface(surface, |buffer| buffer.pixels.fill(index))
        .expect("invariant: the surface is live");
}

fn shown(parts: &Parts) -> u8 {
    parts.easel.shown().pixels[0]
}

/// 21 fronts, the crate's doc naming them and another game's 3.
#[test]
fn every_name_placed_in_the_crate_doc() {
    let doc = include_str!("lib.rs");
    for name in [
        "DirectDrawCreate",
        "SetCooperativeLevel",
        "SetDisplayMode",
        "GetCaps",
        "CreateSurface",
        "CreatePalette",
        "RestoreDisplayMode",
        "SetEntries",
        "GetAttachedSurface",
        "GetSurfaceDesc",
        "`Lock`",
        "`Unlock`",
        "`IsLost`",
        "`Restore`",
        "`Flip`",
        "`Blt`",
        "`BltFast`",
        "`SetPalette`",
        "`Release`",
        "FlipToGDISurface",
        "`GetDC`",
        "`ReleaseDC`",
    ] {
        assert!(doc.contains(name), "{name}");
    }
}

/// A slot's function, by its arguments, `this` counted.
trait Arguments {
    const COUNT: usize;
}

macro_rules! arguments {
    ($count:literal: $($a:ident),*) => {
        impl<R, $($a),*> Arguments for unsafe extern "system" fn($($a),*) -> R {
            const COUNT: usize = $count;
        }
    };
}
arguments!(1: A);
arguments!(2: A, C);
arguments!(3: A, C, D);
arguments!(4: A, C, D, E);
arguments!(5: A, C, D, E, F);
arguments!(6: A, C, D, E, F, G);

fn count<T, F: Arguments>(_: fn(&T) -> &F) -> usize {
    F::COUNT
}

macro_rules! slots {
    ($table:ident: $($field:ident $offset:literal $arguments:literal,)*) => {
        let mut seen = 0;
        $(
            assert_eq!(
                sdk_offset(offset_of!($table<(), ()>, $field)),
                $offset,
                stringify!($field)
            );
            assert_eq!(
                count(|table: &$table<(), ()>| &table.$field),
                $arguments,
                stringify!($field)
            );
            seen += 1;
        )*
        assert_eq!(size_of::<$table<(), ()>>(), seen * size_of::<usize>());
    };
}

/// All 66 of `ddraw.h`, the literal `s4-ddraw-h.txt` read from the header: each
/// slot's offset and its arguments, `this` counted, so `stdcall` pops right.
#[test]
fn every_slot_stands_at_its_sdk_offset_with_its_arguments() {
    assert_eq!(offset_of!(Interface<(), ()>, table), 0);
    slots!(ObjectTable:
        query_interface 0x00 3, add_ref 0x04 1, release 0x08 1, compact 0x0c 1,
        create_clipper 0x10 4, create_palette 0x14 5, create_surface 0x18 4,
        duplicate_surface 0x1c 3, enum_display_modes 0x20 5, enum_surfaces 0x24 5,
        flip_to_gdi_surface 0x28 1, get_caps 0x2c 3, get_display_mode 0x30 2,
        get_four_cc_codes 0x34 3, get_gdi_surface 0x38 2, get_monitor_frequency 0x3c 2,
        get_scan_line 0x40 2, get_vertical_blank_status 0x44 2, initialize 0x48 2,
        restore_display_mode 0x4c 1, set_cooperative_level 0x50 3, set_display_mode 0x54 4,
        wait_for_vertical_blank 0x58 3,
    );
    slots!(SurfaceTable:
        query_interface 0x00 3, add_ref 0x04 1, release 0x08 1, add_attached_surface 0x0c 2,
        add_overlay_dirty_rect 0x10 2, blt 0x14 6, blt_batch 0x18 4, blt_fast 0x1c 6,
        delete_attached_surface 0x20 3, enum_attached_surfaces 0x24 3,
        enum_overlay_z_orders 0x28 4, flip 0x2c 3, get_attached_surface 0x30 3,
        get_blt_status 0x34 2, get_caps 0x38 2, get_clipper 0x3c 2, get_color_key 0x40 3,
        get_dc 0x44 2, get_flip_status 0x48 2, get_overlay_position 0x4c 3, get_palette 0x50 2,
        get_pixel_format 0x54 2, get_surface_desc 0x58 2, initialize 0x5c 3, is_lost 0x60 1,
        lock 0x64 5, release_dc 0x68 2, restore 0x6c 1, set_clipper 0x70 2, set_color_key 0x74 3,
        set_overlay_position 0x78 3, set_palette 0x7c 2, unlock 0x80 2, update_overlay 0x84 6,
        update_overlay_display 0x88 2, update_overlay_z_order 0x8c 3,
    );
    slots!(PaletteTable:
        query_interface 0x00 3, add_ref 0x04 1, release 0x08 1, get_caps 0x0c 2,
        get_entries 0x10 5, initialize 0x14 4, set_entries 0x18 5,
    );
}

/// 3 flips of planted bytes: each back moves up, the primary's old pixels go to
/// the last, and the back's address cycles as the trace's 3 did.
#[test]
fn flips_turn_the_chain_and_the_lock_address_cycles() {
    let parts = parts();
    let surfaces = &parts.surfaces;
    let (primary, back) = chain(surfaces);
    let lock = || {
        let mut desc = [0; DESC_WORDS];
        assert_eq!(surfaces.lock(back, None, &mut desc, 1), DD_OK);
        desc[DESC_SURFACE]
    };
    let first = lock();
    plant(surfaces, back, 0x22);
    assert_eq!(surfaces.flip(primary, 0, 1), DD_OK);
    assert_eq!(shown(&parts), 0x22);
    let second = lock();
    plant(surfaces, back, 0x33);
    assert_eq!(surfaces.flip(primary, 0, 1), DD_OK);
    assert_eq!(shown(&parts), 0x33);
    let third = lock();
    assert_eq!(surfaces.flip(primary, 0, 1), DD_OK);
    assert_eq!(shown(&parts), 0);
    assert_eq!(lock(), first);
    assert_eq!(surfaces.flip(primary, 0, 1), DD_OK);
    assert_eq!(shown(&parts), 0x22);
    assert_eq!(lock(), second);
    assert_ne!(first, second);
    assert_ne!(second, third);
    assert_ne!(first, third);
}

/// The game file's `Flip(primary, third, 1)`, no run reaching it: with an override
/// the two swap alone, so two flips put them back.
#[test]
fn a_flip_with_an_override_swaps_the_two_alone() {
    let parts = parts();
    let surfaces = &parts.surfaces;
    let (primary, back) = chain(surfaces);
    plant(surfaces, back, 0x22);
    assert_eq!(surfaces.flip(primary, back, 1), DD_OK);
    assert_eq!(shown(&parts), 0x22);
    assert_eq!(surfaces.flip(primary, back, 1), DD_OK);
    assert_eq!(shown(&parts), 0);
    assert_eq!(surfaces.flip(primary, primary, 1), DDERR_INVALIDPARAMS);
    assert_eq!(surfaces.flip(back, 0, 1), DDERR_NOTFLIPPABLE);
    assert_eq!(surfaces.flip(99, 0, 1), DDERR_INVALIDOBJECT);
}

/// The last flip wins and a present no one read is a `draw.drop` line: a flip
/// never waits for the page.
#[test]
fn the_last_flip_wins_and_a_dropped_flip_is_a_line() {
    let parts = parts();
    let surfaces = &parts.surfaces;
    let (primary, back) = chain(surfaces);
    let mut palette = 0;
    let mut entries = [0_u32; 256];
    entries[0x22] = 0x00ff_0000;
    entries[0x33] = 0x0000_00ff;
    assert_eq!(surfaces.create_palette(0x44, &entries, &mut palette), DD_OK);
    assert_eq!(surfaces.set_palette(primary, palette), DD_OK);
    plant(surfaces, back, 0x22);
    let lines = debug_lines(|| {
        assert_eq!(surfaces.flip(primary, 0, 1), DD_OK);
        plant(surfaces, back, 0x33);
        assert_eq!(surfaces.flip(primary, 0, 1), DD_OK);
    });
    assert_eq!(
        lines
            .iter()
            .filter(|line| line.starts_with("draw.drop"))
            .count(),
        2
    );
    let mut rgba = [0_u8; 4];
    assert!(parts.page.read(&mut rgba));
    assert_eq!(
        rgba,
        [0xff, 0, 0, 0xff],
        "0x33's entry, red in the low byte"
    );
    assert!(!parts.page.read(&mut rgba));
}

/// A fade flips nothing, so `SetEntries` on the primary's palette shows and
/// presents, and one on a palette no primary holds does neither.
#[test]
fn set_entries_presents_on_the_primarys_palette_alone() {
    let parts = parts();
    let surfaces = &parts.surfaces;
    let (primary, _) = chain(surfaces);
    let (mut attached, mut other) = (0, 0);
    assert_eq!(
        surfaces.create_palette(0x44, &[0; 256], &mut attached),
        DD_OK
    );
    assert_eq!(surfaces.create_palette(0x44, &[0; 256], &mut other), DD_OK);
    assert_eq!(surfaces.set_palette(primary, attached), DD_OK);
    let mut rgba = [0_u8; 4];
    assert!(parts.page.read(&mut rgba), "SetPalette itself presents");
    assert_eq!(surfaces.set_entries(other, 0, 0, &[0xff]), DD_OK);
    assert!(!parts.page.read(&mut rgba));
    assert_eq!(surfaces.set_entries(attached, 0, 0, &[0xff]), DD_OK);
    assert!(parts.page.read(&mut rgba));
    assert_eq!(rgba, [0xff, 0, 0, 0xff]);
    assert_eq!(
        surfaces.set_entries(0x7777, 0, 0, &[0xff]),
        DDERR_INVALIDOBJECT
    );
}

/// The trace's two fills: `(0,0)-(320,200)` is the whole, `(0,0)-(319,199)` leaves
/// the last row and column: right and bottom are exclusive.
#[test]
fn a_fill_writes_the_low_byte_over_the_rect_right_and_bottom_exclusive() {
    let parts = parts();
    let surfaces = &parts.surfaces;
    let (_, back) = chain(surfaces);
    assert_eq!(
        surfaces.blt(back, Some([0, 0, 63, 31]), (0, None), 0x0100_0400, 0xabcd),
        DD_OK
    );
    surfaces.with_surface(back, |buffer| {
        assert_eq!(buffer.pixel(0, 0), 0xcd);
        assert_eq!(buffer.pixel(62, 30), 0xcd);
        assert_eq!(buffer.pixel(63, 30), 0);
        assert_eq!(buffer.pixel(62, 31), 0);
    });
    assert_eq!(
        surfaces.blt(back, None, (0, None), DDBLT_COLORFILL, 7),
        DD_OK
    );
    surfaces.with_surface(back, |buffer| assert_eq!(buffer.pixel(63, 31), 7));
    assert_eq!(
        surfaces.blt(
            back,
            Some([-9, -9, 900, 900]),
            (0, None),
            DDBLT_COLORFILL,
            8
        ),
        DD_OK,
        "a wild rectangle lands what fits"
    );
    assert_eq!(
        surfaces.blt(back, None, (0, None), 0x800, 0),
        DDERR_UNSUPPORTED
    );
}

/// A fill or a copy onto the primary presents; onto a back it does not.
#[test]
fn a_blt_onto_the_primary_presents_and_a_copy_lands() {
    let parts = parts();
    let surfaces = &parts.surfaces;
    let (primary, back) = chain(surfaces);
    let mut rgba = [0_u8; 4];
    assert_eq!(
        surfaces.blt(back, None, (0, None), DDBLT_COLORFILL, 5),
        DD_OK
    );
    assert!(!parts.page.read(&mut rgba));
    let whole = [0, 0, 64, 32];
    assert_eq!(
        surfaces.blt(primary, Some(whole), (back, Some(whole)), DDBLT_WAIT, 0),
        DD_OK
    );
    assert!(parts.page.read(&mut rgba));
    assert_eq!(shown(&parts), 5);
    assert_eq!(
        surfaces.blt(primary, None, (99, None), DDBLT_WAIT, 0),
        DDERR_INVALIDOBJECT
    );
}

/// The trace's 1,626 `BltFast`s are all of the back onto itself, nothing and 0.
#[test]
fn blt_fast_onto_itself_is_nothing_and_elsewhere_copies() {
    let parts = parts();
    let surfaces = &parts.surfaces;
    let (primary, back) = chain(surfaces);
    plant(surfaces, back, 9);
    let mut rgba = [0_u8; 4];
    assert_eq!(
        surfaces.blt_fast((back, 0, 0), back, None, DDBLTFAST_WAIT),
        DD_OK
    );
    assert!(!parts.page.read(&mut rgba));
    assert_eq!(shown(&parts), 0);
    let rect = Some([0, 0, 4, 4]);
    assert_eq!(
        surfaces.blt_fast((primary, 0, 0), back, rect, DDBLTFAST_WAIT),
        DD_OK
    );
    assert!(parts.page.read(&mut rgba));
    assert_eq!(shown(&parts), 9);
    assert_eq!(parts.easel.shown().pixels[4], 0, "unscaled, 4 wide");
    assert_eq!(
        surfaces.blt_fast((primary, 0, 0), back, None, 1),
        DDERR_UNSUPPORTED
    );
    assert_eq!(
        surfaces.blt_fast((primary, 0, 0), 99, None, 0),
        DDERR_INVALIDOBJECT
    );
}

/// `Lock`'s two fields, what its callers read; the rest of the record untouched.
#[test]
fn lock_writes_the_pointer_and_the_pitch_and_leaves_the_rest() {
    let parts = parts();
    let surfaces = &parts.surfaces;
    let (_, back) = chain(surfaces);
    let mut desc = [0x5a5a_5a5a; DESC_WORDS];
    assert_eq!(surfaces.lock(back, None, &mut desc, 1), DD_OK);
    assert_eq!(desc[DESC_PITCH], 64);
    assert_ne!(desc[DESC_SURFACE], 0);
    assert_eq!(desc[DESC_WIDTH], 0x5a5a_5a5a);
    assert_eq!(surfaces.unlock(back), DD_OK);
    assert_eq!(
        surfaces.lock(back, Some([0; 4]), &mut desc, 1),
        DDERR_UNSUPPORTED
    );
    assert_eq!(surfaces.lock(99, None, &mut desc, 1), DDERR_INVALIDOBJECT);
    assert_eq!(surfaces.unlock(99), DDERR_INVALIDOBJECT);
}

/// What `GetSurfaceDesc`'s one reader takes, the pitch, and the rest as `ddraw.h`.
#[test]
fn surface_desc_answers_the_shape() {
    let parts = parts();
    let surfaces = &parts.surfaces;
    let (primary, back) = chain(surfaces);
    let mut desc = [0; DESC_WORDS];
    assert_eq!(
        surfaces.get_surface_desc(back, &mut desc),
        DDERR_INVALIDPARAMS
    );
    desc[DESC_SIZE] = DESC_BYTES;
    assert_eq!(surfaces.get_surface_desc(back, &mut desc), DD_OK);
    assert_eq!(desc[DESC_PITCH], 64);
    assert_eq!((desc[DESC_WIDTH], desc[DESC_HEIGHT]), (64, 32));
    assert_eq!(desc[DESC_FORMAT_BITS], 8);
    assert_eq!(desc[DESC_CAPS], 0x81c, "Wine's own line for this surface");
    assert_eq!(surfaces.get_surface_desc(primary, &mut desc), DD_OK);
    assert_eq!(desc[DESC_BACK_COUNT], 2);
    assert_eq!(desc[DESC_CAPS], 0x238);
    assert_eq!(
        surfaces.get_surface_desc(99, &mut desc),
        DDERR_INVALIDOBJECT
    );
}

/// `GetCaps`' caller refuses the bank-switched bit: it stays clear, the size left.
#[test]
fn get_caps_clears_dw_caps_and_leaves_the_size() {
    let parts = parts();
    let mut driver = [0xac, u32::MAX];
    assert_eq!(parts.surfaces.get_caps(Some(&mut driver), None), DD_OK);
    assert_eq!(driver, [0xac, 0]);
}

/// A number no row holds is refused, the caller's own error path.
#[test]
fn what_no_row_holds_is_refused() {
    let parts = parts();
    let surfaces = &parts.surfaces;
    assert_eq!(surfaces.set_cooperative_level(1, 0x51), DD_OK);
    assert_eq!(surfaces.set_cooperative_level(1, 8), DD_OK);
    assert_eq!(surfaces.set_cooperative_level(1, 9), DDERR_INVALIDPARAMS);
    assert_eq!(surfaces.set_display_mode(320, 200, 8), DD_OK);
    assert_eq!(parts.easel.shown().width, 320);
    assert_eq!(surfaces.set_display_mode(320, 200, 16), DDERR_INVALIDMODE);
    assert_eq!(surfaces.set_display_mode(321, 200, 8), DDERR_INVALIDMODE);
    assert_eq!(
        parts.easel.shown().width,
        320,
        "a refused mode leaves the screen"
    );
    let mut out = 0x77;
    assert_eq!(
        surfaces.create_surface(&chain_desc(3), &mut out),
        DDERR_OUTOFMEMORY
    );
    let mut wrong = chain_desc(2);
    wrong[DESC_SIZE] = 1;
    assert_eq!(
        surfaces.create_surface(&wrong, &mut out),
        DDERR_INVALIDPARAMS
    );
    assert_eq!(surfaces.create_palette(0, &[], &mut out), DDERR_UNSUPPORTED);
    assert_eq!(out, 0x77, "a refusal writes nothing");
    assert_eq!(
        surfaces.get_attached_surface(1, 0x40, &mut out),
        DDERR_NOTFOUND
    );
}

/// Both caps answer the one surface, its count 2 then 3, the trace's lines; the
/// primary's one `Release` to 0 frees the chain, and its palette keeps a count of
/// 1, the trace's `decreasing refcount to 1`.
#[test]
fn a_chain_answers_one_surface_and_goes_with_its_primary() {
    let parts = parts();
    let surfaces = &parts.surfaces;
    let (primary, back) = chain(surfaces);
    let mut again = 0;
    assert_eq!(
        surfaces.get_attached_surface(primary, DDSCAPS_FLIP, &mut again),
        DD_OK
    );
    assert_eq!(again, back);
    let mut palette = 0;
    assert_eq!(
        surfaces.create_palette(0x44, &[0; 256], &mut palette),
        DD_OK
    );
    assert_eq!(surfaces.set_palette(primary, palette), DD_OK);
    assert_eq!(surfaces.release_surface(back), 2, "a back never goes alone");
    assert!(surfaces.with_surface(back, |_| ()).is_some());
    assert_eq!(surfaces.release_surface(primary), 0);
    assert!(surfaces.with_surface(back, |_| ()).is_none());
    assert!(surfaces.with_surface(primary, |_| ()).is_none());
    assert_eq!(surfaces.release_palette(palette), 0);
    assert_eq!(surfaces.release_surface(primary), 0, "no such answers 0");
}

/// A plain surface, a caller's own buffer in a mode other than 320 by 200: the
/// reading of the callers, no run reaching it.
#[test]
fn a_plain_surface_is_sized_by_its_description_and_goes_alone() {
    let parts = parts();
    let surfaces = &parts.surfaces;
    let mut desc = [0; DESC_WORDS];
    desc[DESC_SIZE] = DESC_BYTES;
    desc[DESC_FLAGS] = 6;
    desc[DESC_WIDTH] = 16;
    desc[DESC_HEIGHT] = 8;
    desc[DESC_CAPS] = 0x840;
    let mut plain = 0;
    assert_eq!(surfaces.create_surface(&desc, &mut plain), DD_OK);
    surfaces.with_surface(plain, |buffer| assert_eq!(buffer.pixels.len(), 16 * 8));
    assert_eq!(surfaces.release_surface(plain), 0);
    assert!(surfaces.with_surface(plain, |_| ()).is_none());
    desc[DESC_FLAGS] = 0;
    assert_eq!(
        surfaces.create_surface(&desc, &mut plain),
        DDERR_INVALIDPARAMS
    );
}

/// The object's `Release` to 0 takes everything; the reading of its callers.
#[test]
fn the_objects_release_frees_all() {
    let parts = parts();
    let surfaces = &parts.surfaces;
    assert_eq!(surfaces.direct_draw_create(0, 0), DD_OK);
    let (primary, _) = chain(surfaces);
    let mut palette = 0;
    assert_eq!(
        surfaces.create_palette(0x44, &[0; 256], &mut palette),
        DD_OK
    );
    assert_eq!(surfaces.restore_display_mode(), DD_OK);
    assert_eq!(surfaces.is_lost(primary), DD_OK);
    assert_eq!(surfaces.restore(primary), DD_OK);
    assert_eq!(surfaces.release(), 0);
    assert!(surfaces.with_surface(primary, |_| ()).is_none());
    assert_eq!(
        surfaces.set_entries(palette, 0, 0, &[0]),
        DDERR_INVALIDOBJECT
    );
}

/// One `imp.call` line a call, an interface's name in `c`; a palette's entries on
/// `draw.table` lines, 16 to a line.
#[test]
fn each_call_writes_one_line_as_interface_name() {
    let parts = parts();
    let surfaces = &parts.surfaces;
    let lines = debug_lines(|| {
        let _ = surfaces.direct_draw_create(0, 0);
        let _ = surfaces.set_display_mode(320, 200, 8);
        let _ = surfaces.is_lost(9);
        let _ = surfaces.restore_display_mode();
    });
    assert_eq!(
        lines,
        [
            "imp.call c=DDRAW!DirectDrawCreate a=0,0 v=0 by=draw-surfaces",
            "imp.call c=IDirectDraw::SetDisplayMode a=140,c8,8 v=0 by=draw-surfaces",
            "imp.call c=IDirectDrawSurface::IsLost a=9 v=0 by=draw-surfaces",
            "imp.call c=IDirectDraw::RestoreDisplayMode v=0 by=draw-surfaces",
        ]
    );
    let mut palette = 0;
    let lines = debug_lines(|| {
        let _ = surfaces.create_palette(0x44, &[1; 17], &mut palette);
    });
    assert_eq!(lines.len(), 3);
    assert_eq!(
        lines[0],
        format!("imp.call c=IDirectDraw::CreatePalette a=44 o={palette:x} v=0 by=draw-surfaces")
    );
    assert!(lines[2].starts_with("draw.table c=IDirectDraw::CreatePalette i=10 a=1"));
}

type ComRig = Com<&'static Page, Host>;
type This = *mut Interface<&'static Page, Host>;

#[expect(unsafe_code, reason = "the created word is this test's own")]
fn com() -> (&'static ComRig, This) {
    let parts = parts();
    let com: &'static ComRig = Box::leak(Box::new(Com::new(parts.surfaces)));
    let mut object: This = core::ptr::null_mut();
    // SAFETY: `object` is this test's own word.
    let answer = unsafe { com.direct_draw_create(0, &raw mut object, 0) };
    assert_eq!(answer, DD_OK);
    (com, object)
}

/// The word at `offset` of `this`'s table: what a port reads as `*(*obj + off)`,
/// on the host's own slot size.
///
/// # Safety
/// `this` is live and its table holds that slot.
#[expect(unsafe_code, reason = "reading a table as a program does")]
unsafe fn slot(this: This, offset: usize) -> usize {
    // SAFETY: by this function's contract.
    unsafe {
        let table = (*this).table.cast::<usize>();
        *table.add(offset / 4)
    }
}

/// A slot called through the raw table, the object first, as a port calls it: the
/// init and a frame of the trace, through the tables alone.
#[test]
#[expect(unsafe_code, reason = "calling a table as a program does")]
fn a_program_walks_the_tables() {
    type Create = unsafe extern "system" fn(This, *const Desc, *mut This, u32) -> u32;
    type Attached = unsafe extern "system" fn(This, *const u32, *mut This) -> u32;
    type Lost = unsafe extern "system" fn(This) -> u32;
    type Fill = unsafe extern "system" fn(
        This,
        *const [i32; 4],
        This,
        *const [i32; 4],
        u32,
        *const [u32; FX_WORDS],
    ) -> u32;
    type Lock = unsafe extern "system" fn(This, *const [i32; 4], *mut Desc, u32, u32) -> u32;
    type Unlock = unsafe extern "system" fn(This, u32) -> u32;
    type Flip = unsafe extern "system" fn(This, This, u32) -> u32;
    type NewPalette = unsafe extern "system" fn(This, u32, *const u32, *mut This, u32) -> u32;
    type SetPalette = unsafe extern "system" fn(This, This) -> u32;
    type Entries = unsafe extern "system" fn(This, u32, u32, u32, *const u32) -> u32;
    type Absent = unsafe extern "system" fn(This, u32) -> u32;
    let (com, object) = com();
    // SAFETY: each `transmute` gives a slot the type `ddraw.h` declares, which
    // `every_slot_stands_at_its_sdk_offset_with_its_arguments` holds; each pointer
    // handed is this test's own, alive across its call.
    unsafe {
        let create: Create = core::mem::transmute(slot(object, 0x18));
        let mut primary: This = core::ptr::null_mut();
        let desc = chain_desc(2);
        assert_eq!(create(object, &raw const desc, &raw mut primary, 0), DD_OK);
        let attached: Attached = core::mem::transmute(slot(primary, 0x30));
        let (mut back, mut again): (This, This) = (core::ptr::null_mut(), core::ptr::null_mut());
        let caps = [DDSCAPS_BACKBUFFER, DDSCAPS_FLIP];
        assert_eq!(attached(primary, &raw const caps[0], &raw mut back), DD_OK);
        assert_eq!(attached(primary, &raw const caps[1], &raw mut again), DD_OK);
        assert_eq!(back, again, "both caps answer the one interface");
        assert_ne!(back, primary);
        let lost: Lost = core::mem::transmute(slot(primary, 0x60));
        assert_eq!(lost(primary), DD_OK);
        let fill: Fill = core::mem::transmute(slot(back, 0x14));
        let mut fx = [0_u32; FX_WORDS];
        fx[0] = 100;
        fx[FX_COLOUR] = 0x42;
        let rect = [0, 0, 64, 32];
        assert_eq!(
            fill(
                back,
                &raw const rect,
                core::ptr::null_mut(),
                core::ptr::null(),
                0x0100_0400,
                &raw const fx
            ),
            DD_OK
        );
        let lock: Lock = core::mem::transmute(slot(back, 0x64));
        let mut locked = [0; DESC_WORDS];
        assert_eq!(lock(back, core::ptr::null(), &raw mut locked, 1, 0), DD_OK);
        assert_eq!(locked[DESC_PITCH], 64);
        let unlock: Unlock = core::mem::transmute(slot(back, 0x80));
        assert_eq!(unlock(back, locked[DESC_SURFACE]), DD_OK);
        let flip: Flip = core::mem::transmute(slot(primary, 0x2c));
        assert_eq!(flip(primary, core::ptr::null_mut(), 1), DD_OK);
        let new_palette: NewPalette = core::mem::transmute(slot(object, 0x14));
        let entries = [0_u32; 256];
        let mut palette: This = core::ptr::null_mut();
        assert_eq!(
            new_palette(object, 0x44, entries.as_ptr(), &raw mut palette, 0),
            DD_OK
        );
        let set_palette: SetPalette = core::mem::transmute(slot(primary, 0x7c));
        assert_eq!(set_palette(primary, palette), DD_OK);
        let set_entries: Entries = core::mem::transmute(slot(palette, 0x18));
        let colours = [0x0000_00ff_u32; 256];
        assert_eq!(set_entries(palette, 0, 0, 256, colours.as_ptr()), DD_OK);
        // Another game's `GetDC` and the object's own `AddRef`: stubs, never null.
        let get_dc: Absent = core::mem::transmute(slot(primary, 0x44));
        let mut answer = 0;
        let lines = debug_lines(|| answer = get_dc(primary, 0));
        assert_eq!(answer, DDERR_UNSUPPORTED);
        assert_eq!(lines, ["surfaces.absent c=IDirectDrawSurface i=44"]);
        let add_ref: Lost = core::mem::transmute(slot(palette, 0x04));
        let lines = debug_lines(|| answer = add_ref(palette));
        assert_eq!(answer, DDERR_UNSUPPORTED);
        assert_eq!(lines, ["surfaces.absent c=IDirectDrawPalette i=4"]);
        let flip_to_gdi: Lost = core::mem::transmute(slot(object, 0x28));
        let lines = debug_lines(|| answer = flip_to_gdi(object));
        assert_eq!(answer, DDERR_UNSUPPORTED);
        assert_eq!(lines, ["surfaces.absent c=IDirectDraw i=28"]);
        let release: Lost = core::mem::transmute(slot(primary, 0x08));
        assert_eq!(release(primary), 0);
    }
    assert!(com.surfaces().with_surface(1, |_| ()).is_none());
}
