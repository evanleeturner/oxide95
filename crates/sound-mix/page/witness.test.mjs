// SPDX-License-Identifier: MIT
// `node --test`: the page's hash against the answer a second program gave, the same list
// and answer `src/tests/witness.rs` holds, so the 2 sides agree through it.
import assert from "node:assert/strict";
import test from "node:test";

import { HASH_START, hashSamples, hashText } from "./witness.mjs";

const FIXED_LIST = Int16Array.of(1, -1, 32767, -32768, 0, 256);
const FIXED_LIST_HASH = "5ba47c5c86047d13";

test("the hash of nothing is where a stream starts", () => {
  assert.equal(hashText(hashSamples(HASH_START, [])), "cbf29ce484222325");
});

test("the fixed list reads the second program's answer", () => {
  assert.equal(hashText(hashSamples(HASH_START, FIXED_LIST)), FIXED_LIST_HASH);
});

test("one stream cut three ways reads one hash", () => {
  for (const cut of [1, 2, 5]) {
    const first = hashSamples(HASH_START, FIXED_LIST.subarray(0, cut));
    const whole = hashSamples(first, FIXED_LIST.subarray(cut));
    assert.equal(hashText(whole), FIXED_LIST_HASH);
  }
});
