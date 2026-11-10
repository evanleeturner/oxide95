// SPDX-License-Identifier: MIT
//! The whole-factor fit: one rule for the picture a drawing back shows and
//! the pointer a host's back hands on. The drawing crate shows the
//! program's picture on a larger surface by [`Fit::whole`]; a host's back
//! places its pointer in the program's rectangle by the same fit, through
//! [`PointerFit`], the rectangle being the picture. A back that tells the
//! program a screen of its own size places the program's windows on the
//! real screen by the same fit, through [`Told`].

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

/// The told world: a program told its screen is [`Told::screen`] wide and
/// high, its windows standing on the real screen at [`Fit::whole`] of the
/// told screen on the real one. A told point lands at the fit's corner plus
/// the factor times the point, a told width or height at the factor times
/// it; a real point comes back less the corner, divided by the factor
/// rounding down. A real screen the told size, or one no whole factor
/// fits, is factor 1 at 0, 0: every number as it came.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Told {
    screen: (u32, u32),
    fit: Fit,
}

impl Told {
    /// The told world of a `screen` on the `real` one, both
    /// `(width, height)`.
    #[must_use]
    pub fn new(screen: (u32, u32), real: (u32, u32)) -> Self {
        let fit = Fit::whole(screen, real).unwrap_or(Fit {
            factor: 1,
            x: 0,
            y: 0,
        });
        Self { screen, fit }
    }

    /// The screen's width and height the program is told.
    #[must_use]
    pub const fn screen(&self) -> (u32, u32) {
        self.screen
    }

    /// The told screen's fit on the real one; factor 1 at 0, 0 when none.
    #[must_use]
    pub const fn fit(&self) -> Fit {
        self.fit
    }

    /// The real screen's point for the told `point`.
    #[must_use]
    pub fn real_point(&self, point: (i32, i32)) -> (i32, i32) {
        let Fit { factor, x, y } = self.fit;
        (outward(point.0, x, factor), outward(point.1, y, factor))
    }

    /// The real width or height for a told one.
    #[must_use]
    pub fn real_span(&self, span: i32) -> i32 {
        outward(span, 0, self.fit.factor)
    }

    /// The told point for the real screen's `point`.
    #[must_use]
    pub fn told_point(&self, point: (i32, i32)) -> (i32, i32) {
        let Fit { factor, x, y } = self.fit;
        (inward(point.0, x, factor), inward(point.1, y, factor))
    }
}

/// `strip` plus `factor` times `at`, held to an `i32`.
fn outward(at: i32, strip: u32, factor: u32) -> i32 {
    held(i64::from(strip) + i64::from(factor) * i64::from(at))
}

/// `at` less `strip`, divided by `factor` rounding down, held to an `i32`.
/// A fit's factor is 1 at least.
fn inward(at: i32, strip: u32, factor: u32) -> i32 {
    held((i64::from(at) - i64::from(strip)).div_euclid(i64::from(factor)))
}

/// `value` held to an `i32`'s range.
fn held(value: i64) -> i32 {
    i32::try_from(value.clamp(i64::from(i32::MIN), i64::from(i32::MAX))).unwrap_or(i32::MAX)
}

#[cfg(test)]
mod tests {
    use super::{Fit, PointerFit, Told};
    use crate::Rect;

    /// The screen sizes the drawing crate's fit tests paint on, and the
    /// larger screens a player's box may hold.
    const REAL: [(u32, u32); 11] = [
        (640, 480),
        (639, 480),
        (640, 479),
        (0, 0),
        (641, 480),
        (800, 600),
        (1280, 960),
        (1280, 1080),
        (1440, 1080),
        (1920, 1080),
        (2560, 1440),
    ];

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

    /// The told world's fit is the whole-factor fit on every size, and
    /// factor 1 at 0, 0 where that fits nothing.
    #[test]
    fn the_told_worlds_fit_is_the_whole_factor_fit() {
        let alone = Fit {
            factor: 1,
            x: 0,
            y: 0,
        };
        for real in REAL {
            let told = Told::new((640, 480), real);
            assert_eq!(told.screen(), (640, 480));
            let want = Fit::whole((640, 480), real).unwrap_or(alone);
            assert_eq!(told.fit(), want, "real {real:?}");
        }
        let wide = Fit {
            factor: 2,
            x: 80,
            y: 60,
        };
        assert_eq!(Told::new((640, 480), (1440, 1080)).fit(), wide);
    }

    /// 640 by 480 told on 1440 by 1080: a told point lands on its 2 by 2
    /// block at 80, 60, a size doubles, and each real pixel of a block comes
    /// back as its told point; there and back is the identity on every size.
    #[test]
    fn a_told_point_lands_on_its_block_and_comes_back() {
        let told = Told::new((640, 480), (1440, 1080));
        assert_eq!(told.real_point((0, 0)), (80, 60));
        assert_eq!(told.real_point((640, 480)), (1360, 1020));
        assert_eq!((told.real_span(640), told.real_span(480)), (1280, 960));
        for x in 80..1360 {
            assert_eq!(told.told_point((x, 60)).0, (x - 80) / 2, "x {x}");
        }
        for y in 60..1020 {
            assert_eq!(told.told_point((80, y)).1, (y - 60) / 2, "y {y}");
        }
        assert_eq!(told.told_point((79, 59)), (-1, -1));
        for real in REAL {
            let told = Told::new((640, 480), real);
            for corner in [(0, 0), (639, 0), (0, 479), (639, 479), (640, 480)] {
                let there = told.real_point(corner);
                assert_eq!(told.told_point(there), corner, "real {real:?}");
            }
        }
    }

    /// A real screen the told size, or one no whole factor fits: every
    /// point and span as it came, the extremes too.
    #[test]
    fn a_told_world_with_no_larger_fit_hands_on_every_number() {
        let edges = [i32::MIN, -5, 0, 1, 479, 639, 640, i32::MAX];
        for real in [(640, 480), (639, 480), (640, 479), (0, 0)] {
            let told = Told::new((640, 480), real);
            for at in edges {
                for other in edges {
                    assert_eq!(told.real_point((at, other)), (at, other));
                    assert_eq!(told.told_point((at, other)), (at, other));
                }
                assert_eq!(told.real_span(at), at);
            }
        }
    }
}
