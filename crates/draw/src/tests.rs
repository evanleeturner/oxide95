// SPDX-License-Identifier: MIT
//! The fronts on the page's back, on the host. A test replaying a run says so,
//! `from` and `tid` left out, handles apart. A name no run reached carries the
//! reading of its callers' code, its test's doc saying so. The rows are a
//! test's, their names the `DLL!NAME` spellings the layer logs.
use window::Rect;
use window::desk::{Desk, Window};
use window::page::Page as Host;

use crate::easel::{self, Area, Back, Easel, Palette, Shown};
use crate::front::{Front, Header};
use crate::page::Page;
use crate::table::{Calls, Route, Row};
use crate::test_log::debug_lines;

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
    file: "test",
    calls: &CALLS,
    caps: &[(0x18, -1), (0xc, 32)],
    route: Route::Blit,
};

/// The header a run logged: 640 by 480, top-down, 8 bits.
const HEADER: Header = Header([0x28, 0x280, 0xffff_fe20, 0x8_0001, 0, 0, 0, 0, 0, 0]);
const WINDOW: u32 = 0x2005e;

fn rig() -> (Easel<Page>, Desk<Host>) {
    let desk = Desk::new(Host::new(0x280, 0x1e0));
    desk.state().windows.push(Window {
        handle: WINDOW,
        class: Vec::new(),
        procedure: 0,
        rect: Rect::sized(0, 0, 0x280, 0x1e0),
        style: 0,
        ex_style: 0,
        user: 0,
        longs: Vec::new(),
        title: Vec::new(),
        parent: 0,
        shown: true,
    });
    (Easel::new(Page::new(), 0x280, 0x1e0), desk)
}

/// 23 fronts, the crate's doc naming them and another game's 7.
#[test]
fn every_name_placed_in_the_crate_doc() {
    let doc = include_str!("lib.rs");
    let names = [
        "CreatePalette",
        "SelectPalette",
        "SetPaletteEntries",
        "RealizePalette",
        "SetDIBColorTable",
        "GetSystemPaletteEntries",
        "CreateDIBSection",
        "BitBlt",
        "SetDIBitsToDevice",
        "StretchDIBits",
        "StretchBlt",
        "CreateCompatibleDC",
        "DeleteDC",
        "DeleteObject",
        "SelectObject",
        "GetDeviceCaps",
        "GetStockObject",
        "GetDC",
        "ReleaseDC",
        "UpdateWindow",
        "BeginPaint",
        "EndPaint",
        "ValidateRect",
        "GetPixel",
        "Rectangle",
        "SetPixel",
        "CreateDCA",
        "GetObjectA",
        "SetMapMode",
        "LoadImageA",
    ];
    assert_eq!(names.len(), 30);
    for name in names {
        assert!(doc.contains(&format!("`{name}`")), "{name}");
    }
}

/// A run's palette lines: `a=1000300`, then select, 0x100 entries, realize 0.
#[test]
fn palette_calls_replay_d1() {
    let (easel, desk) = rig();
    let front = Front::new(&ROW, &easel, &desk);
    let dc = front.get_dc(WINDOW);
    let entries = [0x0400_0000_u32; 256];
    let lines = debug_lines(|| {
        let palette = front.create_palette(0x100_0300, &entries);
        assert_ne!(front.select_palette(dc, palette, 0), 0);
        assert_eq!(
            front.set_palette_entries(palette, 0, 0x100, &entries),
            0x100
        );
        assert_eq!(front.realize_palette(dc), 0);
    });
    let head = lines[0]
        .split(" v=")
        .next()
        .expect("invariant: a line has a v");
    assert_eq!(head, "imp.call c=GDI32!CreatePalette a=1000300");
    assert!(lines[1].starts_with("draw.table c=GDI32!CreatePalette i=0 a=4000000,"));
    assert_eq!(
        lines.iter().filter(|l| l.starts_with("draw.table")).count(),
        32
    );
    assert!(
        lines
            .iter()
            .any(|l| l.ends_with("a=") || l.contains(",0,100 v=100"))
    );
    assert!(
        lines
            .last()
            .expect("invariant: lines")
            .ends_with(" v=0 by=draw")
    );
}

