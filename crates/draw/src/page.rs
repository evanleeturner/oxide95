// SPDX-License-Identifier: MIT
//! The page's back: it keeps the picture last presented until the page's
//! code reads it, once a frame. A second present before that read wins, the
//! one it replaces written as a `draw.drop` line: the program runs at its
//! own pace and the page shows what is newest.

use core::cell::{Cell, RefCell};

use crate::easel::{Back, Shown};

/// The page's end of the seam.
#[derive(Debug, Default)]
pub struct Page {
    kept: RefCell<Option<Shown>>,
    dropped: Cell<u32>,
    /// The picture last presented's width and height, for the script to size
    /// its `ImageData`.
    size: Cell<(u32, u32)>,
}

impl Page {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The width and height of the picture last presented; 0 before any.
    #[must_use]
    pub fn size(&self) -> (u32, u32) {
        self.size.get()
    }

    /// The picture as RGBA into `into`, as [`Shown::to_rgba`]; whether a new
    /// one came since the last read. No new one leaves `into` as it was.
    pub fn read(&self, into: &mut [u8]) -> bool {
        let Some(shown) = self.kept.take() else {
            return false;
        };
        shown.to_rgba(into);
        true
    }
}

impl Back for Page {
    fn present(&self, shown: &Shown) {
        self.size.set((shown.width, shown.height));
        if self.kept.replace(Some(shown.clone())).is_some() {
            let dropped = self.dropped.get().wrapping_add(1);
            self.dropped.set(dropped);
            log::debug!(n:% = oxide_trace::__shown!(dropped); "draw.drop");
        }
    }
}
