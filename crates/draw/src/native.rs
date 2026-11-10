// SPDX-License-Identifier: MIT
//! A host's back through softbuffer 0.4.8: Linux's X11 and Wayland, and
//! Windows; macOS builds untried, as `window`'s. Generic over the handle
//! traits, so this crate names no winit type: `window`'s `native::Lent` is
//! the one a program hands in.
//!
//! The caller sizes the surface, from the window's own size: this back never
//! asks the host. A window sized other than the picture shows it at the
//! largest whole factor, centred, black around, or from its top left corner
//! as [`Native::scale`] asks ([`Shown::to_zero_rgb_scaled`]). The walls,
//! named: a window smaller than the picture on either side shows what fits
//! from its top left corner, never scaled; a window of no size paints
//! nothing.
//!
//! Each present writes one `draw.present` line at DEBUG, the sizes and the
//! fit; one `draw.fit` line at INFO when the fit, the window's size or the
//! picture's differs from the last present's; a refusal one `draw.refused`
//! line, and the program goes on, as it does when a host drops a frame.

use core::cell::{Cell, RefCell};
use core::num::NonZeroU32;

use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use softbuffer::{Context, SoftBufferError, Surface};

use crate::easel::{Back, Fit, Scale, Shown};

/// One window's surface: softbuffer's end of the seam.
pub struct Native<H> {
    surface: RefCell<Surface<H, H>>,
    size: Cell<(u32, u32)>,
    scale: Cell<Scale>,
    /// The last painted present's, the one `draw.fit` is written against;
    /// `None` before the first.
    fitted: Cell<Option<Fitted>>,
}

/// What one present was painted at: its fit, and the surface's and the
/// picture's sizes the fit was made from.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Fitted {
    fit: Option<Fit>,
    surface: (u32, u32),
    picture: (u32, u32),
}

impl<H: HasDisplayHandle + HasWindowHandle + Clone> Native<H> {
    /// A surface over `window`, `width` by `height`, filled at the whole
    /// factor ([`Scale::Whole`]); `Err` the host's refusal.
    pub fn new(window: H, width: u32, height: u32) -> Result<Self, SoftBufferError> {
        let context = Context::new(window.clone())?;
        let surface = Surface::new(&context, window)?;
        let back = Self {
            surface: RefCell::new(surface),
            size: Cell::new((0, 0)),
            scale: Cell::new(Scale::default()),
            fitted: Cell::new(None),
        };
        back.resize(width, height)?;
        Ok(back)
    }

    /// How the next presents fill a surface sized other than the picture.
    pub fn scale(&self, scale: Scale) {
        self.scale.set(scale);
    }

    /// The surface's size now, the caller's to hand in as the window changes;
    /// `Err` the host's refusal, the size before kept.
    pub fn resize(&self, width: u32, height: u32) -> Result<(), SoftBufferError> {
        let (Some(wide), Some(high)) = (NonZeroU32::new(width), NonZeroU32::new(height)) else {
            self.size.set((0, 0));
            return Ok(());
        };
        self.surface.borrow_mut().resize(wide, high)?;
        self.size.set((width, height));
        Ok(())
    }

    /// `shown` on the surface, the words of its rows handed to the host; the
    /// fit it was painted at, `None` from the corner or on a surface of no
    /// size.
    fn paint(&self, shown: &Shown) -> Result<Option<Fit>, SoftBufferError> {
        let (width, height) = self.size.get();
        if width == 0 || height == 0 {
            return Ok(None);
        }
        let mut surface = self.surface.borrow_mut();
        let mut buffer = surface.buffer_mut()?;
        let fit = shown.to_zero_rgb_scaled(&mut buffer, width, self.scale.get());
        buffer.present()?;
        Ok(fit)
    }
}

impl<H: HasDisplayHandle + HasWindowHandle + Clone> Back for Native<H> {
    fn present(&self, shown: &Shown) {
        let (width, height) = self.size.get();
        match self.paint(shown) {
            Ok(fit) => {
                let (k, x, y) = fit.map_or((0, 0, 0), |fit| (fit.factor, fit.x, fit.y));
                let now = Some(Fitted {
                    fit,
                    surface: (width, height),
                    picture: (shown.width, shown.height),
                });
                if self.fitted.replace(now) != now {
                    log::info!(
                        k:% = oxide_trace::__shown!(k),
                        x:% = oxide_trace::__shown!(x),
                        y:% = oxide_trace::__shown!(y),
                        w:% = oxide_trace::__shown!(width),
                        h:% = oxide_trace::__shown!(height),
                        pw:% = oxide_trace::__shown!(shown.width),
                        ph:% = oxide_trace::__shown!(shown.height);
                        "draw.fit"
                    );
                }
                log::debug!(
                    w:% = oxide_trace::__shown!(width),
                    h:% = oxide_trace::__shown!(height),
                    k:% = oxide_trace::__shown!(k),
                    x:% = oxide_trace::__shown!(x),
                    y:% = oxide_trace::__shown!(y);
                    "draw.present"
                );
            }
            Err(refusal) => log::warn!(refused:% = refusal; "draw.refused"),
        }
    }
}
