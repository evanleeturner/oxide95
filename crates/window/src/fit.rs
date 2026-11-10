// SPDX-License-Identifier: MIT
//! The whole-factor fit: one rule for the picture a drawing back shows and
//! the pointer a host's back hands on. The drawing crate shows the
//! program's picture on a larger surface by [`Fit::whole`]; a host's back
//! places its pointer in the program's rectangle by the same fit, through
//! [`PointerFit`], the rectangle being the picture.

use crate::Rect;

/// Where a picture lands on a larger surface: each of its pixels a `factor`
/// by `factor` block, its top left corner at `x`, `y` of the surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fit {
    pub factor: u32,
    pub x: u32,
    pub y: u32,
}

impl Fit {
    /// The largest whole factor that keeps `picture` inside `surface`, both
    /// `(width, height)`, centred, an odd strip's extra pixel right and below;
    /// `None` when a side of the picture is 0 or the surface is smaller than
    /// the picture on either side.
    #[must_use]
    pub fn whole(picture: (u32, u32), surface: (u32, u32)) -> Option<Self> {
        let (width, height) = picture;
        let factor = surface
            .0
            .checked_div(width)?
            .min(surface.1.checked_div(height)?);
        (factor > 0).then(|| Self {
            factor,
            x: (surface.0 - factor * width) / 2,
            y: (surface.1 - factor * height) / 2,
        })
    }
}

/// How a host's pointer over a window's inside is placed in the program's
/// rectangle: the rectangle is the picture, shown on the inside at
/// [`Fit::whole`], as the drawing crate shows a buffer the rectangle's size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PointerFit {
    fit: Option<Fit>,
    inside: (u32, u32),
    picture: (u32, u32),
}

impl PointerFit {
    /// The fit of `rect` on an inside `inside` wide and high, in the host's
    /// pixels; a side of `rect` below 0 counts as 0, which fits nothing.
    #[must_use]
    pub fn new(rect: Rect, inside: (u32, u32)) -> Self {
        let side = |span: i32| u32::try_from(span).unwrap_or(0);
        let picture = (
            side(rect.right.saturating_sub(rect.left)),
            side(rect.bottom.saturating_sub(rect.top)),
        );
        Self {
            fit: Fit::whole(picture, inside),
            inside,
            picture,
        }
    }

    /// The fit, `None` when the inside is smaller than the rectangle on
    /// either side or the rectangle has a side of 0.
    #[must_use]
    pub const fn fit(&self) -> Option<Fit> {
        self.fit
    }

    /// The window's inside this fit was made on, width and height in the
    /// host's pixels.
    #[must_use]
    pub const fn inside(&self) -> (u32, u32) {
        self.inside
    }

    /// The rectangle's width and height this fit was made from.
    #[must_use]
    pub const fn picture(&self) -> (u32, u32) {
        self.picture
    }

    /// The program's point for the host's pointer at `host` in the window's
    /// inside, `rect` the program's rectangle: the rectangle's corner plus
    /// the host point less the strip, divided by the factor; a host point
    /// on the black strips, or outside the inside, at the picture's nearest
    /// edge pixel. An inside the rectangle's own size, or one no whole
    /// factor fits, hands on the rectangle's corner plus the host point,
    /// never held to the rectangle.
    #[must_use]
    pub fn point(&self, rect: Rect, host: (i32, i32)) -> (i32, i32) {
        match self.fit {
            Some(fit) if self.inside != self.picture => (
                rect.left
                    .saturating_add(axis(host.0, fit.x, fit.factor, self.picture.0)),
                rect.top
                    .saturating_add(axis(host.1, fit.y, fit.factor, self.picture.1)),
            ),
            _ => (
                rect.left.saturating_add(host.0),
                rect.top.saturating_add(host.1),
            ),
        }
    }
}

/// One axis of [`PointerFit::point`]: `host` less `strip`, divided by
/// `factor` rounding down, held to the `span` pixels of the picture on that
/// axis. A fit's factor and span are each 1 at least.
fn axis(host: i32, strip: u32, factor: u32, span: u32) -> i32 {
    let at = (i64::from(host) - i64::from(strip)).div_euclid(i64::from(factor));
    i32::try_from(at.clamp(0, i64::from(span) - 1)).unwrap_or(i32::MAX)
}

