// SPDX-License-Identifier: MIT
//! The whole-factor fit: one rule for the picture a drawing back shows and
//! the pointer a host's back hands on. The drawing crate shows the
//! program's picture on a larger surface by [`Fit::whole`]; a host's back
//! places its pointer in the program's rectangle by the same fit, through
//! [`PointerFit`], the rectangle being the picture. A back that tells the
//! program a screen of its own size places the program's windows on the
//! real screen by the same fit, through [`Told`], and reads the real
//! pointer back into the told screen by the pointer's rule.

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
/// fits, is factor 1 at 0, 0: every number as it came. A real point the
/// program reads in comes back by [`Told::read_point`], held to the told
/// screen, and the points a window's inside answers by its told corner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Told {
    screen: (u32, u32),
    real: (u32, u32),
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
        Self { screen, real, fit }
    }

    /// The screen's width and height the program is told.
    #[must_use]
    pub const fn screen(&self) -> (u32, u32) {
        self.screen
    }

    /// The real screen's width and height the told world stands on.
    #[must_use]
    pub const fn real(&self) -> (u32, u32) {
        self.real
    }

    /// The told point for a real screen `point` the program reads in: the
    /// told screen is the picture, shown on the real screen at the told
    /// world's fit, and the point is [`PointerFit::point`] of it there. On
    /// the picture that is [`Told::told_point`]; on the black strips, or off
    /// the real screen, the told screen's nearest edge pixel. A real screen
    /// the told size, or one no whole factor fits, hands on every number as
    /// it came.
    #[must_use]
    pub fn read_point(&self, point: (i32, i32)) -> (i32, i32) {
        let screen = Rect::sized(0, 0, side(self.screen.0), side(self.screen.1));
        PointerFit::new(screen, self.real).point(screen, point)
    }

    /// The told corner of a window's inside, from one real pixel's point on
    /// the real screen, `screen`, and in the inside, `client`, as Wine gave
    /// them: [`Told::read_point`] of the inside's real corner, `screen` less
    /// `client`. An inside placed on the told world's blocks has its own
    /// told corner; one whose real corner lies on a strip or off the real
    /// screen's top left reads the told screen's nearest edge, so an inside
    /// grown to the whole real screen has told corner 0, 0.
    #[must_use]
    pub fn told_corner(&self, screen: (i32, i32), client: (i32, i32)) -> (i32, i32) {
        self.read_point(less(screen, client))
    }

    /// `ScreenToClient`'s told answer for the program's told `point`, Wine
    /// handed [`Told::real_point`] of it and answering the real client point
    /// `real_answer`: `point` less the inside's [`Told::told_corner`]. Not
    /// held to the told screen: a told point outside it answers as far
    /// outside the inside.
    #[must_use]
    pub fn screen_to_client(&self, point: (i32, i32), real_answer: (i32, i32)) -> (i32, i32) {
        less(point, self.told_corner(self.real_point(point), real_answer))
    }

    /// `ClientToScreen`'s told answer for the program's `client` point, Wine
    /// handed it as given and answering the real screen point `real_answer`:
    /// `client` plus the inside's [`Told::told_corner`]. Wine's answer gives
    /// the corner alone, so [`Told::screen_to_client`] of this is `client`
    /// again wherever no number was held to an `i32`'s range on the way.
    #[must_use]
    pub fn client_to_screen(&self, client: (i32, i32), real_answer: (i32, i32)) -> (i32, i32) {
        plus(client, self.told_corner(real_answer, client))
    }

    /// A mouse message's told client point, `pt` the real screen point the
    /// message carries and `client` the real client point its `lParam`
    /// holds: [`Told::read_point`] of `pt` less the inside's
    /// [`Told::told_corner`], so a point on the strips reads the inside's
    /// nearest edge as the pointer does.
    #[must_use]
    pub fn message_point(&self, pt: (i32, i32), client: (i32, i32)) -> (i32, i32) {
        less(self.read_point(pt), self.told_corner(pt, client))
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

    /// The real screen's rectangle for a told window's `rect`, `frame` its
    /// edges about the inside as `AdjustWindowRect` makes them of a zero
    /// rectangle (left and top 0 or less, right and bottom 0 or more): the
    /// inside, `rect` less the frame, lands by [`Told::real_point`] of its
    /// corner and [`Told::real_span`] of its width and height, and the frame
    /// stands around it again at its own size, never scaled. A frame of 0
    /// maps the rectangle's corner and sizes alone. Does not check that the
    /// frame fits inside `rect`.
    #[must_use]
    pub fn real_rect(&self, rect: Rect, frame: Rect) -> Rect {
        let inside = within(rect, frame);
        let (left, top) = self.real_point((inside.left, inside.top));
        let width = self.real_span(inside.right.saturating_sub(inside.left));
        let height = self.real_span(inside.bottom.saturating_sub(inside.top));
        let real = Rect {
            left,
            top,
            right: left.saturating_add(width),
            bottom: top.saturating_add(height),
        };
        around(real, frame)
    }

    /// The told rectangle for a real window's `rect`, `frame` as for
    /// [`Told::real_rect`]: the inside's two corners by
    /// [`Told::told_point`], the frame around them again at its own size.
    /// `told_rect` of `real_rect` of a rectangle is that rectangle wherever
    /// no number was held to an `i32`'s range on the way.
    #[must_use]
    pub fn told_rect(&self, rect: Rect, frame: Rect) -> Rect {
        let inside = within(rect, frame);
        let (left, top) = self.told_point((inside.left, inside.top));
        let (right, bottom) = self.told_point((inside.right, inside.bottom));
        around(
            Rect {
                left,
                top,
                right,
                bottom,
            },
            frame,
        )
    }
}

