// SPDX-License-Identifier: MIT
//! A screen the original drew, sent back through the
//! fronts. It takes a dump's `.bits` and `.table` from a `DRAW_DUMP` run, drives
//! `CreateDIBSection`, a write through the bits pointer, `SetDIBColorTable` and
//! a whole-screen `BitBlt`, and writes what the screen then shows, the pixels
//! and the palette's words, for `sha256sum` against the dump's own:
//!
//! `cargo run -p draw --features page --example front_dump -- IN.bits IN.table OUT.bits OUT.table`
//!
//! It exits 0 once it wrote both, 1 when a front refused or a file would not
//! serve.
//!
//! **The difference from the run:** no logged `BitBlt` is whole-screen, so this
//! replays the buffer as the call found it, not the call's own rectangle.
use std::process::ExitCode;
use std::{env, fs};

use draw::easel::{Easel, Shown};
use draw::front::{Front, Header};
use draw::page::Page;
use draw::table::{Calls, Route, Row};
use window::Rect;
use window::desk::{Desk, Window};
use window::page::Page as Host;

/// The dump's picture: the one size the original's buffer took.
const WIDTH: u32 = 0x280;
const HEIGHT: u32 = 0x1e0;
/// The same two sides as a desk takes them, signed.
const SIDES: (i32, i32) = (0x280, 0x1e0);
/// The header a run logged: top-down, 8 bits.
const HEADER: Header = Header([0x28, WIDTH, 0xffff_fe20, 0x8_0001, 0, 0, 0, 0, 0, 0]);
/// One window's handle, as a desk would give it.
const WINDOW: u32 = 1;
/// `SRCCOPY`, the one raster operation the fronts serve.
const SRCCOPY: u32 = 0x00cc_0020;
/// A full table's entries.
const ENTRIES: u32 = 256;

static CALLS: Calls = Calls {
    create_palette: "GDI32!CreatePalette",
    select_palette: "GDI32!SelectPalette",
    set_palette_entries: "GDI32!SetPaletteEntries",
    realize_palette: "GDI32!RealizePalette",
    set_dib_color_table: "GDI32!SetDIBColorTable",
    get_system_palette_entries: "GDI32!GetSystemPaletteEntries",
    create_dib_section: "GDI32!CreateDIBSection",
    bit_blt: "GDI32!BitBlt",
    set_dib_bits_to_device: "GDI32!SetDIBitsToDevice",
    stretch_dib_bits: "GDI32!StretchDIBits",
    stretch_blt: "GDI32!StretchBlt",
    create_compatible_dc: "GDI32!CreateCompatibleDC",
    delete_dc: "GDI32!DeleteDC",
    delete_object: "GDI32!DeleteObject",
    select_object: "GDI32!SelectObject",
    get_device_caps: "GDI32!GetDeviceCaps",
    get_stock_object: "GDI32!GetStockObject",
    get_dc: "USER32!GetDC",
    release_dc: "USER32!ReleaseDC",
    update_window: "USER32!UpdateWindow",
    begin_paint: "USER32!BeginPaint",
    end_paint: "USER32!EndPaint",
    validate_rect: "USER32!ValidateRect",
};

static ROW: Row = Row {
    file: "front_dump",
    calls: &CALLS,
    caps: &[],
    route: Route::Blit,
};

/// `bits` and `quads` through the fronts onto the screen; the front that
/// refused, by name, when one did.
fn replay(bits: &[u8], quads: &[u32]) -> Result<Shown, &'static str> {
    let desk = Desk::new(Host::new(SIDES.0, SIDES.1));
    desk.state().windows.push(Window {
        handle: WINDOW,
        class: Vec::new(),
        procedure: 0,
        rect: Rect::sized(0, 0, SIDES.0, SIDES.1),
        style: 0,
        ex_style: 0,
        user: 0,
        longs: Vec::new(),
        title: Vec::new(),
        parent: 0,
        shown: true,
    });
    let easel = Easel::new(Page::new(), WIDTH, HEIGHT);
    let front = Front::new(&ROW, &easel, &desk);
    let window_dc = front.get_dc(WINDOW);
    let memory = front.create_compatible_dc(window_dc);
    let mut pointer = 0;
    let section = front.create_dib_section(window_dc, 0, 0, 0, HEADER, &[], &mut pointer);
    if section == 0 {
        return Err("CreateDIBSection");
    }
    front.select_object(memory, section);
    let written = easel.with_buffer(section, |buffer| {
        buffer
            .pixels
            .get_mut(..bits.len())
            .map(|pixels| pixels.copy_from_slice(bits))
    });
    if !matches!(written, Some(Some(()))) {
        return Err("the bits pointer's write");
    }
    if front.set_dib_color_table(memory, 0, ENTRIES, quads) != ENTRIES {
        return Err("SetDIBColorTable");
    }
    if front.bit_blt(window_dc, 0, 0, WIDTH, HEIGHT, memory, 0, 0, SRCCOPY) != 1 {
        return Err("BitBlt");
    }
    Ok(easel.shown())
}

/// The run behind `main`: its error is the one line a failed run prints.
fn run(paths: &[String]) -> Result<(), String> {
    let [bits_in, table_in, bits_out, table_out] = paths else {
        return Err("takes IN.bits IN.table OUT.bits OUT.table".to_owned());
    };
    let read = |path: &String| fs::read(path).map_err(|error| format!("{path}: {error}"));
    let bits = read(bits_in)?;
    let quads: Vec<u32> = read(table_in)?
        .chunks_exact(4)
        .map(|word| {
            word.iter()
                .rev()
                .fold(0, |all, byte| all << 8 | u32::from(*byte))
        })
        .collect();
    let shown = replay(&bits, &quads).map_err(|front| format!("{front} refused"))?;
    let words: Vec<u8> = shown
        .palette
        .0
        .iter()
        .flat_map(|word| word.to_le_bytes())
        .collect();
    let write = |path: &String, bytes: &[u8]| {
        fs::write(path, bytes).map_err(|error| format!("{path}: {error}"))
    };
    write(bits_out, &shown.pixels)?;
    write(table_out, &words)
}

fn main() -> ExitCode {
    let paths: Vec<String> = env::args().skip(1).collect();
    match run(&paths) {
        Ok(()) => ExitCode::SUCCESS,
        Err(line) => {
            eprintln!("front_dump: {line}");
            ExitCode::FAILURE
        }
    }
}