/// A run: `GetDeviceCaps` `NUMCOLORS` answers `ffffffff`; an index not in the
/// row 0. The 32 is unmeasured: `BITSPIXEL` reached no line.
#[test]
fn device_caps_answer_the_row() {
    let (easel, desk) = rig();
    let front = Front::new(&ROW, &easel, &desk);
    assert_eq!(front.get_device_caps(7, 0x18), 0xffff_ffff);
    assert_eq!(front.get_device_caps(7, 0xc), 32);
    assert_eq!(front.get_device_caps(7, 0x99), 0);
}

/// A run's line, handles apart: 32 of 32 answered `1e0`, a window's context.
#[test]
fn set_dib_bits_to_device_replays_d1() {
    let (easel, desk) = rig();
    let front = Front::new(&ROW, &easel, &desk);
    let dc = front.get_dc(WINDOW);
    let mut bits = vec![0_u8; 0x280 * 0x1e0];
    bits[0x280 * 2 + 3] = 9;
    let lines = debug_lines(|| {
        let answer =
            front.set_dib_bits_to_device(dc, 0, 0, 0x280, 0x1e0, 0, 0, 0, 0x1e0, 1, HEADER, &bits);
        assert_eq!(answer, 0x1e0);
    });
    let want = format!(
        "imp.call c=GDI32!SetDIBitsToDevice a={dc:x},0,0,280,1e0,0,0,0,1e0,1,28,280,fffffe20,80001,0,0,0,0,0,0 v=1e0 by=draw"
    );
    assert!(lines.contains(&want), "{lines:?}");
    assert_eq!(easel.shown().pixels[0x280 * 2 + 3], 9);
}

/// A run's `BitBlt`, handles apart, 1 of 266 of 266: the rectangle lands, its
/// colours the section's own table, written by a program's bits pointer.
#[test]
fn section_blit_replays_d1_and_shows_its_table() {
    let (easel, desk) = rig();
    let front = Front::new(&ROW, &easel, &desk);
    let window_dc = front.get_dc(WINDOW);
    let memory = front.create_compatible_dc(window_dc);
    let (mut bits, table) = (0, [0x00ab_cdef_u32; 256]);
    let section = front.create_dib_section(window_dc, 0, 0, 0, HEADER, &table, &mut bits);
    assert_ne!(section, 0);
    assert_ne!(bits, 0);
    let _ = front.select_object(memory, section);
    easel.with_buffer(section, |buffer| buffer.pixels[0x280 * 0x1ba + 0x1d] = 7);
    let lines = debug_lines(|| {
        let answer = front.bit_blt(
            window_dc, 0x1d, 0x1ba, 0xdb, 0xf, memory, 0x1d, 0x1ba, 0xcc_0020,
        );
        assert_eq!(answer, 1);
    });
    let want = format!(
        "imp.call c=GDI32!BitBlt a={window_dc:x},1d,1ba,db,f,{memory:x},1d,1ba,cc0020 v=1 by=draw"
    );
    assert!(lines.contains(&want), "{lines:?}");
    let shown = easel.shown();
    assert_eq!(shown.pixels[0x280 * 0x1ba + 0x1d], 7);
    assert_eq!(shown.palette.zero_rgb(7), 0x00ab_cdef);
}

