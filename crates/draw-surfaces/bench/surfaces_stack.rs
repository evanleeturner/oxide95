// SPDX-License-Identifier: MIT
//! Every slot of the three tables called as a program it never met calls them: by
//! offset, from this program's own list of names and argument counts in `ddraw.h`'s
//! order, with `esp` read on each side. The crate's test reads Rust's types; this reads
//! the stack, which only a 32-bit target has in this shape.
//!
//! It prints one row a call and judges nothing. A wrong answer is planted at run time, by
//! pointing an interface's first word at a changed copy of its table, so none is in `src/`.
//! Run under Wine: `surfaces_stack <check> <plant> <frames>`, the first and last unread.
#![expect(
    unsafe_code,
    reason = "raw calls by offset are what this program reads"
)]

use std::process::ExitCode;

/// This program reads a 32-bit stack, and no other target has one.
#[cfg(not(target_arch = "x86"))]
fn main() -> ExitCode {
    println!("no run: a 32-bit target's alone");
    ExitCode::FAILURE
}

#[cfg(target_arch = "x86")]
fn main() -> ExitCode {
    stack::run()
}

#[cfg(target_arch = "x86")]
#[path = "wine_window.rs"]
mod wine_window;

#[cfg(target_arch = "x86")]
mod stack {
    use std::process::ExitCode;
    use std::sync::Mutex;

    use draw::easel::Easel;
    use draw::user32::{Real, Window};
    use draw_surfaces::com::{Com, Interface};
    use draw_surfaces::sdk::{
        DD_OK, DDBLT_COLORFILL, DDBLT_WAIT, DDBLTFAST_WAIT, DDERR_INVALIDMODE, DDERR_UNSUPPORTED,
        DDSCAPS_BACKBUFFER, DDSCAPS_COMPLEX, DDSCAPS_FLIP, DDSCAPS_PRIMARYSURFACE,
        DDSCAPS_SYSTEMMEMORY, DDSD_BACKBUFFERCOUNT, DDSD_CAPS, DDSD_HEIGHT, DDSD_WIDTH,
        DESC_BACK_COUNT, DESC_BYTES, DESC_CAPS, DESC_FLAGS, DESC_HEIGHT, DESC_SIZE, DESC_SURFACE,
        DESC_WIDTH, DESC_WORDS, Desc, FX_COLOUR, FX_WORDS, PALETTE_ENTRIES,
    };
    use draw_surfaces::surfaces::Surfaces;
    use draw_surfaces::table::Row;

    use super::wine_window::make_window;

