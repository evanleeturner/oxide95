// SPDX-License-Identifier: MIT
//! The `DirectDraw` stand-in: the object, surfaces and palette a program reaches
//! through tables, written once over `draw`'s easel. `DirectDraw` is never
//! rebuilt: a front answers what a program reads back of a call, its return and
//! what it wrote, and draws what its pixels say. The primary surface is `draw`'s
//! screen, so a GDI call and a `DirectDraw` call see one picture.
//!
//! [`surfaces::Surfaces`] is the portable part, in a file's 32-bit numbers;
//! [`com::Com`] is the shape a program calls, the tables in `ddraw.h`'s order;
//! [`table::Row`] is one file's facts, kept in its game's rows crate; [`sdk`]
//! is `ddraw.h`'s numbers.
//!
//! The 21 fronts: `DirectDrawCreate`; the object's `SetCooperativeLevel`,
//! `SetDisplayMode`, `GetCaps`, `CreateSurface`, `CreatePalette`,
//! `RestoreDisplayMode`, `Release`; the palette's `SetEntries`, `Release`; the
//! surface's `GetAttachedSurface`, `GetSurfaceDesc`, `Lock`, `Unlock`, `IsLost`,
//! `Restore`, `Flip`, `Blt`, `BltFast`, `SetPalette`, `Release`.
//!
//! Every other slot of the 66 answers `DDERR_UNSUPPORTED` with one
//! `surfaces.absent` line, popping its own arguments, and is never null. Each
//! comes with its first measured caller. Another game's 3 are among them:
//! `FlipToGDISurface`, `GetDC` and `ReleaseDC`.
//!
//! Each call writes one `imp.call` line, `c` the method as `INTERFACE::NAME`; a
//! palette's entries follow on `draw.table` lines.
//!
//! The walls, named: one object a [`surfaces::Surfaces`]; every surface is 8 bits;
//! a mode no row lists is refused; `Lock` serves the whole surface alone; a write
//! through a primary's `Lock` shows at the next present; `IsLost` always answers
//! 0, nothing here being lost; a colour key and a blit effect are refused; an
//! interface is kept until its [`com::Com`] goes, so a stale pointer still lands
//! on a table; on a 64-bit host `lpSurface` is not a pointer, `draw`'s
//! own wall, so a host test writes by [`surfaces::Surfaces::with_surface`].
#![cfg_attr(not(test), no_std)]

#[cfg(any(test, feature = "desk"))]
extern crate alloc;

#[cfg(any(test, feature = "desk"))]
pub mod com;
pub mod sdk;
#[cfg(any(test, feature = "desk"))]
pub mod surfaces;
pub mod table;
#[cfg(test)]
mod test_log;
#[cfg(test)]
mod tests;
