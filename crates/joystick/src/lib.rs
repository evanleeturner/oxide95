// SPDX-License-Identifier: MIT
//! The calls a program makes to a joystick: WINMM's `joyGetNumDevs`,
//! `joyGetDevCapsA` and `joyGetPosEx`, answered from a row, on any target.
//!
//! [`Front`] says no joystick is there. The count is 0, and asking a device's
//! capabilities is refused, so a program that adds only what answers adds none.
//! Each answer writes one `imp.call` line where it is computed: `c` the call's
//! name, `a` its numbers, `v` its answer, `by` that this crate answered.
//!
//! Not answered: reading a position. With no device there is nothing a position
//! could be read from, so `joyGetPosEx` is a stop by name, never a guess: one
//! `joy.stop` line, then [`Back::stop`].
//!
//! [`pads::PadFront`] answers the same calls with real pads behind them, one
//! module a place they come from: `page`, what a page's script hands in; `winmm`,
//! Windows' own calls, on Windows; `gilrs_back`, the `gilrs` library, on Linux and
//! macOS behind the `gilrs` feature. No real pad has been read through any of them:
//! `page/recorded-pad.txt` is written by hand, and neither `winmm`'s calls nor
//! `page/gamepads.mjs` in a browser have run. macOS has never been built.
//!
//! Not answered with a pad: any flags but a presence test or a plain read, as a
//! `joyGetPosEx` stops on them. `joyGetDevCapsA` gives no names, leaving those bytes
//! 0, and no period. `dwButtonNumber` is how many buttons are down on the page and
//! through `gilrs`, as no source read here says what else it should be.
//!
//! The crate names no game, nothing of any one target and no allocator, so it
//! builds for `wasm32-unknown-unknown` and runs where no heap exists yet.
#![cfg_attr(not(test), no_std)]

#[cfg(all(feature = "gilrs", any(target_os = "linux", target_os = "macos")))]
pub mod gilrs_back;
pub mod pads;
#[cfg(test)]
mod pads_tests;
pub mod page;
pub mod table;
#[cfg(test)]
mod test_log;
#[cfg(test)]
mod tests;
#[cfg(any(windows, test))]
pub mod winmm;
#[cfg(test)]
mod winmm_tests;

use table::Row;

/// No device, on any target.
const NO_DEVICES: u32 = 0;
/// `JOYERR_PARMS`: `JOYERR_BASE`, 160, plus 5.
const PARMS: u32 = 165;

/// What a target does for the front.
pub trait Back {
    /// Ends the program; the front has written the call's `joy.stop` line.
    fn stop(&self, name: &str) -> !;
    /// Writes one line around whatever the target must keep across it.
    fn quiet(&self, line: impl FnOnce()) {
        line();
    }
}

/// One file's joystick calls: its row, and its target.
pub struct Front<B> {
    file: &'static Row,
    back: B,
}

impl<B: Back> Front<B> {
    pub const fn new(file: &'static Row, back: B) -> Self {
        Self { file, back }
    }

    /// `joyGetNumDevs`: none.
    #[must_use]
    pub fn devices(&self) -> u32 {
        self.back.quiet(|| {
            log::debug!(
                c = self.file.calls.devices,
                v:% = oxide_trace::__shown!(NO_DEVICES),
                by = "joystick"; "imp.call"
            );
        });
        NO_DEVICES
    }

    /// `joyGetDevCapsA`: refused for every device and size. The caller's record
    /// is not handed in, as a refusal writes nothing to it.
    #[must_use]
    pub fn caps(&self, device: u32, size: u32) -> u32 {
        self.back.quiet(|| {
            log::debug!(
                c = self.file.calls.caps,
                a:% = oxide_trace::Args([
                    oxide_trace::__shown!(device),
                    oxide_trace::__shown!(size)
                ]),
                v:% = oxide_trace::__shown!(PARMS),
                by = "joystick"; "imp.call"
            );
        });
        PARMS
    }

    /// `joyGetPosEx`: stops, its number alone logged.
    pub fn position(&self, device: u32) -> ! {
        let name = self.file.calls.position;
        log::error!(
            c = name,
            a:% = oxide_trace::Args([oxide_trace::__shown!(device)]),
            by = "joystick"; "joy.stop"
        );
        self.back.stop(name)
    }
}