/// Unreached by a run, the reading of the callers' code: other rops answer 0, the
/// stretches answer as their callers compare, the depth gate answers 0.
#[test]
fn unreached_answers_read_from_the_callers() {
    let (easel, desk) = rig();
    let front = Front::new(&ROW, &easel, &desk);
    let dc = front.get_dc(WINDOW);
    let memory = front.create_compatible_dc(dc);
    let (mut bits, kept) = (5, 5);
    let deep = Header([0x28, 8, 8, 0x18_0001, 0, 0, 0, 0, 0, 0]);
    assert_eq!(
        front.create_dib_section(dc, 0, 0, 0, deep, &[], &mut bits),
        0
    );
    assert_eq!(bits, kept);
    let section = front.create_dib_section(dc, 1, 0, 0, HEADER, &[], &mut bits);
    let first = front.select_object(memory, section);
    assert_ne!(first, 0);
    assert_eq!(front.bit_blt(dc, 0, 0, 1, 1, memory, 0, 0, 0x42), 0);
    let whole = (0, 0, 2, 2);
    assert_eq!(front.stretch_blt(dc, whole, memory, whole, 0xcc_0020), 1);
    assert_eq!(front.stretch_blt(dc, whole, memory, whole, 0x42), 0);
    let bytes = [1_u8; 0x280 * 2];
    assert_eq!(
        front.stretch_dib_bits(dc, whole, whole, 1, 0xcc_0020, HEADER, &bytes),
        0x1e0
    );
    assert_eq!(
        front.stretch_dib_bits(dc, whole, whole, 1, 0x42, HEADER, &bytes),
        0
    );
    assert_eq!(front.get_stock_object(4), Easel::<Page>::stock_object(4));
    assert_eq!(front.get_stock_object(99), 0);
    // A selected object is refused, as Windows refuses; let go, it deletes.
    assert_eq!(front.delete_object(section), 0);
    assert_eq!(front.select_object(memory, first), section);
    assert_eq!(front.delete_object(section), 1);
    assert_eq!(front.delete_dc(memory), 1);
    assert_eq!(front.delete_dc(memory), 0);
    assert_eq!(front.release_dc(WINDOW, dc), 1);
}

/// Paint names: the record's 6 words, the window's whole rectangle; no such
/// window answers 0 and leaves the record. A real run reached none of the 4, so
/// this holds a reading of the code, not a measurement.
#[test]
fn paint_calls_answer_the_window() {
    let (easel, desk) = rig();
    let front = Front::new(&ROW, &easel, &desk);
    let mut paint = [9; 6];
    assert_eq!(front.begin_paint(1, &mut paint), 0);
    assert_eq!(paint, [9; 6]);
    let lines = debug_lines(|| assert_ne!(front.begin_paint(WINDOW, &mut paint), 0));
    let want = format!(
        "imp.call c=USER32!BeginPaint a=2005e o={:x},0,0,0,280,1e0 v={:x} by=draw",
        paint[0], paint[0]
    );
    assert!(lines.contains(&want), "{lines:?}");
    assert_eq!(front.end_paint(WINDOW, &paint), 1);
    assert_eq!(front.end_paint(WINDOW, &paint), 0);
    assert_eq!(front.validate_rect(WINDOW, None), 1);
    assert_eq!(front.validate_rect(1, Some([0, 0, 1, 1])), 0);
    assert_eq!(front.update_window(1), 0);
    assert_eq!(front.update_window(WINDOW), 1);
}

/// Unreached by a run, and by any row yet, the reading of the callers' code: the
/// shell asks only when `BITSPIXEL` is 8, and a row answering 32 never gets here.
/// No value is harvested. The count is what fits: the buffer, the count asked
/// and the 256 entries each bound it, and one call writes one `imp.call` line.
#[test]
fn system_palette_entries_write_what_fits() {
    let (easel, desk) = rig();
    let front = Front::new(&ROW, &easel, &desk);
    let dc = front.get_dc(WINDOW);
    let mut into = [9; 4];
    assert_eq!(front.get_system_palette_entries(dc, 0, 2, &mut into), 2);
    assert_eq!(into[2..], [9, 9]);
    assert_eq!(front.get_system_palette_entries(dc, 0, 256, &mut into), 4);
    assert_eq!(front.get_system_palette_entries(dc, 254, 4, &mut into), 2);
    assert_eq!(front.get_system_palette_entries(dc, 256, 4, &mut into), 0);
    assert_eq!(front.get_system_palette_entries(dc, 0, 4, &mut []), 0);
    let lines = debug_lines(|| {
        assert_eq!(front.get_system_palette_entries(dc, 0, 2, &mut into), 2);
    });
    let want = format!("imp.call c=GDI32!GetSystemPaletteEntries a={dc:x},0,2 v=2 by=draw");
    assert!(lines.contains(&want), "{lines:?}");
}

