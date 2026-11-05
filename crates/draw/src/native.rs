// SPDX-License-Identifier: MIT
//! A host's back through softbuffer 0.4.8: Linux's X11 and Wayland, and
//! Windows; macOS builds untried, as `window`'s. Generic over the handle
//! traits, so this crate names no winit type: `window`'s `native::Lent` is
//! the one a program hands in.
//!
//! The caller sizes the surface, from the window's own size: this back never
//! asks the host. The walls, named: a window sized other than the picture
//! shows what fits from its top left corner, the rest black, never scaled
//! ([`Shown::to_zero_rgb`]); a window of no size paints nothing. Each
//! present writes one `draw.present` line, a refusal one `draw.refused` line;
//! the program goes on, as it does when a host drops a frame.

use core::cell::{Cell, RefCell};
use core::num::NonZeroU32;

use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use softbuffer::{Context, SoftBufferError, Surface};

use crate::easel::{Back, Shown};

/// One window's surface: softbuffer's end of the seam.
pub struct Native<H> {
    surface: RefCell<Surface<H, H>>,
    size: Cell<(u32, u32)>,
}

impl<H: HasDisplayHandle + HasWindowHandle + Clone> Native<H> {
    /// A surface over `window`, `width` by `height`; `Err` the host's refusal.
    pub fn new(window: H, width: u32, height: u32) -> Result<Self, SoftBufferError> {
        let context = Context::new(window.clone())?;
        let surface = Surface::new(&context, window)?;
        let back = Self {
            surface: RefCell::new(surface),
            size: Cell::new((0, 0)),
        };
        back.resize(width, height)?;
        Ok(back)
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

    /// `shown` on the surface, the words of its rows handed to the host.
    fn paint(&self, shown: &Shown) -> Result<(), SoftBufferError> {
        let (width, height) = self.size.get();
        if width == 0 || height == 0 {
            return Ok(());
        }
        let mut surface = self.surface.borrow_mut();
        let mut buffer = surface.buffer_mut()?;
        shown.to_zero_rgb(&mut buffer, width);
        buffer.present()
    }
}

impl<H: HasDisplayHandle + HasWindowHandle + Clone> Back for Native<H> {
    fn present(&self, shown: &Shown) {
        let (width, height) = self.size.get();
        match self.paint(shown) {
            Ok(()) => log::debug!(
                w:% = oxide_trace::__shown!(width),
                h:% = oxide_trace::__shown!(height);
                "draw.present"
            ),
            Err(refusal) => log::warn!(refused:% = refusal; "draw.refused"),
        }
    }
}
