// SPDX-License-Identifier: MIT
//! The shape a program calls: an interface whose first word is its table, the
//! tables in `ddraw.h`'s order, each slot `extern "system"`, stdcall on a 32-bit
//! Windows, so a front pops its own arguments as the caller expects.
//!
//! A program reads `*(*object + offset)` and calls it with the object first.
//! Every slot holds a function: a served one its front, the others a stub of the
//! slot's argument count ([`crate::surfaces::Surfaces::absent`]), never null.
//!
//! [`Com::direct_draw_create`] is where a target's exported `DirectDrawCreate`
//! lands. A [`Com`] lives as long as the program, hence its `'static`.
#![expect(
    unsafe_code,
    reason = "the seam a program's raw calls land on: each front reads its pointers"
)]

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use core::cell::RefCell;
use core::mem::{offset_of, size_of};

use draw::easel::Back;
use draw::front::Windows;

use crate::sdk::{
    DD_OK, DDBLT_COLORFILL, DDERR_INVALIDPARAMS, DDPCAPS_8BIT, Desc, FX_COLOUR, FX_WORDS,
    PALETTE_ENTRIES,
};
use crate::surfaces::Surfaces;

const OBJECT: u8 = 0;
const SURFACE: u8 = 1;
const PALETTE: u8 = 2;
/// Each interface's SDK name, by its kind.
const NAMES: [&str; 3] = ["IDirectDraw", "IDirectDrawSurface", "IDirectDrawPalette"];

/// A Windows `RECT`.
type Rect = [i32; 4];
type This<B, W> = *mut Interface<B, W>;
/// A slot's function by its arguments, `this` counted.
type Absent1<B, W> = unsafe extern "system" fn(This<B, W>) -> u32;
type Absent2<B, W> = unsafe extern "system" fn(This<B, W>, u32) -> u32;
type Absent3<B, W> = unsafe extern "system" fn(This<B, W>, u32, u32) -> u32;
type Absent4<B, W> = unsafe extern "system" fn(This<B, W>, u32, u32, u32) -> u32;
type Absent5<B, W> = unsafe extern "system" fn(This<B, W>, u32, u32, u32, u32) -> u32;
type Absent6<B, W> = unsafe extern "system" fn(This<B, W>, u32, u32, u32, u32, u32) -> u32;

/// The interfaces handed out, by kind and id; a box's address never moves.
type Made<B, W> = BTreeMap<(u8, u32), Box<Interface<B, W>>>;

/// One object, surface or palette as a program holds it: the table first.
#[repr(C)]
pub struct Interface<B: 'static, W: 'static> {
    pub(crate) table: *const (),
    com: *const Com<B, W>,
    id: u32,
}

/// One program's [`Surfaces`] and the interfaces handed out for them.
pub struct Com<B: 'static, W: 'static> {
    surfaces: Surfaces<'static, B, W>,
    made: RefCell<Made<B, W>>,
}

impl<B: Back + 'static, W: Windows + 'static> Com<B, W> {
    #[must_use]
    pub fn new(surfaces: Surfaces<'static, B, W>) -> Self {
        Self {
            surfaces,
            made: RefCell::new(BTreeMap::new()),
        }
    }

    #[must_use]
    pub const fn surfaces(&self) -> &Surfaces<'static, B, W> {
        &self.surfaces
    }

    /// The one interface of `kind`'s `id`, made at its first asking and kept until
    /// this goes, so a program's stale pointer still lands on a table.
    fn interface(&'static self, kind: u8, id: u32) -> This<B, W> {
        let table = match kind {
            OBJECT => core::ptr::from_ref(ObjectTable::<B, W>::STATIC).cast(),
            SURFACE => core::ptr::from_ref(SurfaceTable::<B, W>::STATIC).cast(),
            _ => core::ptr::from_ref(PaletteTable::<B, W>::STATIC).cast(),
        };
        let mut made = self.made.borrow_mut();
        let kept = made.entry((kind, id)).or_insert_with(|| {
            Box::new(Interface {
                table,
                com: self,
                id,
            })
        });
        &raw mut **kept
    }

    /// `DirectDrawCreate`: 0 and `out` the object; `DDERR_INVALIDPARAMS` for a null
    /// `out`. `guid` and `outer` are shown and not read.
    ///
    /// # Safety
    /// `out` is null or points at a word the caller owns.
    pub unsafe fn direct_draw_create(
        &'static self,
        guid: u32,
        out: *mut This<B, W>,
        outer: u32,
    ) -> u32 {
        if out.is_null() {
            return DDERR_INVALIDPARAMS;
        }
        let answer = self.surfaces.direct_draw_create(guid, outer);
        // SAFETY: not null, and the caller's word by this function's contract.
        unsafe { *out = self.interface(OBJECT, 0) };
        answer
    }
}

