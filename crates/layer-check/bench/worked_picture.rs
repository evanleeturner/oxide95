// SPDX-License-Identifier: MIT
//! The picture the computer's own checks show, and the words a wrong answer says: the
//! same as `page/picture.mjs` and `page/key.mjs`, so every arm is judged on one picture.
//! Shared by file, as the bench's programs are built one file at a time by bare `rustc`.

pub const WIDTH: u32 = 320;
pub const HEIGHT: u32 = 200;
/// Where the window sits, in both arms, so one reader takes both.
pub const LEFT: i32 = 40;
pub const TOP: i32 = 40;
pub const PICTURE_RED: &str =
    "A picture with no oxide95 code came back wrong. The computer or the browser, not the layer.";
pub const KEY_RED: &str =
    "A key with no oxide95 code never arrived. The browser's driver or the screen, not the layer.";

/// Every channel loses its low bits on a 5-6-5 screen, as pure colours would not.
const BANDS: [u32; 4] = [0x0013_375b, 0x008d_2ec1, 0x0045_d927, 0x00e9_6ba3];

/// The worked picture as 0RGB words by rows; its lower half turns the bands by one, so a
/// flip shows.
#[must_use]
pub fn picture() -> Vec<u32> {
    let (width, height) = (WIDTH as usize, HEIGHT as usize);
    let mut words = Vec::with_capacity(width * height);
    for y in 0..height {
        let turn = usize::from(y >= height / 2);
        for x in 0..width {
            words.push(BANDS[(x * BANDS.len() / width + turn) % BANDS.len()]);
        }
    }
    words
}
