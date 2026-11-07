// SPDX-License-Identifier: MIT
//! Wine's own picture, asked by a 32-bit program with no crate of ours, its calls declared
//! by hand. Built by `wine.sh link`; no Cargo target.
//!
//! It shows the worked picture in a window at 40,40, prints `ready` after the first time,
//! and stays up for 450 turns of 20 ms. The screen is read from outside, by `xwd -root`, so
//! this program says only the words a wrong picture must carry.
//!
//! Lines take the log's `imp.call` form, printed as this file links no logger. Exits 0 when
//! Wine took every picture, 1 on red.
use std::ffi::c_void;
use std::process::ExitCode;
use std::thread::sleep;
use std::time::Duration;

mod bare_window;
// Each program takes only part of the shared file.
#[allow(dead_code)]
mod worked_picture;

use bare_window::Handle;
use worked_picture::{HEIGHT, LEFT, PICTURE_RED, TOP, WIDTH, picture};

const TURNS: u32 = 450;
const TURN: Duration = Duration::from_millis(20);
const DIB_RGB_COLORS: u32 = 0;
const BI_RGB: u32 = 0;
/// A negative height says the rows run from the top, as the worked picture's do.
const TOP_DOWN_HEIGHT: i32 = -(HEIGHT as i32);

/// Windows' `BITMAPINFOHEADER`, whose 32-bit pixels sit in memory as blue, green, red, 0:
/// a little-endian 0RGB word.
#[repr(C)]
struct BitmapHeader {
    size: u32,
    width: i32,
    height: i32,
    planes: u16,
    bits: u16,
    compression: u32,
    image_size: u32,
    x_per_metre: i32,
    y_per_metre: i32,
    colours_used: u32,
    colours_important: u32,
}

#[link(name = "user32")]
unsafe extern "system" {
    fn GetDC(window: Handle) -> Handle;
}

#[link(name = "gdi32")]
unsafe extern "system" {
    fn SetDIBitsToDevice(
        device: Handle,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        source_x: i32,
        source_y: i32,
        start_line: u32,
        lines: u32,
        pixels: *const c_void,
        header: *const BitmapHeader,
        usage: u32,
    ) -> i32;
}

fn red(why: &str) -> ExitCode {
    println!("CV1 refused {why}");
    println!("CV1 red {PICTURE_RED}");
    ExitCode::FAILURE
}

fn main() -> ExitCode {
    let size = (WIDTH as i32, HEIGHT as i32);
    let window = match bare_window::open(c"picture_probe", (LEFT, TOP), size) {
        Ok(window) => window,
        Err(why) => return red(&why),
    };
    let words = picture();
    let header = BitmapHeader {
        size: size_of::<BitmapHeader>() as u32,
        width: WIDTH as i32,
        height: TOP_DOWN_HEIGHT,
        planes: 1,
        bits: 32,
        compression: BI_RGB,
        image_size: 0,
        x_per_metre: 0,
        y_per_metre: 0,
        colours_used: 0,
        colours_important: 0,
    };
    // SAFETY: the window just made.
    let device = unsafe { GetDC(window) };
    for turn in 0..TURNS {
        // SAFETY: `words` holds WIDTH by HEIGHT words, as the header says, and outlives the call.
        let lines = unsafe {
            SetDIBitsToDevice(
                device,
                0,
                0,
                WIDTH,
                HEIGHT,
                0,
                0,
                0,
                HEIGHT,
                words.as_ptr().cast(),
                &raw const header,
                DIB_RGB_COLORS,
            )
        };
        if turn == 0 {
            println!(
                "imp.call c=GDI32!SetDIBitsToDevice a={WIDTH},{HEIGHT} v={lines} by=layer-check"
            );
        }
        if lines != HEIGHT as i32 {
            return red(&format!("SetDIBitsToDevice took {lines} lines"));
        }
        bare_window::pump();
        if turn == 0 {
            println!("CV1 if-wrong {PICTURE_RED}");
            println!("ready");
        }
        sleep(TURN);
    }
    ExitCode::SUCCESS
}