/// The program's interface behind `this`: its [`Com`] and its id.
///
/// # Safety
/// `this` came from [`Com::interface`], whose box lives as long as its `'static`
/// [`Com`].
unsafe fn parts<B: 'static, W: 'static>(this: This<B, W>) -> (&'static Com<B, W>, u32) {
    // SAFETY: by this function's contract `this` is a live `Interface`, and its
    // `com` is the `'static` that made it.
    unsafe { (&*(*this).com, (*this).id) }
}

/// The id of a surface a program hands over, 0 for null.
///
/// # Safety
/// `surface` is null or as [`parts`] asks.
unsafe fn id_of<B: 'static, W: 'static>(surface: This<B, W>) -> u32 {
    // SAFETY: not null, and a live `Interface` by this function's contract.
    unsafe { surface.as_ref() }.map_or(0, |kept| kept.id)
}

/// The caller's `count` entries, none for a null `entries`.
///
/// # Safety
/// `entries` is null or holds `count` 32-bit words, aligned.
unsafe fn words<'a>(entries: *const u32, count: usize) -> &'a [u32] {
    if entries.is_null() {
        return &[];
    }
    // SAFETY: not null and holding `count` by this function's contract.
    unsafe { core::slice::from_raw_parts(entries, count) }
}

/// The answer of a slot with no front.
///
/// # Safety
/// As [`parts`].
unsafe fn absent<B: Back + 'static, W: Windows + 'static>(
    this: This<B, W>,
    kind: u8,
    slot: u32,
) -> u32 {
    // SAFETY: passed on from this function's contract.
    let (com, _) = unsafe { parts(this) };
    com.surfaces.absent(NAMES[kind as usize], slot)
}

/// A stub by its argument count, each popped by `extern "system"`.
macro_rules! stubs {
    ($($name:ident($($arg:ident),*);)*) => {$(
        unsafe extern "system" fn $name<
            B: Back + 'static,
            W: Windows + 'static,
            const KIND: u8,
            const SLOT: u32,
        >(
            this: This<B, W>,
            $($arg: u32,)*
        ) -> u32 {
            $(let _ = $arg;)*
            // SAFETY: a program calls a slot with the interface it read it from.
            unsafe { absent(this, KIND, SLOT) }
        }
    )*};
}

stubs! {
    absent1();
    absent2(a);
    absent3(a, b);
    absent4(a, b, c);
    absent5(a, b, c, d);
    absent6(a, b, c, d, e);
}

/// A byte offset in a table as `ddraw.h` numbers it, a slot being 4 bytes on the
/// 32-bit targets a program's calls come from.
#[expect(
    clippy::cast_possible_truncation,
    reason = "a table's offsets are below 0x100"
)]
pub(crate) const fn sdk_offset(at: usize) -> u32 {
    (at / size_of::<usize>() * 4) as u32
}

/// A stub for a table's named slot, its `SLOT` read off that slot's place.
macro_rules! stub {
    ($stub:ident, $kind:expr, $table:ident, $slot:ident) => {
        $stub::<B, W, $kind, { sdk_offset(offset_of!($table<(), ()>, $slot)) }>
    };
}