    /// A slot's name and its arguments, `this` counted, as `ddraw.h` declares it.
    type Slot = (&'static str, usize);

    const OBJECT: &str = "IDirectDraw";
    const SURFACE: &str = "IDirectDrawSurface";
    const PALETTE: &str = "IDirectDrawPalette";
    const OBJECT_SLOTS: [Slot; 23] = [
        ("QueryInterface", 3),
        ("AddRef", 1),
        ("Release", 1),
        ("Compact", 1),
        ("CreateClipper", 4),
        ("CreatePalette", 5),
        ("CreateSurface", 4),
        ("DuplicateSurface", 3),
        ("EnumDisplayModes", 5),
        ("EnumSurfaces", 5),
        ("FlipToGDISurface", 1),
        ("GetCaps", 3),
        ("GetDisplayMode", 2),
        ("GetFourCCCodes", 3),
        ("GetGDISurface", 2),
        ("GetMonitorFrequency", 2),
        ("GetScanLine", 2),
        ("GetVerticalBlankStatus", 2),
        ("Initialize", 2),
        ("RestoreDisplayMode", 1),
        ("SetCooperativeLevel", 3),
        ("SetDisplayMode", 4),
        ("WaitForVerticalBlank", 3),
    ];
    const SURFACE_SLOTS: [Slot; 36] = [
        ("QueryInterface", 3),
        ("AddRef", 1),
        ("Release", 1),
        ("AddAttachedSurface", 2),
        ("AddOverlayDirtyRect", 2),
        ("Blt", 6),
        ("BltBatch", 4),
        ("BltFast", 6),
        ("DeleteAttachedSurface", 3),
        ("EnumAttachedSurfaces", 3),
        ("EnumOverlayZOrders", 4),
        ("Flip", 3),
        ("GetAttachedSurface", 3),
        ("GetBltStatus", 2),
        ("GetCaps", 2),
        ("GetClipper", 2),
        ("GetColorKey", 3),
        ("GetDC", 2),
        ("GetFlipStatus", 2),
        ("GetOverlayPosition", 3),
        ("GetPalette", 2),
        ("GetPixelFormat", 2),
        ("GetSurfaceDesc", 2),
        ("Initialize", 3),
        ("IsLost", 1),
        ("Lock", 5),
        ("ReleaseDC", 2),
        ("Restore", 1),
        ("SetClipper", 2),
        ("SetColorKey", 3),
        ("SetOverlayPosition", 3),
        ("SetPalette", 2),
        ("Unlock", 2),
        ("UpdateOverlay", 6),
        ("UpdateOverlayDisplay", 2),
        ("UpdateOverlayZOrder", 3),
    ];
    const PALETTE_SLOTS: [Slot; 7] = [
        ("QueryInterface", 3),
        ("AddRef", 1),
        ("Release", 1),
        ("GetCaps", 2),
        ("GetEntries", 5),
        ("Initialize", 4),
        ("SetEntries", 5),
    ];
    /// The slots with a front, by interface; every other answers `absent`.
    const OBJECT_SERVED: [&str; 7] = [
        "Release",
        "CreatePalette",
        "CreateSurface",
        "GetCaps",
        "RestoreDisplayMode",
        "SetCooperativeLevel",
        "SetDisplayMode",
    ];
    const SURFACE_SERVED: [&str; 11] = [
        "Release",
        "Blt",
        "BltFast",
        "Flip",
        "GetAttachedSurface",
        "GetSurfaceDesc",
        "IsLost",
        "Lock",
        "Restore",
        "SetPalette",
        "Unlock",
    ];
    const PALETTE_SERVED: [&str; 2] = ["Release", "SetEntries"];

    const WIDTH: u32 = 64;
    const HEIGHT: u32 = 32;
    const LEVEL: u32 = 0x51;
    const PALETTE_FLAGS: u32 = 0x44;
    const BACKS: u32 = 2;
    const WAIT: u32 = 1;
    /// `sizeof(DDCAPS)` in the SDK this table's order comes from.
    const CAPS_BYTES: u32 = 0x13c;
    const CAPS_WORDS: usize = 79;
    const FX_BYTES: u32 = 100;
    const WORD: u32 = 4;
    /// The most arguments a slot takes.
    const MOST: usize = 6;
    /// Zero words below a call's own, so a front that takes too many reads nothing live.
    const PAD: usize = 8;

    static ROW: Row = Row {
        file: "stack",
        levels: &[LEVEL, 8],
        modes: &[(WIDTH, HEIGHT, 8)],
        refused: DDERR_INVALIDMODE,
        chain: BACKS,
        palette_flags: PALETTE_FLAGS,
    };

    /// Each call's lines, by a logger that holds them until the row is printed.
    static LINES: Mutex<Vec<String>> = Mutex::new(Vec::new());

    struct Catch;

    impl log::Log for Catch {
        fn enabled(&self, _: &log::Metadata<'_>) -> bool {
            true
        }

        fn log(&self, record: &log::Record<'_>) {
            let line = oxide_logline::body(record);
            let own = line.starts_with("surfaces.absent")
                || (line.starts_with("imp.call") && line.contains("by=draw-surfaces"));
            if own && let Ok(mut lines) = LINES.lock() {
                let shown = if line.starts_with("imp.call") { 2 } else { 3 };
                lines.push(line.split(' ').take(shown).collect::<Vec<_>>().join(" "));
            }
        }

        fn flush(&self) {}
    }

    static CATCH: Catch = Catch;

    /// What a call left: `esp` on each side of it, and its answer.
    struct Read {
        before: u32,
        after: u32,
        answer: u32,
    }

    /// Calls `target` with `words`, `this` first, pushed last first over the zero words.
    /// `esp` comes back from a register the call must keep, so a wrong pop is read and
    /// this program goes on.
    ///
    /// # Safety
    /// `target` is a function that takes its arguments from the stack and keeps `ebx`
    /// and `edi`, and `words` hold at most `MOST`.
    unsafe fn call_at(target: u32, words: &[u32]) -> Read {
        let mut stack = [0_u32; MOST + PAD];
        stack[..words.len()].copy_from_slice(words);
        let count = u32::try_from(words.len() + PAD).unwrap_or(0);
        let (before, after, answer): (u32, u32, u32);
        // SAFETY: by this function's contract; `esp` is put back before the block ends.
        unsafe {
            core::arch::asm!(
                "mov edi, esp",
                "2:",
                "push dword ptr [edx + ecx*4 - 4]",
                "dec ecx",
                "jnz 2b",
                "mov ebx, esp",
                "call eax",
                "mov ecx, esp",
                "mov esp, edi",
                inlateout("eax") target => answer,
                inlateout("ecx") count => after,
                inlateout("edx") stack.as_ptr() => _,
                out("ebx") before,
                out("edi") _,
                clobber_abi("system"),
            );
        }
        Read {
            before,
            after,
            answer,
        }
    }

    unsafe extern "system" fn plant_short(_this: u32, _over: u32) -> u32 {
        DD_OK
    }

    unsafe extern "system" fn plant_few(_this: u32) -> u32 {
        log::debug!(c = SURFACE, i:% = "44"; "surfaces.absent");
        DDERR_UNSUPPORTED
    }

    unsafe extern "system" fn plant_no_line(_this: u32, _word: u32) -> u32 {
        DDERR_UNSUPPORTED
    }

    fn function_word(function: *const ()) -> u32 {
        u32::try_from(function.addr()).unwrap_or(0)
    }

    /// The address of a value a call only reads, as a word on its stack.
    fn held<T: ?Sized>(value: &T) -> u32 {
        u32::try_from(std::ptr::from_ref(value).cast::<()>().expose_provenance()).unwrap_or(0)
    }

    /// The address of a value a call writes, as a word on its stack.
    fn out<T>(value: &mut T) -> u32 {
        u32::try_from(std::ptr::from_mut(value).expose_provenance()).unwrap_or(0)
    }

    /// Points `this` at a copy of its table with `edit` done to the copy.
    fn plant_table(this: u32, count: usize, edit: impl Fn(&mut [u32])) {
        let word = this as usize as *mut u32;
        // SAFETY: `this` is a live interface, whose first word is its table of `count`.
        let table = unsafe { std::slice::from_raw_parts(*word as usize as *const u32, count) };
        let copy = Box::leak(table.to_vec().into_boxed_slice());
        edit(copy);
        // SAFETY: the interface's own first word; this program holds no other reader.
        unsafe { *word = held(&*copy) };
    }

    /// The plant's change to a surface's table, none for a clean run.
    fn plant_edit(plant: &str, table: &mut [u32]) {
        let at = |offset: usize| offset / 4;
        match plant {
            "moved" => table.swap(at(0x60), at(0x64)),
            "moved-same" => table.swap(at(0x2c), at(0x30)),
            "wrong-line" => table.swap(at(0x3c), at(0x44)),
            "short" => {
                let stub = plant_short as unsafe extern "system" fn(u32, u32) -> u32;
                table[at(0x2c)] = function_word(stub as *const ());
            }
            "few" => {
                let stub = plant_few as unsafe extern "system" fn(u32) -> u32;
                table[at(0x44)] = function_word(stub as *const ());
            }
            "no-line" => {
                let stub = plant_no_line as unsafe extern "system" fn(u32, u32) -> u32;
                table[at(0x44)] = function_word(stub as *const ());
            }
            _ => {}
        }
    }

    fn slots_of(interface: &str) -> &'static [Slot] {
        match interface {
            OBJECT => &OBJECT_SLOTS,
            SURFACE => &SURFACE_SLOTS,
            _ => &PALETTE_SLOTS,
        }
    }

