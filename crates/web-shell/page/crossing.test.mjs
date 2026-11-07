// SPDX-License-Identifier: MIT
// `node --test`: the fence on the ring's page side in `crossing.mjs`. `src/crossing.rs`'s
// tests hold the Worker's side and the two files' numbers.
import assert from "node:assert/strict";
import test from "node:test";

import {
  HEAD,
  LOST,
  RING,
  RING_SLOTS,
  TAIL,
  WORDS,
  pushEdge,
} from "./crossing.mjs";

const fresh = () =>
  new Int32Array(new SharedArrayBuffer(WORDS * Int32Array.BYTES_PER_ELEMENT));

test("edges go into the ring in the order the keys moved", () => {
  const words = fresh();
  assert.equal(pushEdge(words, 0x26, true), true);
  assert.equal(pushEdge(words, 0x41, true), true);
  assert.equal(pushEdge(words, 0x26, false), true);
  assert.deepEqual(
    Array.from(words.slice(RING, RING + 3)),
    [0x126, 0x141, 0x026],
  );
  assert.equal(words[HEAD], 3);
});

test("a full ring refuses an edge and counts it, keeping what it holds", () => {
  const words = fresh();
  for (let at = 0; at < RING_SLOTS; at += 1) pushEdge(words, 0x41, true);
  assert.equal(pushEdge(words, 0x42, true), false);
  assert.equal(words[LOST], 1);
  assert.equal(words[HEAD], RING_SLOTS);
  assert.equal(words[RING], 0x141);
});

test("a head past the top of its word still lands in the ring", () => {
  const words = fresh();
  words[HEAD] = -1;
  words[TAIL] = -1;
  assert.equal(pushEdge(words, 0x26, true), true);
  assert.equal(words[RING + RING_SLOTS - 1], 0x126);
  assert.equal(words[HEAD], 0);
});
