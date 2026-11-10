// SPDX-License-Identifier: MIT
//! The Win32 target's end of the seam: the program's windows are Wine's own,
//! so Miles' and Smacker's own `USER32` calls find them, and the picture
//! reaches them by `SetDIBitsToDevice` alone. [`crate::easel::Easel`] is the same
//! as every target's: its buffers and contexts are this crate's, not GDI's.
//!
//! [`Real`] answers a front's questions from the real window. [`Window`] paints
//! [`Shown`] into it, its inside asked by `GetClientRect` each present: an
//! inside sized other than the picture shows it at the largest whole factor,
//! centred, black around ([`Shown::to_zero_rgb_sized`]). When the program is
//! told a smaller screen at a factor above 1 ([`told_factor`]), an inside no
//! whole factor fits, smaller than the picture on a side, shows the picture
//! at that factor from its corner, clipped, black past it
//! ([`Shown::to_zero_rgb_cornered`]): a window the program sized for its own
//! world, its picture at the world's scale.
//!
//! Each present writes one `draw.present` line at DEBUG, `by=draw`: the
//! picture's `w` and `h`, the source rectangle from 0,0; the factor `k`; the
//! destination `dx`, `dy`, `dw`, `dh` in the inside, clipped to it. One
//! `draw.fit` line at INFO when the fit, the inside's size or the picture's
//! differs from the last present's, as the host's back writes it.
//!
//! The walls, named: outside a told world, an inside smaller than the
//! picture on either side, or one whose size Wine refuses, shows the
//! picture's top left, since `SetDIBitsToDevice` clips; no window aimed yet
//! paints nothing and writes no line.
#![allow(
    unsafe_code,
    reason = "this back is calls into USER32 and GDI32 and nothing else"
)]

use alloc::vec::Vec;
use core::cell::{Cell, RefCell};

use window::user32::{kept, told_factor};

use crate::easel::{Back, Fit, Painted, Shown, unsigned};
use crate::front::Windows;

unsafe extern "system" {
    fn GetClientRect(window: u32, rectangle: *mut [i32; 4]) -> u32;
    fn GetDC(window: u32) -> u32;
    fn ReleaseDC(window: u32, dc: u32) -> u32;
    fn UpdateWindow(window: u32) -> u32;
    fn ValidateRect(window: u32, rectangle: *const [u32; 4]) -> u32;
    fn SetDIBitsToDevice(
        dc: u32,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        from_x: i32,
        from_y: i32,
        start: u32,
        lines: u32,
        bits: *const u32,
        header: *const [u32; 10],
        usage: u32,
    ) -> i32;
}

/// `BITMAPINFOHEADER`'s size in bytes, its first word.
const HEADER_BYTES: u32 = 40;
/// `biPlanes` 1 in the low half and `biBitCount` 32 in the high.
const PLANES_AND_DEPTH: u32 = 0x0020_0001;
/// `DIB_RGB_COLORS`: the words are colours.
const DIB_RGB_COLORS: u32 = 0;

/// The real windows, asked directly. Holds nothing: the handle is Wine's.
pub struct Real;

impl Windows for Real {
    fn quiet(&self, line: impl FnOnce()) {
        kept(line);
    }

    fn size(&self, window: u32) -> Option<(u32, u32)> {
        let mut rectangle = [0_i32; 4];
        // SAFETY: the rectangle is a local of the four words the call writes.
        let answer = unsafe { GetClientRect(window, &raw mut rectangle) };
        (answer != 0).then_some((unsigned(rectangle[2]), unsigned(rectangle[3])))
    }

    fn update(&self, window: u32) -> bool {
        // SAFETY: a handle only; Wine answers 0 for one that is not a window.
        // The window's procedure may run inside: no easel borrow crosses it.
        unsafe { UpdateWindow(window) != 0 }
    }

    fn validate(&self, window: u32, rectangle: Option<[u32; 4]>) -> bool {
        let rectangle = rectangle
            .as_ref()
            .map_or(core::ptr::null(), core::ptr::from_ref);
        // SAFETY: the rectangle is null or a local that outlives the call.
        unsafe { ValidateRect(window, rectangle) != 0 }
    }
}

/// The back: the real window the picture goes to, 0 until the layer aims it,
/// and the words it last painted, kept to save an allocation a present.
pub struct Window {
    aimed: Cell<u32>,
    words: RefCell<Vec<u32>>,
    /// The last present's, the one `draw.fit` is written against; `None`
    /// before the first.
    fitted: Cell<Option<Fitted>>,
}