    /// Calls `interface`'s slot `name` on `this` by its offset and prints the row.
    fn call(interface: &str, this: u32, name: &str, rest: &[u32]) -> u32 {
        let slots = slots_of(interface);
        let index = slots
            .iter()
            .position(|slot| slot.0 == name)
            .expect("a name on this program's own list");
        at_slot(interface, this, index, rest)
    }

    fn at_slot(interface: &str, this: u32, index: usize, rest: &[u32]) -> u32 {
        let (name, count) = slots_of(interface)[index];
        assert_eq!(rest.len() + 1, count, "{name}: this program's own list");
        let offset = u32::try_from(index).unwrap_or(0) * WORD;
        let mut words = vec![this];
        words.extend_from_slice(rest);
        // SAFETY: `this` is a live interface and its first word a table holding this slot.
        let target = unsafe {
            let table = *(this as usize as *const u32);
            *((table + offset) as usize as *const u32)
        };
        // SAFETY: a slot's function, stdcall, and `words` as `ddraw.h` counts them.
        let read = unsafe { call_at(target, &words) };
        println!(
            "slot\t{interface}\t{offset:x}\t{name}\t{count}\t{:08x}\t{:08x}\t{:08x}\t{}",
            read.before,
            read.after,
            read.answer,
            take_lines()
        );
        read.answer
    }

