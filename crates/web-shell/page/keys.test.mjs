// SPDX-License-Identifier: MIT
// `node --test`: the fence on `keys.mjs`' contract: a key in the map gives its virtual
// key number, and anything else gives null.
import assert from "node:assert/strict";
import test from "node:test";

import { virtualKey } from "./keys.mjs";

test("a key gives its virtual key number", () => {
  assert.equal(virtualKey("KeyA"), 0x41);
  assert.equal(virtualKey("ArrowUp"), 0x26);
  assert.equal(virtualKey("Digit0"), 0x30);
});

test("letters and function keys run in their order", () => {
  for (let at = 0; at < 26; at += 1) {
    const letter = String.fromCharCode(0x41 + at);
    assert.equal(virtualKey(`Key${letter}`), 0x41 + at, letter);
  }
  for (let at = 0; at < 12; at += 1) {
    assert.equal(virtualKey(`F${at + 1}`), 0x70 + at, `F${at + 1}`);
  }
});

test("Tab is left out, so a keyboard can always leave the canvas", () => {
  assert.equal(virtualKey("Tab"), null);
});

test("a name the object inherits is not a key", () => {
  assert.equal(virtualKey("toString"), null);
});