/// The inside of a window whose outside is `rect`: each edge less
/// `frame`'s, held to an `i32`.
fn within(rect: Rect, frame: Rect) -> Rect {
    Rect {
        left: rect.left.saturating_sub(frame.left),
        top: rect.top.saturating_sub(frame.top),
        right: rect.right.saturating_sub(frame.right),
        bottom: rect.bottom.saturating_sub(frame.bottom),
    }
}

/// The outside of a window whose inside is `inside`: each edge plus
/// `frame`'s, held to an `i32`.
fn around(inside: Rect, frame: Rect) -> Rect {
    Rect {
        left: inside.left.saturating_add(frame.left),
        top: inside.top.saturating_add(frame.top),
        right: inside.right.saturating_add(frame.right),
        bottom: inside.bottom.saturating_add(frame.bottom),
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

/// `a` less `b`, each axis held to an `i32`.
fn less(a: (i32, i32), b: (i32, i32)) -> (i32, i32) {
    (
        held(i64::from(a.0) - i64::from(b.0)),
        held(i64::from(a.1) - i64::from(b.1)),
    )
}

/// `a` plus `b`, each axis held to an `i32`.
fn plus(a: (i32, i32), b: (i32, i32)) -> (i32, i32) {
    (
        held(i64::from(a.0) + i64::from(b.0)),
        held(i64::from(a.1) + i64::from(b.1)),
    )
}

/// A width or height as an `i32`, held to its range.
fn side(span: u32) -> i32 {
    i32::try_from(span).unwrap_or(i32::MAX)
}

#[cfg(test)]
mod tests {
    use super::{Fit, PointerFit, Told, side};
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

    /// The frame `AdjustWindowRect` makes of a zero rectangle for a framed
    /// top window with a caption, as the program's own call read it in a
    /// play: left 3, top 29, right 3, bottom 3.
    const CAPTION: Rect = Rect {
        left: -3,
        top: -29,
        right: 3,
        bottom: 3,
    };

    /// A frame of 0: a rectangle lands by its corner and sizes alone and
    /// comes back by its two corners, on every size.
    #[test]
    fn a_frame_of_0_maps_the_corner_and_sizes_alone() {
        let none = Rect::default();
        for real in REAL {
            let told = Told::new((640, 480), real);
            for rect in [screen(0, 0), Rect::sized(157, 124, 326, 232)] {
                let (left, top) = told.real_point((rect.left, rect.top));
                let width = told.real_span(rect.right - rect.left);
                let height = told.real_span(rect.bottom - rect.top);
                let there = told.real_rect(rect, none);
                assert_eq!(there, Rect::sized(left, top, width, height), "{real:?}");
                let (left, top) = told.told_point((there.left, there.top));
                let (right, bottom) = told.told_point((there.right, there.bottom));
                let back = Rect {
                    left,
                    top,
                    right,
                    bottom,
                };
                assert_eq!(told.told_rect(there, none), back, "{real:?}");
            }
        }
    }

    /// A framed window told 157, 124, 326 wide and 232 high on 1440 by 1080:
    /// its inside 160, 153, 320 by 200 lands at 400, 366, 640 by 400, the
    /// frame around it at its own size, and the real rectangle comes back.
    /// The numbers are a play's: the program's window, the frame its own
    /// call read.
    #[test]
    fn a_framed_windows_inside_lands_on_its_blocks_and_its_frame_keeps_its_size() {
        let told = Told::new((640, 480), (1440, 1080));
        let rect = Rect::sized(157, 124, 326, 232);
        let real = Rect::sized(397, 337, 646, 432);
        assert_eq!(told.real_rect(rect, CAPTION), real);
        assert_eq!(told.told_rect(real, CAPTION), rect);
    }

    /// There and back, framed or not, is the identity on every size.
    #[test]
    fn a_rectangle_there_and_back_is_itself() {
        let rects = [
            screen(0, 0),
            Rect::sized(157, 124, 326, 232),
            Rect::sized(-40, -30, 100, 80),
        ];
        for real in REAL {
            let told = Told::new((640, 480), real);
            for frame in [Rect::default(), CAPTION] {
                for rect in rects {
                    let there = told.real_rect(rect, frame);
                    assert_eq!(told.told_rect(there, frame), rect, "{real:?} {frame:?}");
                }
            }
        }
    }

    /// The larger screens the told world is played on, a strip on each:
    /// factor 2 with strips on all four sides, factor 3 with strips left and
    /// right, factor 2 with strips on all four sides, wider left and right,
    /// and factor 1 with strips on all four sides.
    const LARGER: [(u32, u32); 4] = [(1440, 1080), (2560, 1440), (1920, 1080), (800, 600)];

    /// One axis of a read point by the strips' rule: `at` before the
    /// picture's first block reads 0, at or past its end the last of `span`
    /// pixels, and on the picture `on`, the told point's axis.
    fn by_strips(at: i32, strip: u32, factor: u32, span: i32, on: i32) -> i32 {
        let first = side(strip);
        if at < first {
            0
        } else if at >= first + side(factor) * span {
            span - 1
        } else {
            on
        }
    }

    /// Every real pixel of each larger screen: on the picture the read point
    /// is the told point, on the strips the told screen's nearest edge.
    #[test]
    fn every_real_pixel_reads_its_told_point_or_the_nearest_edge() {
        for real in LARGER {
            let told = Told::new((640, 480), real);
            let Fit { factor, x, y } = told.fit();
            for down in 0..side(real.1) {
                for across in 0..side(real.0) {
                    let on = told.told_point((across, down));
                    let want = (
                        by_strips(across, x, factor, 640, on.0),
                        by_strips(down, y, factor, 480, on.1),
                    );
                    let read = told.read_point((across, down));
                    assert_eq!(read, want, "{real:?} at {across}, {down}");
                }
            }
        }
    }

    /// Each told point given out lands on its block, and every real pixel
    /// of that block reads the told point back, on each larger screen.
    #[test]
    fn every_told_points_block_reads_it_back() {
        for real in LARGER {
            let told = Told::new((640, 480), real);
            let factor = side(told.fit().factor);
            for down in 0..480 {
                for across in 0..640 {
                    let (left, top) = told.real_point((across, down));
                    for step_down in 0..factor {
                        for step_across in 0..factor {
                            let pixel = (left + step_across, top + step_down);
                            let read = told.read_point(pixel);
                            assert_eq!(read, (across, down), "{real:?} at {pixel:?}");
                        }
                    }
                }
            }
        }
    }

    /// The numbers a play measured: 640 by 480 told on 1440 by 1080, the
    /// program's inside grown to the whole real screen, its corner at real
    /// 0, 0. The picture's centre, real 720, 540, reads the told centre by
    /// the pointer and by a message whose client point Wine gave as 720,
    /// 540; the program's `ScreenToClient` of the told centre, Wine handed
    /// 720, 540 and answering 720, 540, is the told centre, and its
    /// `ClientToScreen` of the told centre, Wine handed 320, 240 and
    /// answering 320, 240, is the told centre too.
    #[test]
    fn the_pictures_centre_on_a_whole_screen_inside_reads_the_told_centre() {
        let told = Told::new((640, 480), (1440, 1080));
        assert_eq!(told.real(), (1440, 1080));
        assert_eq!(told.read_point((720, 540)), (320, 240));
        assert_eq!(told.told_corner((720, 540), (720, 540)), (0, 0));
        assert_eq!(told.message_point((720, 540), (720, 540)), (320, 240));
        assert_eq!(told.real_point((320, 240)), (720, 540));
        assert_eq!(told.screen_to_client((320, 240), (720, 540)), (320, 240));
        assert_eq!(told.client_to_screen((320, 240), (320, 240)), (320, 240));
    }

    /// The framed window's inside a play placed at real 400, 366 on 1440 by
    /// 1080 has its own told corner, 160, 153; a real client point 100, 100
    /// there is told 50, 50 by a message, by `ScreenToClient` of told 210,
    /// 203, and `ClientToScreen` of told 50, 50 answers 210, 203.
    #[test]
    fn a_framed_windows_inside_on_its_blocks_has_its_own_told_corner() {
        let told = Told::new((640, 480), (1440, 1080));
        assert_eq!(told.told_corner((500, 466), (100, 100)), (160, 153));
        assert_eq!(told.message_point((500, 466), (100, 100)), (50, 50));
        assert_eq!(told.real_point((210, 203)), (500, 466));
        assert_eq!(told.screen_to_client((210, 203), (100, 100)), (50, 50));
        assert_eq!(told.client_to_screen((50, 50), (450, 416)), (210, 203));
    }

    /// The strips' corners on 1440 by 1080: real 0, 0 reads the told
    /// screen's first pixel, real 1439, 1079 its last.
    #[test]
    fn the_real_screens_corners_read_the_told_screens_corners() {
        let told = Told::new((640, 480), (1440, 1080));
        assert_eq!(told.read_point((0, 0)), (0, 0));
        assert_eq!(told.read_point((1439, 1079)), (639, 479));
    }

    /// An inside whose real corner is off the real screen's top left, at
    /// -10, -10, has told corner 0, 0: the named limit.
    #[test]
    fn an_inside_off_the_real_screens_top_left_has_told_corner_0_0() {
        let told = Told::new((640, 480), (1440, 1080));
        assert_eq!(told.told_corner((-5, -5), (5, 5)), (0, 0));
    }

    /// Wine's two calls as the real inside answers them, its real corner at
    /// `corner`: `ClientToScreen` adds the corner, `ScreenToClient` takes it
    /// away. On every size, with the corner on a block, at 0, 0 and off a
    /// block, the told `ScreenToClient` of the told `ClientToScreen` of a
    /// client point is that point.
    #[test]
    fn client_to_screen_and_back_is_the_client_point_at_any_corner() {
        let clients = [(0, 0), (1, 1), (50, 50), (319, 239), (639, 479), (-7, -3)];
        for real in REAL {
            let told = Told::new((640, 480), real);
            for corner in [(0, 0), told.real_point((160, 153)), (401, 367)] {
                for client in clients {
                    let wine = (client.0 + corner.0, client.1 + corner.1);
                    let screen = told.client_to_screen(client, wine);
                    let handed = told.real_point(screen);
                    let wine = (handed.0 - corner.0, handed.1 - corner.1);
                    let back = told.screen_to_client(screen, wine);
                    assert_eq!(back, client, "{real:?} corner {corner:?}");
                }
            }
        }
    }

    /// A real screen the told size, or one no whole factor fits: the read
    /// point hands on every number, the extremes too, and the three answers
    /// built on it hand on Wine's own, wherever no number is held to an
    /// `i32`'s range on the way.
    #[test]
    fn with_no_larger_fit_every_read_point_and_answer_is_as_it_came() {
        let edges = [i32::MIN, -5, 0, 1, 479, 639, 640, i32::MAX];
        let halves = [i32::MIN / 2, -5, 0, 1, 479, 639, 640, i32::MAX / 2];
        for real in [(640, 480), (639, 480), (640, 479), (0, 0)] {
            let told = Told::new((640, 480), real);
            for at in edges {
                for other in edges {
                    assert_eq!(told.read_point((at, other)), (at, other), "{real:?}");
                }
            }
            for given in halves {
                for wine in halves {
                    let point = (given, wine);
                    let answer = (wine, given);
                    assert_eq!(told.screen_to_client(point, answer), answer, "{real:?}");
                    assert_eq!(told.client_to_screen(point, answer), answer, "{real:?}");
                    assert_eq!(told.message_point(point, answer), answer, "{real:?}");
                }
            }
        }
    }
}
