// SPDX-License-Identifier: MIT
//! The picture: one square on a plain ground, 8 bits a pixel, as every picture the
//! layer draws is. It drifts right on its own, so a picture that stops is visible,
//! unless the page holds it still; a held key lifts it either way, so a key that
//! crosses is visible too.

/// The palette index of the ground.
pub const GROUND: u8 = 0;
/// The palette index of the square.
pub const SQUARE: u8 = 1;
/// The square's side in pixels.
pub const SIDE: u32 = 32;
/// How far the square drifts each picture.
const DRIFT: u32 = 4;
/// How far a held key lifts the square each picture.
const LIFT: u32 = 4;

/// The square's place on a screen of a given size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mark {
    width: u32,
    height: u32,
    left: u32,
    top: u32,
}

impl Mark {
    /// At the left edge, halfway down. A screen no bigger than the square never moves it.
    #[must_use]
    pub const fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            left: 0,
            top: height.saturating_sub(SIDE) / 2,
        }
    }

    #[must_use]
    pub const fn left(self) -> u32 {
        self.left
    }

    #[must_use]
    pub const fn top(self) -> u32 {
        self.top
    }

    /// One picture's move: the drift when `drifts`, and the lift when `lifted`, each
    /// wrapping round.
    pub const fn step(&mut self, lifted: bool, drifts: bool) {
        let across = self.width.saturating_sub(SIDE);
        let down = self.height.saturating_sub(SIDE);
        if drifts && across > 0 {
            self.left = (self.left + DRIFT) % across;
        }
        if lifted && down > 0 {
            self.top = (self.top + down - LIFT % down) % down;
        }
    }

    /// The picture into `bits`, rows top first with no gap, `width` bytes a row.
    pub fn paint(self, bits: &mut [u8]) {
        bits.fill(GROUND);
        let (left, width) = (self.left as usize, self.width as usize);
        for row in bits
            .chunks_exact_mut(width.max(1))
            .skip(self.top as usize)
            .take(SIDE as usize)
        {
            let end = (left + SIDE as usize).min(width);
            row[left..end].fill(SQUARE);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_drifts_and_wraps() {
        let mut mark = Mark::new(64, 64);
        assert_eq!((mark.left(), mark.top()), (0, 16));
        mark.step(false, true);
        assert_eq!((mark.left(), mark.top()), (4, 16));
        for _ in 0..7 {
            mark.step(false, true);
        }
        assert_eq!(mark.left(), 0, "wrapped at width less side");
    }

    #[test]
    fn a_held_key_lifts_it_and_a_free_key_does_not() {
        let mut mark = Mark::new(64, 64);
        mark.step(true, true);
        assert_eq!(mark.top(), 12);
        mark.step(false, true);
        assert_eq!(mark.top(), 12);
    }

    #[test]
    fn a_still_mark_lifts_and_does_not_drift() {
        let mut mark = Mark::new(64, 64);
        mark.step(true, false);
        assert_eq!((mark.left(), mark.top()), (0, 12));
        mark.step(false, false);
        assert_eq!((mark.left(), mark.top()), (0, 12));
    }

    #[test]
    fn the_lift_wraps_past_the_top() {
        let mut mark = Mark::new(64, 64);
        for _ in 0..5 {
            mark.step(true, true);
        }
        assert_eq!(mark.top(), 28, "16, 12, 8, 4, 0, then round to the bottom");
    }

    #[test]
    fn a_screen_no_bigger_than_the_square_keeps_it_still() {
        let mut mark = Mark::new(SIDE, SIDE);
        mark.step(true, true);
        assert_eq!((mark.left(), mark.top()), (0, 0));
    }

    #[test]
    fn the_paint_covers_the_square_and_nothing_else() {
        let (width, height) = (64_usize, 64_usize);
        let mut bits = vec![9; width * height];
        let mark = Mark::new(64, 64);
        mark.paint(&mut bits);
        #[expect(clippy::naive_bytecount, reason = "a test, worth no new dependency")]
        let painted = bits.iter().filter(|&&b| b == SQUARE).count();
        assert_eq!(painted, (SIDE * SIDE) as usize);
        assert_eq!(bits[16 * width], SQUARE, "its top left corner");
        assert_eq!(bits[15 * width], GROUND, "the row above it");
        assert_eq!(
            bits[16 * width + 32],
            GROUND,
            "the pixel past its right edge"
        );
    }
}
