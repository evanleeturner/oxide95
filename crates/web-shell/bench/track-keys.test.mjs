// SPDX-License-Identifier: MIT
// The key bench's own walls, without a browser: the screen read stops on a real
// screen, each plant changes exactly one served file, and the bench's key list names
// every key the map holds and no other.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { test } from "node:test";

import { virtualKey } from "../page/keys.mjs";
import { KEY_ROWS, TEXT_PLANTS, screenStop, servedTexts } from "./track-keys.mjs";

const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

test("the screen read stops on a real screen and passes a private one", () => {
  assert.equal(screenStop(["DISPLAY=:0"]), "a real screen, DISPLAY=:0");
  assert.equal(screenStop(["DISPLAY=:1"]), "a real screen, DISPLAY=:1");
  assert.equal(screenStop(["DISPLAY=:2", "WAYLAND_DISPLAY=wayland-0"]), "WAYLAND_DISPLAY set");
  assert.equal(screenStop([]), "no browser environment read");
  assert.equal(screenStop(["DISPLAY=:2"]), null);
});

test("each plant changes exactly the one file it names", () => {
  const clean = servedTexts("none");
  for (const [plant, [file]] of Object.entries(TEXT_PLANTS)) {
    const planted = servedTexts(plant);
    for (const [name, bytes] of clean) {
      const same = sha256(bytes) === sha256(planted.get(name));
      assert.equal(same, name !== file, `${plant} on ${name}`);
    }
  }
});

test("the key list names every key of the map, once each", () => {
  const codes = KEY_ROWS.map(([code]) => code);
  assert.equal(new Set(codes).size, codes.length);
  for (const code of codes) assert.notEqual(virtualKey(code), null, code);
  const text = servedTexts("none").get("keys.mjs").toString("utf8");
  const mapped = [...text.matchAll(/^ {2}([A-Za-z0-9]+): 0x/gm)].map((m) => m[1]);
  assert.deepEqual([...codes].sort(), [...mapped].sort());
  assert.equal(virtualKey("Tab"), null);
  assert.equal(virtualKey("Backquote"), null);
});