/// `IDirectDraw`'s table: `ddraw.h`'s 23 in order.
#[repr(C)]
pub struct ObjectTable<B: 'static, W: 'static> {
    pub query_interface: Absent3<B, W>,
    pub add_ref: Absent1<B, W>,
    pub release: Absent1<B, W>,
    pub compact: Absent1<B, W>,
    pub create_clipper: Absent4<B, W>,
    pub create_palette:
        unsafe extern "system" fn(This<B, W>, u32, *const u32, *mut This<B, W>, u32) -> u32,
    pub create_surface:
        unsafe extern "system" fn(This<B, W>, *const Desc, *mut This<B, W>, u32) -> u32,
    pub duplicate_surface: Absent3<B, W>,
    pub enum_display_modes: Absent5<B, W>,
    pub enum_surfaces: Absent5<B, W>,
    pub flip_to_gdi_surface: Absent1<B, W>,
    pub get_caps: unsafe extern "system" fn(This<B, W>, *mut [u32; 2], *mut [u32; 2]) -> u32,
    pub get_display_mode: Absent2<B, W>,
    pub get_four_cc_codes: Absent3<B, W>,
    pub get_gdi_surface: Absent2<B, W>,
    pub get_monitor_frequency: Absent2<B, W>,
    pub get_scan_line: Absent2<B, W>,
    pub get_vertical_blank_status: Absent2<B, W>,
    pub initialize: Absent2<B, W>,
    pub restore_display_mode: Absent1<B, W>,
    pub set_cooperative_level: Absent3<B, W>,
    pub set_display_mode: Absent4<B, W>,
    pub wait_for_vertical_blank: Absent3<B, W>,
}

impl<B: Back + 'static, W: Windows + 'static> ObjectTable<B, W> {
    /// The table a program reads, promoted to live as long as the program.
    const STATIC: &'static Self = &Self::FRONTS;
    const FRONTS: Self = Self {
        query_interface: stub!(absent3, OBJECT, ObjectTable, query_interface),
        add_ref: stub!(absent1, OBJECT, ObjectTable, add_ref),
        release: object_release::<B, W>,
        compact: stub!(absent1, OBJECT, ObjectTable, compact),
        create_clipper: stub!(absent4, OBJECT, ObjectTable, create_clipper),
        create_palette: object_create_palette::<B, W>,
        create_surface: object_create_surface::<B, W>,
        duplicate_surface: stub!(absent3, OBJECT, ObjectTable, duplicate_surface),
        enum_display_modes: stub!(absent5, OBJECT, ObjectTable, enum_display_modes),
        enum_surfaces: stub!(absent5, OBJECT, ObjectTable, enum_surfaces),
        flip_to_gdi_surface: stub!(absent1, OBJECT, ObjectTable, flip_to_gdi_surface),
        get_caps: object_get_caps::<B, W>,
        get_display_mode: stub!(absent2, OBJECT, ObjectTable, get_display_mode),
        get_four_cc_codes: stub!(absent3, OBJECT, ObjectTable, get_four_cc_codes),
        get_gdi_surface: stub!(absent2, OBJECT, ObjectTable, get_gdi_surface),
        get_monitor_frequency: stub!(absent2, OBJECT, ObjectTable, get_monitor_frequency),
        get_scan_line: stub!(absent2, OBJECT, ObjectTable, get_scan_line),
        get_vertical_blank_status: stub!(absent2, OBJECT, ObjectTable, get_vertical_blank_status),
        initialize: stub!(absent2, OBJECT, ObjectTable, initialize),
        restore_display_mode: object_restore_display_mode::<B, W>,
        set_cooperative_level: object_set_cooperative_level::<B, W>,
        set_display_mode: object_set_display_mode::<B, W>,
        wait_for_vertical_blank: stub!(absent3, OBJECT, ObjectTable, wait_for_vertical_blank),
    };
}