#[cfg(test)]
mod tests {
    use super::{Fit, PointerFit};
    use crate::Rect;

    /// A 640 by 480 rectangle, its corner at `left`, `top`.
    const fn screen(left: i32, top: i32) -> Rect {
        Rect::sized(left, top, 640, 480)
    }

    /// 640 by 480 on 2560 by 1440: factor 3, a strip 320 wide each side;
    /// every column of the picture's 1,920 and every row of the 1,440 lands
    /// on its block, and a rectangle's corner is added to the point.
    #[test]
    fn on_2560_by_1440_each_host_pixel_lands_on_its_3_by_3_block() {
        let rect = screen(0, 0);
        let pointer = PointerFit::new(rect, (2560, 1440));
        let fit = Fit {
            factor: 3,
            x: 320,
            y: 0,
        };
        assert_eq!(pointer.fit(), Some(fit));
        for x in 320..2240 {
            assert_eq!(pointer.point(rect, (x, 0)).0, (x - 320) / 3, "x {x}");
        }
        for y in 0..1440 {
            assert_eq!(pointer.point(rect, (320, y)).1, y / 3, "y {y}");
        }
        assert_eq!(pointer.point(rect, (323, 3)), (1, 1));
        assert_eq!(pointer.point(rect, (2239, 1439)), (639, 479));
        let moved = screen(100, 50);
        let pointer = PointerFit::new(moved, (2560, 1440));
        assert_eq!(pointer.point(moved, (323, 3)), (101, 51));
    }

    /// An inside the picture's own size: every point the rectangle's corner
    /// plus the host's, and past either edge too.
    #[test]
    fn an_inside_the_pictures_size_hands_on_the_host_point_plus_the_corner() {
        let rect = screen(100, 50);
        let pointer = PointerFit::new(rect, (640, 480));
        for y in 0..480 {
            for x in 0..640 {
                assert_eq!(pointer.point(rect, (x, y)), (100 + x, 50 + y));
            }
        }
        assert_eq!(pointer.point(rect, (-5, -5)), (95, 45));
        assert_eq!(pointer.point(rect, (700, 500)), (800, 550));
    }

    /// The black strips either side, above and below, and host points past
    /// the inside: each at the picture's nearest edge pixel.
    #[test]
    fn a_point_on_the_strips_or_past_the_inside_stands_at_the_nearest_edge() {
        let rect = screen(0, 0);
        let wide = PointerFit::new(rect, (2560, 1440));
        for x in (0..320).chain(2240..2560) {
            let want = if x < 320 { 0 } else { 639 };
            assert_eq!(wide.point(rect, (x, 700)).0, want, "x {x}");
        }
        assert_eq!(wide.point(rect, (-5, -5)), (0, 0));
        assert_eq!(wide.point(rect, (5000, 5000)), (639, 479));
        let tall = PointerFit::new(rect, (1280, 1080));
        let fit = Fit {
            factor: 2,
            x: 0,
            y: 60,
        };
        assert_eq!(tall.fit(), Some(fit));
        for y in (0..60).chain(1020..1080) {
            let want = if y < 60 { 0 } else { 479 };
            assert_eq!(tall.point(rect, (640, y)).1, want, "y {y}");
        }
    }

    /// An inside narrower or shorter than the picture fits no whole factor:
    /// every point the rectangle's corner plus the host's.
    #[test]
    fn an_inside_smaller_than_the_picture_hands_on_the_host_point_plus_the_corner() {
        let rect = screen(100, 50);
        for inside in [(639, 480), (640, 479)] {
            let pointer = PointerFit::new(rect, inside);
            assert_eq!(pointer.fit(), None);
            for y in 0..480 {
                for x in 0..640 {
                    assert_eq!(pointer.point(rect, (x, y)), (100 + x, 50 + y));
                }
            }
        }
    }
}
