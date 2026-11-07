// SPDX-License-Identifier: MIT
// The page's copy of `src/crossing.rs`'s numbers, and the ring's page side. A test
// there reads this file, so the two cannot part.
//
// Call: new Int32Array(new SharedArrayBuffer(WORDS * 4)), then pushEdge(words, key, down).
export const PAGE_TICKS = 0;
export const POLLS = 1;
export const POLLS_DOWN = 2;
export const FRAMES = 3;
export const DISC = 4;
export const READ_AGAIN = 5;
export const HEAD = 6;
export const TAIL = 7;
export const EDGES = 8;
export const LAST_KEY = 9;
export const YIELDED = 10;
export const LOST = 11;
export const SIT = 12;
export const MARK_LEFT = 13;
export const STILL = 14;
export const MARKER_AT = 16;
export const MARKERS = 5;
export const RING = 32;
export const RING_SLOTS = 256;
export const WORDS = RING + RING_SLOTS;

export const DISC_NONE = 0;
export const DISC_READ = 1;
export const DISC_MOVED = 2;
export const DISC_REFUSED = 3;

const DOWN_BIT = 0x100;

// One edge into the ring, in the order the keys moved. A full ring refuses it and
// counts it in LOST, rather than overwrite one the Worker has not read.
export function pushEdge(words, key, down) {
  const head = Atomics.load(words, HEAD);
  const waiting = (head - Atomics.load(words, TAIL)) | 0;
  if (waiting >= RING_SLOTS) {
    Atomics.add(words, LOST, 1);
    return false;
  }
  const slot = RING + ((head >>> 0) % RING_SLOTS);
  Atomics.store(words, slot, key | (down ? DOWN_BIT : 0));
  Atomics.store(words, HEAD, (head + 1) | 0);
  return true;
}