/// `IDirectDrawSurface`'s table: `ddraw.h`'s 36 in order.
#[repr(C)]
pub struct SurfaceTable<B: 'static, W: 'static> {
    pub query_interface: Absent3<B, W>,
    pub add_ref: Absent1<B, W>,
    pub release: Absent1<B, W>,
    pub add_attached_surface: Absent2<B, W>,
    pub add_overlay_dirty_rect: Absent2<B, W>,
    pub blt: unsafe extern "system" fn(
        This<B, W>,
        *const Rect,
        This<B, W>,
        *const Rect,
        u32,
        *const [u32; FX_WORDS],
    ) -> u32,
    pub blt_batch: Absent4<B, W>,
    pub blt_fast:
        unsafe extern "system" fn(This<B, W>, u32, u32, This<B, W>, *const Rect, u32) -> u32,
    pub delete_attached_surface: Absent3<B, W>,
    pub enum_attached_surfaces: Absent3<B, W>,
    pub enum_overlay_z_orders: Absent4<B, W>,
    pub flip: unsafe extern "system" fn(This<B, W>, This<B, W>, u32) -> u32,
    pub get_attached_surface:
        unsafe extern "system" fn(This<B, W>, *const u32, *mut This<B, W>) -> u32,
    pub get_blt_status: Absent2<B, W>,
    pub get_caps: Absent2<B, W>,
    pub get_clipper: Absent2<B, W>,
    pub get_color_key: Absent3<B, W>,
    pub get_dc: Absent2<B, W>,
    pub get_flip_status: Absent2<B, W>,
    pub get_overlay_position: Absent3<B, W>,
    pub get_palette: Absent2<B, W>,
    pub get_pixel_format: Absent2<B, W>,
    pub get_surface_desc: unsafe extern "system" fn(This<B, W>, *mut Desc) -> u32,
    pub initialize: Absent3<B, W>,
    pub is_lost: Absent1<B, W>,
    pub lock: unsafe extern "system" fn(This<B, W>, *const Rect, *mut Desc, u32, u32) -> u32,
    pub release_dc: Absent2<B, W>,
    pub restore: Absent1<B, W>,
    pub set_clipper: Absent2<B, W>,
    pub set_color_key: Absent3<B, W>,
    pub set_overlay_position: Absent3<B, W>,
    pub set_palette: unsafe extern "system" fn(This<B, W>, This<B, W>) -> u32,
    pub unlock: Absent2<B, W>,
    pub update_overlay: Absent6<B, W>,
    pub update_overlay_display: Absent2<B, W>,
    pub update_overlay_z_order: Absent3<B, W>,
}

impl<B: Back + 'static, W: Windows + 'static> SurfaceTable<B, W> {
    /// The table a program reads, promoted to live as long as the program.
    const STATIC: &'static Self = &Self::FRONTS;
    const FRONTS: Self = Self {
        query_interface: stub!(absent3, SURFACE, SurfaceTable, query_interface),
        add_ref: stub!(absent1, SURFACE, SurfaceTable, add_ref),
        release: surface_release::<B, W>,
        add_attached_surface: stub!(absent2, SURFACE, SurfaceTable, add_attached_surface),
        add_overlay_dirty_rect: stub!(absent2, SURFACE, SurfaceTable, add_overlay_dirty_rect),
        blt: surface_blt::<B, W>,
        blt_batch: stub!(absent4, SURFACE, SurfaceTable, blt_batch),
        blt_fast: surface_blt_fast::<B, W>,
        delete_attached_surface: stub!(absent3, SURFACE, SurfaceTable, delete_attached_surface),
        enum_attached_surfaces: stub!(absent3, SURFACE, SurfaceTable, enum_attached_surfaces),
        enum_overlay_z_orders: stub!(absent4, SURFACE, SurfaceTable, enum_overlay_z_orders),
        flip: surface_flip::<B, W>,
        get_attached_surface: surface_get_attached_surface::<B, W>,
        get_blt_status: stub!(absent2, SURFACE, SurfaceTable, get_blt_status),
        get_caps: stub!(absent2, SURFACE, SurfaceTable, get_caps),
        get_clipper: stub!(absent2, SURFACE, SurfaceTable, get_clipper),
        get_color_key: stub!(absent3, SURFACE, SurfaceTable, get_color_key),
        get_dc: stub!(absent2, SURFACE, SurfaceTable, get_dc),
        get_flip_status: stub!(absent2, SURFACE, SurfaceTable, get_flip_status),
        get_overlay_position: stub!(absent3, SURFACE, SurfaceTable, get_overlay_position),
        get_palette: stub!(absent2, SURFACE, SurfaceTable, get_palette),
        get_pixel_format: stub!(absent2, SURFACE, SurfaceTable, get_pixel_format),
        get_surface_desc: surface_get_surface_desc::<B, W>,
        initialize: stub!(absent3, SURFACE, SurfaceTable, initialize),
        is_lost: surface_is_lost::<B, W>,
        lock: surface_lock::<B, W>,
        release_dc: stub!(absent2, SURFACE, SurfaceTable, release_dc),
        restore: surface_restore::<B, W>,
        set_clipper: stub!(absent2, SURFACE, SurfaceTable, set_clipper),
        set_color_key: stub!(absent3, SURFACE, SurfaceTable, set_color_key),
        set_overlay_position: stub!(absent3, SURFACE, SurfaceTable, set_overlay_position),
        set_palette: surface_set_palette::<B, W>,
        unlock: surface_unlock::<B, W>,
        update_overlay: stub!(absent6, SURFACE, SurfaceTable, update_overlay),
        update_overlay_display: stub!(absent2, SURFACE, SurfaceTable, update_overlay_display),
        update_overlay_z_order: stub!(absent3, SURFACE, SurfaceTable, update_overlay_z_order),
    };
}