/// The named faults: a game's wild numbers neither panic nor allocate
/// their size, a mirror lands nothing, a window's context holds no table.
#[test]
fn wild_numbers_answer_without_a_panic() {
    let (easel, desk) = rig();
    let front = Front::new(&ROW, &easel, &desk);
    let dc = front.get_dc(WINDOW);
    let memory = front.create_compatible_dc(dc);
    let mut bits = 0;
    let wild = Header([0x28, 0xffff_ffff, 0x7fff_ffff, 0x8_0001, 0, 0, 0, 0, 0, 0]);
    assert_eq!(
        front.create_dib_section(dc, 0, 0, 0, wild, &[], &mut bits),
        0
    );
    let section = front.create_dib_section(dc, 0, 0, 0, HEADER, &[], &mut bits);
    let _ = front.select_object(memory, section);
    let big = 0x7fff_ffff;
    assert_eq!(
        front.bit_blt(dc, big, big, big, big, memory, big, big, 0xcc_0020),
        1
    );
    assert_eq!(
        front.bit_blt(dc, 0, 0, 0xffff_ffff, 0xffff_ffff, memory, 0, 0, 0xcc_0020),
        1
    );
    assert_eq!(front.set_dib_color_table(dc, 0, 1, &[1]), 0);
    assert_eq!(front.set_dib_color_table(memory, 0, 1, &[1]), 1);
    let area = Area::new(0, 0, 0, 0);
    assert_eq!(easel.blit(dc, area, memory, area), Some(()));
}

/// All 256 palette entries to `0RGB` and to RGBA, from a
/// `PALETTEENTRY` (red low) and a `RGBQUAD` (blue low).
#[test]
fn all_256_entries_convert() {
    let (mut entries, mut quads) = (Palette::black(), Palette::black());
    let words: Vec<u32> = (0..256_u32)
        .map(|i| i | (0x11 << 8) | ((255 - i) << 16))
        .collect();
    assert_eq!(entries.set_entries(0, &words), 256);
    assert_eq!(quads.set_quads(0, &words), 256);
    for i in 0..=255_u8 {
        let (low, high) = (u32::from(i), 255 - u32::from(i));
        assert_eq!(entries.zero_rgb(i), (low << 16) | 0x1100 | high);
        assert_eq!(entries.rgba(i), [i, 0x11, 255 - i, 0xff]);
        assert_eq!(entries.entry(i), low | 0x1100 | (high << 16));
        assert_eq!(quads.zero_rgb(i), low | 0x1100 | (high << 16));
        assert_eq!(quads.rgba(i), [255 - i, 0x11, i, 0xff]);
    }
}

fn one_pixel(index: u8, palette: Palette) -> Shown {
    Shown {
        width: 1,
        height: 1,
        pitch: 4,
        pixels: vec![index, 0, 0, 0],
        palette,
    }
}

/// The page's read once a frame, the last present wins, a dropped
/// present a DEBUG line; no new one reads false and leaves the bytes.
#[test]
fn page_read_once_last_wins_drop_logged() {
    let page = Page::new();
    let mut palette = Palette::black();
    palette.set_quads(1, &[0x0001_0203, 0x0004_0506]);
    let mut into = [0_u8; 4];
    assert!(!page.read(&mut into));
    assert_eq!(page.size(), (0, 0));
    let lines = debug_lines(|| {
        page.present(&one_pixel(1, palette.clone()));
        page.present(&one_pixel(2, palette.clone()));
    });
    assert_eq!(lines, ["draw.drop n=1"]);
    assert_eq!(page.size(), (1, 1));
    assert!(page.read(&mut into));
    assert_eq!(into, [4, 5, 6, 0xff]);
    into = [9; 4];
    assert!(!page.read(&mut into));
    assert_eq!(into, [9; 4]);
}