/// What one present was painted at: its fit, and the inside's and the
/// picture's sizes the fit was made from, the inside `None` when Wine
/// refused its size.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Fitted {
    fit: Option<Fit>,
    inside: Option<(u32, u32)>,
    picture: (u32, u32),
}

impl Window {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            aimed: Cell::new(0),
            words: RefCell::new(Vec::new()),
            fitted: Cell::new(None),
        }
    }

    /// The window a present paints from now on; 0 paints nothing.
    pub fn aim(&self, window: u32) {
        self.aimed.set(window);
    }

    /// The window aimed at now.
    #[must_use]
    pub fn aimed(&self) -> u32 {
        self.aimed.get()
    }

    /// `shown` on `window`, fitted to its inside, or at the told factor from
    /// its corner where no whole factor fits it in a told world above 1:
    /// `SetDIBitsToDevice`'s answer, the rows it drew; what was painted; the
    /// inside's size, `None` when Wine refused it.
    fn paint(&self, window: u32, shown: &Shown) -> (i32, Painted, Option<(u32, u32)>) {
        let inside = Real.size(window);
        let mut words = self.words.borrow_mut();
        let picture = (shown.width, shown.height);
        let painted = match (inside, told_factor()) {
            (Some(surface), Some(factor))
                if factor > 1 && Fit::whole(picture, surface).is_none() =>
            {
                shown.to_zero_rgb_cornered(&mut words, surface, factor)
            }
            _ => shown.to_zero_rgb_sized(&mut words, inside),
        };
        let (width, height) = (painted.width, painted.height);
        let header = [
            HEADER_BYTES,
            width,
            height.wrapping_neg(),
            PLANES_AND_DEPTH,
            0,
            0,
            0,
            0,
            0,
            0,
        ];
        // SAFETY: a handle only; 0 is the answer for a window that is gone,
        // and `SetDIBitsToDevice` draws nothing on a null context.
        let dc = unsafe { GetDC(window) };
        // SAFETY: `words` holds `width` by `height` words and the header says
        // so, both alive across the call; a top-down picture's rows run from 0.
        let rows = unsafe {
            SetDIBitsToDevice(
                dc,
                0,
                0,
                width,
                height,
                0,
                0,
                0,
                height,
                words.as_ptr(),
                &raw const header,
                DIB_RGB_COLORS,
            )
        };
        // SAFETY: the context `GetDC` handed for this window, handed back.
        unsafe { ReleaseDC(window, dc) };
        (rows, painted, inside)
    }
}

impl Default for Window {
    fn default() -> Self {
        Self::new()
    }
}

impl Back for Window {
    fn present(&self, shown: &Shown) {
        let window = self.aimed.get();
        if window == 0 {
            return;
        }
        kept(|| {
            let (rows, painted, inside) = self.paint(window, shown);
            let (pw, ph) = (shown.width, shown.height);
            let (k, x, y) = painted
                .fit
                .map_or((0, 0, 0), |fit| (fit.factor, fit.x, fit.y));
            let now = Some(Fitted {
                fit: painted.fit,
                inside,
                picture: (pw, ph),
            });
            if self.fitted.replace(now) != now {
                let (wide, high) = inside.unwrap_or((0, 0));
                log::info!(
                    window:% = oxide_trace::__shown!(window),
                    k:% = oxide_trace::__shown!(k),
                    x:% = oxide_trace::__shown!(x),
                    y:% = oxide_trace::__shown!(y),
                    w:% = oxide_trace::__shown!(wide),
                    h:% = oxide_trace::__shown!(high),
                    pw:% = oxide_trace::__shown!(pw),
                    ph:% = oxide_trace::__shown!(ph),
                    by = "draw";
                    "draw.fit"
                );
            }
            let (dw, dh) = painted.fit.map_or((pw, ph), |fit| {
                (
                    (fit.factor * pw).min(painted.width),
                    (fit.factor * ph).min(painted.height),
                )
            });
            log::debug!(
                window:% = oxide_trace::__shown!(window),
                w:% = oxide_trace::__shown!(pw),
                h:% = oxide_trace::__shown!(ph),
                k:% = oxide_trace::__shown!(k),
                dx:% = oxide_trace::__shown!(x),
                dy:% = oxide_trace::__shown!(y),
                dw:% = oxide_trace::__shown!(dw),
                dh:% = oxide_trace::__shown!(dh),
                rows = rows,
                by = "draw";
                "draw.present"
            );
        });
    }
}