/// `IDirectDrawPalette`'s table: `ddraw.h`'s 7 in order.
#[repr(C)]
pub struct PaletteTable<B: 'static, W: 'static> {
    pub query_interface: Absent3<B, W>,
    pub add_ref: Absent1<B, W>,
    pub release: Absent1<B, W>,
    pub get_caps: Absent2<B, W>,
    pub get_entries: Absent5<B, W>,
    pub initialize: Absent4<B, W>,
    pub set_entries: unsafe extern "system" fn(This<B, W>, u32, u32, u32, *const u32) -> u32,
}

impl<B: Back + 'static, W: Windows + 'static> PaletteTable<B, W> {
    /// The table a program reads, promoted to live as long as the program.
    const STATIC: &'static Self = &Self::FRONTS;
    const FRONTS: Self = Self {
        query_interface: stub!(absent3, PALETTE, PaletteTable, query_interface),
        add_ref: stub!(absent1, PALETTE, PaletteTable, add_ref),
        release: palette_release::<B, W>,
        get_caps: stub!(absent2, PALETTE, PaletteTable, get_caps),
        get_entries: stub!(absent5, PALETTE, PaletteTable, get_entries),
        initialize: stub!(absent4, PALETTE, PaletteTable, initialize),
        set_entries: palette_set_entries::<B, W>,
    };
}

/// Each served slot at `ddraw.h`'s offset, a program's stack holding what its
/// declaration says. A 32-bit target's alone: the host's test reads all 66.
#[cfg(target_arch = "x86")]
const _: () = {
    type O = ObjectTable<(), ()>;
    type D = SurfaceTable<(), ()>;
    type P = PaletteTable<(), ()>;
    assert!(offset_of!(Interface<(), ()>, table) == 0);
    assert!(offset_of!(O, release) == 0x08);
    assert!(offset_of!(O, create_palette) == 0x14);
    assert!(offset_of!(O, create_surface) == 0x18);
    assert!(offset_of!(O, get_caps) == 0x2c);
    assert!(offset_of!(O, restore_display_mode) == 0x4c);
    assert!(offset_of!(O, set_cooperative_level) == 0x50);
    assert!(offset_of!(O, set_display_mode) == 0x54);
    assert!(offset_of!(D, release) == 0x08);
    assert!(offset_of!(D, blt) == 0x14);
    assert!(offset_of!(D, blt_fast) == 0x1c);
    assert!(offset_of!(D, flip) == 0x2c);
    assert!(offset_of!(D, get_attached_surface) == 0x30);
    assert!(offset_of!(D, get_surface_desc) == 0x58);
    assert!(offset_of!(D, is_lost) == 0x60);
    assert!(offset_of!(D, lock) == 0x64);
    assert!(offset_of!(D, restore) == 0x6c);
    assert!(offset_of!(D, set_palette) == 0x7c);
    assert!(offset_of!(D, unlock) == 0x80);
    assert!(offset_of!(P, release) == 0x08);
    assert!(offset_of!(P, set_entries) == 0x18);
};

// The fronts: each takes the interface the program read its slot from, as `parts`
// asks; a pointer it hands is null or its own memory, aligned to 32 bits, as the
// callers' records and locals all are.

unsafe extern "system" fn object_release<B: Back + 'static, W: Windows + 'static>(
    this: This<B, W>,
) -> u32 {
    // SAFETY: see the fronts' note above.
    let (com, _) = unsafe { parts(this) };
    com.surfaces.release()
}

