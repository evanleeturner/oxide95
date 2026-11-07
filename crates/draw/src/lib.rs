// SPDX-License-Identifier: MIT
//! The drawing stand-in: the GDI32 names a program
//! asks of its pixels and its palette, and USER32's paint names, written once
//! over the `window` crate's desk. Windows' drawing stack is never rebuilt: a front
//! answers what the program reads back of a call, its return and what it
//! wrote, and draws what its pixels say.
//!
//! The portable part every target shares is [`easel::Easel`]: the buffers, the
//! palettes, the contexts and the objects they hold, the blits and the screen.
//! A [`easel::Back`] sees [`easel::Shown`] alone, once a present: [`page::Page`]
//! keeps it for the page's script, [`native::Native`] paints it on a host's
//! window, `user32::Window` on a real one under Wine, by GDI32 alone.
//!
//! The 23 fronts, in [`front::Front`]: `CreatePalette`, `SelectPalette`,
//! `SetPaletteEntries`, `RealizePalette`, `SetDIBColorTable`,
//! `GetSystemPaletteEntries`, `CreateDIBSection`, `BitBlt`,
//! `SetDIBitsToDevice`, `StretchDIBits`, `StretchBlt`, `CreateCompatibleDC`,
//! `DeleteDC`, `DeleteObject`, `SelectObject`, `GetDeviceCaps`,
//! `GetStockObject`, `GetDC`, `ReleaseDC`, `UpdateWindow`, `BeginPaint`,
//! `EndPaint` and `ValidateRect`. Each front's doc names its callers by file;
//! a game's rows crate names the files and each one's answers, [`table::Row`].
//!
//! Names a game imports that no row serves yet have no front, as 6 names
//! in `window`: each comes with its first measured caller. Another game's 7:
//! `GetPixel`, `Rectangle`, `SetPixel`, `CreateDCA`, `GetObjectA`,
//! `SetMapMode` and `LoadImageA`.
//!
//! Each call writes one `imp.call` line, as the layer's without the caller;
//! a table's entries follow on `draw.table` lines; a present the page never
//! read is a `draw.drop` line, a host's paint a `draw.present` line.
//!
//! The walls, named: every picture is 8 bits; a raster operation not `SRCCOPY`
//! draws nothing; a usage 1 table reads as the identity, the one measured; a
//! negative width or height, a mirror, lands nothing; `SetDIBitsToDevice` reads
//! `bits` from the picture's first row, `start` 0 being the one measured, and
//! `from_y` from the top whichever way the rows lie; `BeginPaint`'s `rcPaint`
//! is the whole window; a picture past 1 GiB is refused; on a 64-bit host
//! [`easel::Buffer::address`] is not a pointer, so a host test writes by
//! [`easel::Easel::with_buffer`].
#![cfg_attr(not(test), no_std)]

#[cfg(any(test, feature = "desk"))]
extern crate alloc;

#[cfg(any(test, feature = "desk"))]
pub mod easel;
#[cfg(any(test, feature = "desk"))]
pub mod front;
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub mod native;
#[cfg(any(test, feature = "page"))]
pub mod page;
pub mod table;
#[cfg(test)]
mod test_log;
#[cfg(test)]
mod tests;
#[cfg(all(feature = "user32", target_arch = "x86", target_os = "windows"))]
pub mod user32;
