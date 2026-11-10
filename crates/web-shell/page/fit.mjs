// SPDX-License-Identifier: MIT
// The screen's whole-number fit: the largest whole factor at which the 640 by 480
// picture fits the space it is given, counted in the device's own pixels, so each
// picture pixel lands on a square block of whole device pixels. Pure: no DOM here.

export const PICTURE_WIDTH = 640;
export const PICTURE_HEIGHT = 480;

/**
 * The picture's fit in a space `width` by `height` CSS pixels at `ratio` device
 * pixels a CSS pixel. `k` is the largest whole factor with `k * 640` by `k * 480`
 * device pixels inside the space, and `width` by `height` the CSS size that shows
 * it. `k` is 0, and both sizes 0, when even the picture's own size does not fit or
 * the space or the ratio is not a finite positive number: the page then keeps its
 * plain shrink.
 */
export function wholeFit(width, height, ratio) {
  const k = Math.min(
    Math.floor((width * ratio) / PICTURE_WIDTH),
    Math.floor((height * ratio) / PICTURE_HEIGHT),
  );
  if (!Number.isFinite(k) || k < 1 || !(ratio > 0)) {
    return { k: 0, width: 0, height: 0 };
  }
  return {
    k,
    width: (PICTURE_WIDTH * k) / ratio,
    height: (PICTURE_HEIGHT * k) / ratio,
  };
}