unsafe extern "system" fn object_create_palette<B: Back + 'static, W: Windows + 'static>(
    this: This<B, W>,
    flags: u32,
    entries: *const u32,
    out: *mut This<B, W>,
    _outer: u32,
) -> u32 {
    if out.is_null() {
        return DDERR_INVALIDPARAMS;
    }
    // SAFETY: see the fronts' note above; `DDPCAPS_8BIT` promises 256 entries.
    let (com, entries) = unsafe {
        let count = if flags & DDPCAPS_8BIT == 0 {
            0
        } else {
            PALETTE_ENTRIES
        };
        (parts(this).0, words(entries, count))
    };
    let mut id = 0;
    let answer = com.surfaces.create_palette(flags, entries, &mut id);
    if answer == DD_OK {
        // SAFETY: not null, and the caller's word.
        unsafe { *out = com.interface(PALETTE, id) };
    }
    answer
}

unsafe extern "system" fn object_create_surface<B: Back + 'static, W: Windows + 'static>(
    this: This<B, W>,
    desc: *const Desc,
    out: *mut This<B, W>,
    _outer: u32,
) -> u32 {
    // SAFETY: see the fronts' note above.
    let (com, desc) = unsafe { (parts(this).0, desc.as_ref()) };
    let Some(desc) = desc.filter(|_| !out.is_null()) else {
        return DDERR_INVALIDPARAMS;
    };
    let mut id = 0;
    let answer = com.surfaces.create_surface(desc, &mut id);
    if answer == DD_OK {
        // SAFETY: not null, and the caller's word.
        unsafe { *out = com.interface(SURFACE, id) };
    }
    answer
}

unsafe extern "system" fn object_get_caps<B: Back + 'static, W: Windows + 'static>(
    this: This<B, W>,
    driver: *mut [u32; 2],
    hel: *mut [u32; 2],
) -> u32 {
    // SAFETY: see the fronts' note above; a caps record is past 2 words long.
    let (com, driver, hel) = unsafe { (parts(this).0, driver.as_mut(), hel.as_mut()) };
    com.surfaces.get_caps(driver, hel)
}

unsafe extern "system" fn object_restore_display_mode<B: Back + 'static, W: Windows + 'static>(
    this: This<B, W>,
) -> u32 {
    // SAFETY: see the fronts' note above.
    let (com, _) = unsafe { parts(this) };
    com.surfaces.restore_display_mode()
}

unsafe extern "system" fn object_set_cooperative_level<B: Back + 'static, W: Windows + 'static>(
    this: This<B, W>,
    window: u32,
    flags: u32,
) -> u32 {
    // SAFETY: see the fronts' note above.
    let (com, _) = unsafe { parts(this) };
    com.surfaces.set_cooperative_level(window, flags)
}

unsafe extern "system" fn object_set_display_mode<B: Back + 'static, W: Windows + 'static>(
    this: This<B, W>,
    width: u32,
    height: u32,
    bits: u32,
) -> u32 {
    // SAFETY: see the fronts' note above.
    let (com, _) = unsafe { parts(this) };
    com.surfaces.set_display_mode(width, height, bits)
}

unsafe extern "system" fn surface_release<B: Back + 'static, W: Windows + 'static>(
    this: This<B, W>,
) -> u32 {
    // SAFETY: see the fronts' note above.
    let (com, id) = unsafe { parts(this) };
    com.surfaces.release_surface(id)
}

unsafe extern "system" fn surface_blt<B: Back + 'static, W: Windows + 'static>(
    this: This<B, W>,
    to: *const Rect,
    source: This<B, W>,
    from: *const Rect,
    flags: u32,
    fx: *const [u32; FX_WORDS],
) -> u32 {
    // SAFETY: see the fronts' note above; a fill's caller hands a `DDBLTFX`.
    let ((com, id), to, source, from, fx) = unsafe {
        let fx = fx.as_ref().filter(|_| flags & DDBLT_COLORFILL != 0);
        (
            parts(this),
            to.as_ref().copied(),
            id_of(source),
            from.as_ref().copied(),
            fx,
        )
    };
    let colour = fx.map_or(0, |fx| fx[FX_COLOUR]);
    com.surfaces.blt(id, to, (source, from), flags, colour)
}

unsafe extern "system" fn surface_blt_fast<B: Back + 'static, W: Windows + 'static>(
    this: This<B, W>,
    x: u32,
    y: u32,
    source: This<B, W>,
    from: *const Rect,
    flags: u32,
) -> u32 {
    // SAFETY: see the fronts' note above.
    let ((com, id), source, from) = unsafe { (parts(this), id_of(source), from.as_ref().copied()) };
    com.surfaces.blt_fast((id, x, y), source, from, flags)
}