    /// The lines caught since the last row, joined, so each row holds only its own call's.
    fn take_lines() -> String {
        LINES
            .lock()
            .map_or_else(|_| Vec::new(), |mut lines| core::mem::take(&mut *lines))
            .join("|")
    }

    /// Every slot of `interface` with no front, on `this`.
    fn absent_pass(interface: &str, this: u32) {
        let served: &[&str] = match interface {
            OBJECT => &OBJECT_SERVED,
            SURFACE => &SURFACE_SERVED,
            _ => &PALETTE_SERVED,
        };
        let zeros = [0_u32; MOST];
        for (index, (name, count)) in slots_of(interface).iter().enumerate() {
            if !served.contains(name) {
                let _ = at_slot(interface, this, index, &zeros[..count - 1]);
            }
        }
    }

    /// The answered surface and palette calls, in a frame's order, made after any plant is in:
    /// a planted slot is reached here through the changed table.
    fn frame_calls(
        primary: u32,
        back: u32,
        palette: u32,
        source: u32,
        entries: &[u32; PALETTE_ENTRIES],
    ) {
        let _ = call(SURFACE, primary, "SetPalette", &[palette]);
        let _ = call(PALETTE, palette, "SetEntries", &[0, 0, 256, held(entries)]);
        let mut lent: Desc = [0; DESC_WORDS];
        lent[DESC_SIZE] = DESC_BYTES;
        let _ = call(SURFACE, back, "GetSurfaceDesc", &[out(&mut lent)]);
        let _ = call(SURFACE, back, "Lock", &[0, out(&mut lent), WAIT, 0]);
        let _ = call(SURFACE, back, "Unlock", &[lent[DESC_SURFACE]]);
        let mut fx = [0_u32; FX_WORDS];
        fx[0] = FX_BYTES;
        fx[FX_COLOUR] = 1;
        let fill = [0_i32, 0, 8, 8];
        let flags = DDBLT_COLORFILL | DDBLT_WAIT;
        let _ = call(SURFACE, back, "Blt", &[held(&fill), 0, 0, flags, held(&fx)]);
        let from = [0_i32, 0, 8, 8];
        let _ = call(
            SURFACE,
            back,
            "BltFast",
            &[8, 8, source, held(&from), DDBLTFAST_WAIT],
        );
        let _ = call(SURFACE, primary, "Flip", &[0, WAIT]);
        let _ = call(SURFACE, primary, "IsLost", &[]);
        let _ = call(SURFACE, primary, "Restore", &[]);
    }

