// SPDX-License-Identifier: MIT
// `node --test`: the fence on `fit.mjs`' contract: the largest whole factor counted
// in device pixels, the CSS size that shows it, and 0 where even the picture's own
// size will not fit.
import assert from "node:assert/strict";
import test from "node:test";

import { wholeFit } from "./fit.mjs";

test("a 2560 by 1440 space at one device pixel shows the picture 3 times", () => {
  assert.deepEqual(wholeFit(2560, 1440, 1), {
    k: 3,
    width: 1920,
    height: 1440,
  });
});

test("the shorter side decides: 1920 by 1080 is 2 times, not 3", () => {
  assert.deepEqual(wholeFit(1920, 1080, 1), {
    k: 2,
    width: 1280,
    height: 960,
  });
});

test("the factor counts device pixels, so the CSS size is divided by the ratio", () => {
  assert.deepEqual(wholeFit(1280, 960, 2), { k: 4, width: 1280, height: 960 });
  assert.deepEqual(wholeFit(1280, 720, 1.5), {
    k: 2,
    width: 1280 / 1.5,
    height: 640,
  });
});

test("the picture's own size and the 800 by 700 view are 1 time", () => {
  assert.deepEqual(wholeFit(640, 480, 1), { k: 1, width: 640, height: 480 });
  assert.deepEqual(wholeFit(800, 700, 1), { k: 1, width: 640, height: 480 });
});

test("a space smaller than the picture on either side is 0, the plain shrink", () => {
  for (const [width, height] of [
    [639, 480],
    [640, 479],
    [320, 700],
  ]) {
    assert.equal(wholeFit(width, height, 1).k, 0, `${width} by ${height}`);
  }
});

test("a space or a ratio that is not a finite positive number is 0", () => {
  for (const [width, height, ratio] of [
    [0, 0, 1],
    [2560, 1440, 0],
    [Number.NaN, 1440, 1],
    [-2560, -1440, -1],
    [Infinity, Infinity, 1],
  ]) {
    assert.deepEqual(
      wholeFit(width, height, ratio),
      { k: 0, width: 0, height: 0 },
      `${width} ${height} ${ratio}`,
    );
  }
});