unsafe extern "system" fn surface_flip<B: Back + 'static, W: Windows + 'static>(
    this: This<B, W>,
    over: This<B, W>,
    flags: u32,
) -> u32 {
    // SAFETY: see the fronts' note above.
    let ((com, id), over) = unsafe { (parts(this), id_of(over)) };
    com.surfaces.flip(id, over, flags)
}

unsafe extern "system" fn surface_get_attached_surface<B: Back + 'static, W: Windows + 'static>(
    this: This<B, W>,
    caps: *const u32,
    out: *mut This<B, W>,
) -> u32 {
    // SAFETY: see the fronts' note above; a `DDSCAPS` is one word.
    let ((com, id), caps) = unsafe { (parts(this), caps.as_ref().copied()) };
    let Some(caps) = caps.filter(|_| !out.is_null()) else {
        return DDERR_INVALIDPARAMS;
    };
    let mut next = 0;
    let answer = com.surfaces.get_attached_surface(id, caps, &mut next);
    if answer == DD_OK {
        // SAFETY: not null, and the caller's word.
        unsafe { *out = com.interface(SURFACE, next) };
    }
    answer
}

unsafe extern "system" fn surface_get_surface_desc<B: Back + 'static, W: Windows + 'static>(
    this: This<B, W>,
    desc: *mut Desc,
) -> u32 {
    // SAFETY: see the fronts' note above.
    let ((com, id), desc) = unsafe { (parts(this), desc.as_mut()) };
    desc.map_or(DDERR_INVALIDPARAMS, |desc| {
        com.surfaces.get_surface_desc(id, desc)
    })
}

unsafe extern "system" fn surface_is_lost<B: Back + 'static, W: Windows + 'static>(
    this: This<B, W>,
) -> u32 {
    // SAFETY: see the fronts' note above.
    let (com, id) = unsafe { parts(this) };
    com.surfaces.is_lost(id)
}

unsafe extern "system" fn surface_lock<B: Back + 'static, W: Windows + 'static>(
    this: This<B, W>,
    rect: *const Rect,
    desc: *mut Desc,
    flags: u32,
    _event: u32,
) -> u32 {
    // SAFETY: see the fronts' note above.
    let ((com, id), rect, desc) = unsafe { (parts(this), rect.as_ref().copied(), desc.as_mut()) };
    desc.map_or(DDERR_INVALIDPARAMS, |desc| {
        com.surfaces.lock(id, rect, desc, flags)
    })
}

unsafe extern "system" fn surface_restore<B: Back + 'static, W: Windows + 'static>(
    this: This<B, W>,
) -> u32 {
    // SAFETY: see the fronts' note above.
    let (com, id) = unsafe { parts(this) };
    com.surfaces.restore(id)
}

unsafe extern "system" fn surface_set_palette<B: Back + 'static, W: Windows + 'static>(
    this: This<B, W>,
    palette: This<B, W>,
) -> u32 {
    // SAFETY: see the fronts' note above.
    let ((com, id), palette) = unsafe { (parts(this), id_of(palette)) };
    com.surfaces.set_palette(id, palette)
}

unsafe extern "system" fn surface_unlock<B: Back + 'static, W: Windows + 'static>(
    this: This<B, W>,
    _data: u32,
) -> u32 {
    // SAFETY: see the fronts' note above.
    let (com, id) = unsafe { parts(this) };
    com.surfaces.unlock(id)
}

unsafe extern "system" fn palette_release<B: Back + 'static, W: Windows + 'static>(
    this: This<B, W>,
) -> u32 {
    // SAFETY: see the fronts' note above.
    let (com, id) = unsafe { parts(this) };
    com.surfaces.release_palette(id)
}

unsafe extern "system" fn palette_set_entries<B: Back + 'static, W: Windows + 'static>(
    this: This<B, W>,
    flags: u32,
    start: u32,
    count: u32,
    entries: *const u32,
) -> u32 {
    let count = (count as usize).min(PALETTE_ENTRIES);
    // SAFETY: see the fronts' note above; the caller's `count` at most 256.
    let ((com, id), entries) = unsafe { (parts(this), words(entries, count)) };
    com.surfaces.set_entries(id, flags, start, entries)
}