    pub fn run() -> ExitCode {
        let plant = std::env::args().nth(2).unwrap_or_default();
        log::set_logger(&CATCH).unwrap_or(());
        log::set_max_level(log::LevelFilter::Debug);
        let window = make_window(0, 0, WIDTH, HEIGHT);
        let back_end = Window::new();
        back_end.aim(window);
        let easel: &'static Easel<Window> =
            Box::leak(Box::new(Easel::new(back_end, WIDTH, HEIGHT)));
        let surfaces = Surfaces::new(&ROW, easel, &Real);
        let com: &'static Com<Window, Real> = Box::leak(Box::new(Com::new(surfaces)));

        let mut object: *mut Interface<Window, Real> = core::ptr::null_mut();
        // SAFETY: a local word, alive across the call.
        let created = unsafe { com.direct_draw_create(0, &raw mut object, 0) };
        println!("created\t{created:08x}\t{}", take_lines());
        let object = u32::try_from(object.expose_provenance()).unwrap_or(0);

        let _ = call(OBJECT, object, "SetCooperativeLevel", &[window, LEVEL]);
        let _ = call(OBJECT, object, "SetDisplayMode", &[WIDTH, HEIGHT, 8]);
        let mut caps = [0_u32; CAPS_WORDS];
        caps[0] = CAPS_BYTES;
        let mut other = caps;
        let _ = call(
            OBJECT,
            object,
            "GetCaps",
            &[out(&mut caps), out(&mut other)],
        );

        let entries = [0_u32; PALETTE_ENTRIES];
        let mut palette = 0_u32;
        let _ = call(
            OBJECT,
            object,
            "CreatePalette",
            &[PALETTE_FLAGS, held(&entries), out(&mut palette), 0],
        );

        let mut desc: Desc = [0; DESC_WORDS];
        desc[DESC_SIZE] = DESC_BYTES;
        desc[DESC_FLAGS] = DDSD_CAPS | DDSD_BACKBUFFERCOUNT;
        desc[DESC_BACK_COUNT] = BACKS;
        desc[DESC_CAPS] = DDSCAPS_PRIMARYSURFACE | DDSCAPS_FLIP | DDSCAPS_COMPLEX;
        let mut primary = 0_u32;
        let _ = call(
            OBJECT,
            object,
            "CreateSurface",
            &[held(&desc), out(&mut primary), 0],
        );

        let mut plain: Desc = [0; DESC_WORDS];
        plain[DESC_SIZE] = DESC_BYTES;
        plain[DESC_FLAGS] = DDSD_CAPS | DDSD_HEIGHT | DDSD_WIDTH;
        plain[DESC_WIDTH] = WIDTH;
        plain[DESC_HEIGHT] = HEIGHT;
        plain[DESC_CAPS] = DDSCAPS_SYSTEMMEMORY;
        let mut source = 0_u32;
        let _ = call(
            OBJECT,
            object,
            "CreateSurface",
            &[held(&plain), out(&mut source), 0],
        );

        let wanted = [DDSCAPS_BACKBUFFER];
        let mut back = 0_u32;
        let _ = call(
            SURFACE,
            primary,
            "GetAttachedSurface",
            &[held(&wanted), out(&mut back)],
        );

        for this in [primary, back] {
            plant_table(this, SURFACE_SLOTS.len(), |table| plant_edit(&plant, table));
        }

        frame_calls(primary, back, palette, source, &entries);

        absent_pass(OBJECT, object);
        absent_pass(SURFACE, primary);
        absent_pass(PALETTE, palette);

        let _ = call(PALETTE, palette, "Release", &[]);
        let _ = call(SURFACE, source, "Release", &[]);
        let _ = call(SURFACE, primary, "Release", &[]);
        let _ = call(OBJECT, object, "RestoreDisplayMode", &[]);
        let _ = call(OBJECT, object, "Release", &[]);
        println!("done");
        ExitCode::SUCCESS
    }
}
