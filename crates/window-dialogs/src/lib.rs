// SPDX-License-Identifier: MIT
//! The window stand-in's boxes and menus: the 10 USER32 names a program
//! reached, each a front in [`front::Front`] with one `imp.call` line, over
//! `window`'s desk and [`tables::Tables`].
//!
//! - **Reads:** a template's bytes come from the module on the player's disc by
//!   [`Resources`], [`template`] parses them. No template lies in this tree.
//! - **Rows:** a box and its controls are rows in `window`'s table, so
//!   `window`'s `SetFocus` and `DispatchMessageA` take their handles.
//! - **Words:** `press` and `key` take the player's answers; a box's click is a
//!   flip, then a queued `WM_COMMAND`, so the procedure reads the new state.
//! - **The page's half:** [`view`] holds what a page shows as data; it reaches no
//!   browser, and the script under `page/` takes its text.
//! - **Walls:** a box draws nothing here, on any target, and no font rides
//!   here. `DialogBoxParamA` has two fronts: `dialog_box_param` returns at once,
//!   which holds only for a caller that drops the result, as a page must;
//!   `dialog_box_wait` runs the box's own loop, for a native target. The owner's
//!   window is sent none of the messages Windows sends it while a box is open.
//! - **Apart, no front:** `MessageBoxA`, `SetDlgItemTextA`, `WinHelpA` and
//!   `MessageBeep`, reached by no measured path.
//!
//! The crate names no game and nothing of any one target, so it builds for
//! `wasm32-unknown-unknown`; a game's facts live in its rows crate.
#![cfg_attr(not(test), no_std)]

#[cfg(any(test, feature = "desk"))]
extern crate alloc;

#[cfg(any(test, feature = "desk"))]
pub mod front;
#[cfg(any(test, feature = "plant"))]
pub mod plant;
pub mod table;
#[cfg(any(test, feature = "desk"))]
pub mod tables;
#[cfg(any(test, feature = "desk"))]
pub mod template;
#[cfg(test)]
mod test_log;
#[cfg(test)]
mod tests;
#[cfg(any(test, feature = "desk"))]
pub mod view;

use kernel_process::{Front, Process};

/// Where a template's bytes come from: the seam over `kernel-process`.
pub trait Resources {
    /// The bytes of resource `kind`, `id` in the module `instance`; `None`
    /// for a module or a resource it lacks. Writes no line.
    fn resource(&self, instance: u32, kind: u32, id: u32) -> Option<&[u8]>;
}

impl<P: Process> Resources for Front<P> {
    fn resource(&self, instance: u32, kind: u32, id: u32) -> Option<&[u8]> {
        Front::resource(self, instance, kind, id)
    }
}
