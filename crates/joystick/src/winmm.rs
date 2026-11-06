// SPDX-License-Identifier: MIT
//! The pads through winmm, its calls and records declared by hand, with no outside crate.
//!
//! winmm's own answer to anything is passed up as "nothing there": the front decides what
//! the program hears. Every read asks for all it can, since the front writes only what the
//! program asked. A pad's own span is kept, so its numbers mean what the pad says they mean.
#![allow(unsafe_code)]

use core::mem::{offset_of, size_of};

use crate::pads::{Pads, Reading, Shape, Span};

/// `JOY_RETURNALL`, plus `JOY_RETURNCENTERED`.
const READ_ALL: u32 = 0xff | 0x400;

/// winmm's `JOYINFOEX`. Its numbers are mingw-w64's `joystickapi.h`, each checked by the C
/// compiler in `joy-layout.c`, kept with the records of the check.
#[derive(Default)]
#[repr(C)]
pub(crate) struct JoyInfoEx {
    pub(crate) size: u32,
    pub(crate) flags: u32,
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) z: u32,
    pub(crate) r: u32,
    pub(crate) u: u32,
    pub(crate) v: u32,
    pub(crate) buttons: u32,
    pub(crate) button_number: u32,
    pub(crate) pov: u32,
    reserved: [u32; 2],
}

/// winmm's `JOYCAPSA`, from the same header.
#[repr(C)]
pub(crate) struct JoyCaps {
    mid: u16,
    pid: u16,
    name: [u8; 32],
    pub(crate) x_min: u32,
    pub(crate) x_max: u32,
    pub(crate) y_min: u32,
    pub(crate) y_max: u32,
    pub(crate) z_min: u32,
    pub(crate) z_max: u32,
    pub(crate) buttons: u32,
    period_min: u32,
    period_max: u32,
    pub(crate) r_min: u32,
    pub(crate) r_max: u32,
    pub(crate) u_min: u32,
    pub(crate) u_max: u32,
    pub(crate) v_min: u32,
    pub(crate) v_max: u32,
    pub(crate) caps: u32,
    max_axes: u32,
    pub(crate) axes: u32,
    max_buttons: u32,
    reg_key: [u8; 32],
    oem_driver: [u8; 260],
}

impl JoyCaps {
    const fn empty() -> Self {
        Self {
            mid: 0,
            pid: 0,
            name: [0; 32],
            x_min: 0,
            x_max: 0,
            y_min: 0,
            y_max: 0,
            z_min: 0,
            z_max: 0,
            buttons: 0,
            period_min: 0,
            period_max: 0,
            r_min: 0,
            r_max: 0,
            u_min: 0,
            u_max: 0,
            v_min: 0,
            v_max: 0,
            caps: 0,
            max_axes: 0,
            axes: 0,
            max_buttons: 0,
            reg_key: [0; 32],
            oem_driver: [0; 260],
        }
    }
}

/// Nothing runs these structs on a machine without winmm, so the compiler is the witness.
const _: () = {
    assert!(size_of::<JoyInfoEx>() == 52);
    assert!(offset_of!(JoyInfoEx, size) == 0);
    assert!(offset_of!(JoyInfoEx, flags) == 4);
    assert!(offset_of!(JoyInfoEx, x) == 8);
    assert!(offset_of!(JoyInfoEx, y) == 12);
    assert!(offset_of!(JoyInfoEx, z) == 16);
    assert!(offset_of!(JoyInfoEx, r) == 20);
    assert!(offset_of!(JoyInfoEx, u) == 24);
    assert!(offset_of!(JoyInfoEx, v) == 28);
    assert!(offset_of!(JoyInfoEx, buttons) == 32);
    assert!(offset_of!(JoyInfoEx, button_number) == 36);
    assert!(offset_of!(JoyInfoEx, pov) == 40);
    assert!(offset_of!(JoyInfoEx, reserved) == 44);

    assert!(size_of::<JoyCaps>() == 404);
    assert!(offset_of!(JoyCaps, mid) == 0);
    assert!(offset_of!(JoyCaps, pid) == 2);
    assert!(offset_of!(JoyCaps, name) == 4);
    assert!(offset_of!(JoyCaps, x_min) == 36);
    assert!(offset_of!(JoyCaps, x_max) == 40);
    assert!(offset_of!(JoyCaps, y_min) == 44);
    assert!(offset_of!(JoyCaps, y_max) == 48);
    assert!(offset_of!(JoyCaps, z_min) == 52);
    assert!(offset_of!(JoyCaps, z_max) == 56);
    assert!(offset_of!(JoyCaps, buttons) == 60);
    assert!(offset_of!(JoyCaps, period_min) == 64);
    assert!(offset_of!(JoyCaps, period_max) == 68);
    assert!(offset_of!(JoyCaps, r_min) == 72);
    assert!(offset_of!(JoyCaps, r_max) == 76);
    assert!(offset_of!(JoyCaps, u_min) == 80);
    assert!(offset_of!(JoyCaps, u_max) == 84);
    assert!(offset_of!(JoyCaps, v_min) == 88);
    assert!(offset_of!(JoyCaps, v_max) == 92);
    assert!(offset_of!(JoyCaps, caps) == 96);
    assert!(offset_of!(JoyCaps, max_axes) == 100);
    assert!(offset_of!(JoyCaps, axes) == 104);
    assert!(offset_of!(JoyCaps, max_buttons) == 108);
    assert!(offset_of!(JoyCaps, reg_key) == 112);
    assert!(offset_of!(JoyCaps, oem_driver) == 144);
};

