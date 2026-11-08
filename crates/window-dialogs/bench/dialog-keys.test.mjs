// SPDX-License-Identifier: MIT
// The bench's screen gate: a key of X's goes to whatever holds the screen's focus,
// so the gate must refuse the desktop's own screens before any key is sent.
import assert from "node:assert/strict";
import { test } from "node:test";

import { screenStop } from "./dialog-keys.mjs";

test("the desktop's screens stop the keys", () => {
  assert.match(screenStop(["DISPLAY=:0"]), /real screen/);
  assert.match(screenStop(["DISPLAY=:1"]), /real screen/);
  assert.match(screenStop(["DISPLAY=:99", "DISPLAY=:0"]), /real screen/);
});

test("a Wayland name, a remote screen, or nothing read stops the keys", () => {
  assert.match(screenStop(["DISPLAY=:99", "WAYLAND_DISPLAY=wayland-0"]), /WAYLAND/);
  assert.match(screenStop(["DISPLAY=localhost:10"]), /not a local screen/);
  assert.match(screenStop(["DISPLAY=:0.0"]), /not a local screen/);
  assert.match(screenStop([]), /no browser environment/);
  assert.match(screenStop(["WAYLAND_DISPLAY="]), /WAYLAND/);
});

test("a private screen alone lets the keys go", () => {
  assert.equal(screenStop(["DISPLAY=:99"]), null);
  assert.equal(screenStop(["DISPLAY=:2"]), null);
});