/// A 2 by 2 picture, its rows 3 bytes apart: a row's last byte is padding, so
/// a read that keeps it shears the row after.
fn two_by_two() -> Shown {
    let mut palette = Palette::black();
    palette.0[1] = 0x11_0000;
    palette.0[2] = 0x22_0000;
    palette.0[3] = 0x33_0000;
    palette.0[4] = 0x44_0000;
    Shown {
        width: 2,
        height: 2,
        pitch: 3,
        pixels: vec![1, 2, 9, 3, 4, 9],
        palette,
    }
}

#[test]
fn a_wider_taller_window_shows_the_picture_at_its_corner_the_rest_black() {
    let mut into = [0xdead_beef_u32; 12];
    two_by_two().to_zero_rgb(&mut into, 4);
    assert_eq!(
        into,
        [
            0x11_0000, 0x22_0000, 0, 0, //
            0x33_0000, 0x44_0000, 0, 0, //
            0, 0, 0, 0,
        ]
    );
}

#[test]
fn a_narrower_shorter_window_cuts_the_picture_never_shears_it() {
    let mut into = [0xdead_beef_u32; 1];
    two_by_two().to_zero_rgb(&mut into, 1);
    assert_eq!(into, [0x11_0000]);

    let mut narrow = [0xdead_beef_u32; 2];
    two_by_two().to_zero_rgb(&mut narrow, 1);
    assert_eq!(narrow, [0x11_0000, 0x33_0000]);
}

#[test]
fn a_new_screen_is_all_index_0_and_a_wild_size_leaves_the_old() {
    let (easel, _) = rig();
    easel.with_screen(|screen| screen.pixels.fill(7));
    assert!(easel.resize_screen(320, 200));
    let shown = easel.shown();
    assert_eq!((shown.width, shown.height, shown.pitch), (320, 200, 320));
    assert!(shown.pixels.iter().all(|index| *index == 0));
    assert!(!easel.resize_screen(0x8000_0000, 0x8000_0000));
    assert_eq!(easel.shown().width, 320);
}

/// No copy: the pixels' address moves with them, what a flip's next `Lock` reads.
#[test]
fn a_swap_moves_the_pixels_and_their_address() {
    let (easel, _) = rig();
    assert!(easel.resize_screen(4, 2));
    let buffer = easel::Buffer::new(4, 2, false);
    let bitmap = easel.create_bitmap(buffer);
    easel.with_buffer(bitmap, |buffer| buffer.pixels.fill(9));
    let screen = easel.with_screen(|screen| screen.address());
    let kept = easel.with_buffer(bitmap, |buffer| buffer.address());
    assert!(easel.swap_screen(bitmap));
    assert_eq!(easel.shown().pixels[0], 9);
    assert_eq!(
        easel.with_screen(|screen| screen.address()),
        kept.expect("the bitmap")
    );
    assert_eq!(
        easel.with_buffer(bitmap, |buffer| buffer.address()),
        Some(screen)
    );
    let other = easel.create_bitmap(easel::Buffer::new(8, 2, false));
    assert!(!easel.swap_screen(other), "another shape moves nothing");
    assert!(!easel.swap_screen(0x7777), "no such bitmap");
    assert_eq!(easel.shown().pixels[0], 9);
}

#[test]
fn a_shown_palette_is_the_colours_and_what_a_blit_reads_through() {
    let (easel, _) = rig();
    let palette = easel.create_palette(&[0xff]);
    assert!(easel.show_palette(palette));
    assert_eq!(easel.shown().palette.zero_rgb(0), 0xff_0000);
    assert!(!easel.show_palette(0x7777));
    let dc = easel.window_context(0);
    let memory = easel.memory_context();
    let bitmap = easel.create_bitmap(easel::Buffer::new(4, 2, false));
    easel.select(memory, bitmap);
    let whole = Area::new(0, 0, 4, 2);
    assert_eq!(easel.blit(dc, whole, memory, whole), Some(()));
    assert_eq!(
        easel.shown().palette.zero_rgb(0),
        0xff_0000,
        "the realized colours"
    );
}