#[allow(clippy::cast_possible_truncation)]
const CAPS_SIZE: u32 = size_of::<JoyCaps>() as u32;
#[allow(clippy::cast_possible_truncation)]
const POSITION_SIZE: u32 = size_of::<JoyInfoEx>() as u32;

/// winmm's three calls, the last two answering an `MMRESULT`, 0 for success. The seam: the
/// real ones behind `cfg(windows)`, a fake in the tests.
pub(crate) trait Winmm {
    fn devices(&self) -> u32;
    /// # Safety
    /// `record` is valid for `size` bytes through this call.
    unsafe fn caps(&self, device: usize, record: *mut JoyCaps, size: u32) -> u32;
    /// # Safety
    /// `record` is valid through this call and its `size` says how long it is.
    unsafe fn position(&self, device: u32, record: *mut JoyInfoEx) -> u32;
}

/// The pads over any [`Winmm`].
pub(crate) struct Through<W: Winmm>(pub(crate) W);

impl<W: Winmm> Pads for Through<W> {
    fn devices(&self) -> u32 {
        self.0.devices()
    }

    fn shape(&self, device: u32) -> Option<Shape> {
        let mut record = JoyCaps::empty();
        // SAFETY: a local alive through the call, of the size handed with it.
        let code = unsafe { self.0.caps(device as usize, &raw mut record, CAPS_SIZE) };
        (code == 0).then_some(Shape {
            spans: [
                Span {
                    min: record.x_min,
                    max: record.x_max,
                },
                Span {
                    min: record.y_min,
                    max: record.y_max,
                },
                Span {
                    min: record.z_min,
                    max: record.z_max,
                },
                Span {
                    min: record.r_min,
                    max: record.r_max,
                },
                Span {
                    min: record.u_min,
                    max: record.u_max,
                },
                Span {
                    min: record.v_min,
                    max: record.v_max,
                },
            ],
            caps: record.caps,
            axes: record.axes,
            buttons: record.buttons,
        })
    }

    fn reading(&self, device: u32) -> Option<Reading> {
        let mut record = JoyInfoEx {
            size: POSITION_SIZE,
            flags: READ_ALL,
            ..JoyInfoEx::default()
        };
        // SAFETY: a local alive through the call, whose `size` is its own.
        let code = unsafe { self.0.position(device, &raw mut record) };
        (code == 0).then_some(Reading {
            axes: [record.x, record.y, record.z, record.r, record.u, record.v],
            buttons: record.buttons,
            pressed: record.button_number,
            pov: record.pov,
        })
    }
}

#[cfg(windows)]
pub use real::WinmmPads;

#[cfg(windows)]
mod real {
    use super::{JoyCaps, JoyInfoEx, Pads, Reading, Shape, Through, Winmm};

    #[link(name = "winmm")]
    unsafe extern "system" {
        fn joyGetNumDevs() -> u32;
        fn joyGetDevCapsA(device: usize, record: *mut JoyCaps, size: u32) -> u32;
        fn joyGetPosEx(device: u32, record: *mut JoyInfoEx) -> u32;
    }

    struct Real;

    impl Winmm for Real {
        fn devices(&self) -> u32 {
            // SAFETY: it takes nothing and has no precondition.
            unsafe { joyGetNumDevs() }
        }

        unsafe fn caps(&self, device: usize, record: *mut JoyCaps, size: u32) -> u32 {
            // SAFETY: the trait's own.
            unsafe { joyGetDevCapsA(device, record, size) }
        }

        unsafe fn position(&self, device: u32, record: *mut JoyInfoEx) -> u32 {
            // SAFETY: the trait's own.
            unsafe { joyGetPosEx(device, record) }
        }
    }

    /// The pads through winmm. A newtype, so the seam the tests use never leaks out.
    pub struct WinmmPads(Through<Real>);

    impl WinmmPads {
        /// `const`, so a `static` can hold a front over it.
        #[must_use]
        pub const fn new() -> Self {
            Self(Through(Real))
        }
    }

    impl Default for WinmmPads {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Pads for WinmmPads {
        fn devices(&self) -> u32 {
            self.0.devices()
        }

        fn shape(&self, device: u32) -> Option<Shape> {
            self.0.shape(device)
        }

        fn reading(&self, device: u32) -> Option<Reading> {
            self.0.reading(device)
        }
    }
}
