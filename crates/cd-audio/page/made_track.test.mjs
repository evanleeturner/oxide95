// SPDX-License-Identifier: MIT
// The made disc in script, under `node --test`: the hashes are a second program's, which
// reads neither this script nor the bench's Rust.
import assert from "node:assert/strict";
import { test } from "node:test";

import { runPieces, startPlayer, trackSource } from "./cd-player.mjs";
import {
  PAIRS_A_FRAME,
  keepingElement,
  madeDisc,
  recipe,
  trackBytes,
  trackHash,
} from "./made_track.mjs";

const PINNED = {
  2: "f52cc0b3a2f1531e",
  3: "f5ac31896d9bd7a3",
  4: "70f1d1611e3978a2",
  5: "6690511dd40bb8f3",
};
const WAV_HEADER_BYTES = 44;
const FRAME_BYTES = PAIRS_A_FRAME * 4;
const PLAY = 1;

const plainWindow = {
  Blob,
  URL,
  setTimeout: () => 1,
  clearTimeout: () => {},
  document: {
    made: [],
    createElement(name) {
      const element = {
        name,
        pause() {},
        play: () => Promise.resolve(),
      };
      this.made.push(element);
      return element;
    },
  },
};

// The first pairs of what the player would hand its element, with the WAV header off.
async function firstPairs(track, from, to, count) {
  const blob = trackSource(plainWindow, track, from, to);
  const bytes = await blob.slice(WAV_HEADER_BYTES).arrayBuffer();
  const view = new DataView(bytes);
  return Array.from({ length: count }, (_, n) => [
    view.getInt16(n * 4, true),
    view.getInt16(n * 4 + 2, true),
  ]);
}

const recipePairs = (number, frame, count) =>
  Array.from({ length: count }, (_, n) => recipe(number, frame * PAIRS_A_FRAME + n));

test("each track's whole hash is the second program's", () => {
  for (const [number, pinned] of Object.entries(PINNED)) {
    assert.equal(trackHash(Number(number)), pinned, `track ${number}`);
  }
});

test("track 2's file opens on its pregap of zeros, then the recipe from INDEX 01", () => {
  const bytes = trackBytes(2);
  assert.equal(bytes.length, (2 + 150) * FRAME_BYTES);
  assert.ok(bytes.subarray(0, 2 * FRAME_BYTES).every((byte) => byte === 0));
  const view = new DataView(bytes.buffer, 2 * FRAME_BYTES);
  const at = 300;
  assert.deepEqual(
    [view.getInt16(at * 4, true), view.getInt16(at * 4 + 2, true)],
    recipe(2, at),
  );
});

test("track 3's file is big-endian", () => {
  const view = new DataView(trackBytes(3).buffer);
  const at = 300;
  assert.deepEqual(
    [view.getInt16(at * 4, false), view.getInt16(at * 4 + 2, false)],
    recipe(3, at),
  );
});

test("the player's slice of a clean track is the recipe at the place asked for", async () => {
  const two = madeDisc(plainWindow)[0];
  assert.deepEqual(await firstPairs(two, 30, 120, 8), recipePairs(2, 30, 8));
});

test("the silent plant holds zeros, of the file's own length", async () => {
  const two = madeDisc(plainWindow, "silent")[0];
  assert.equal(two.file.size, trackBytes(2).length);
  const bytes = new Uint8Array(await two.file.arrayBuffer());
  assert.ok(bytes.every((byte) => byte === 0));
});

test("the track plant sounds as track 4 from track 2's slot", async () => {
  const two = madeDisc(plainWindow, "track")[0];
  assert.equal(two.number, 2);
  assert.deepEqual(await firstPairs(two, 30, 120, 8), recipePairs(4, 30, 8));
});

test("the place plant sounds 7 frames on", async () => {
  const two = madeDisc(plainWindow, "place")[0];
  assert.deepEqual(await firstPairs(two, 30, 120, 8), recipePairs(2, 37, 8));
});

test("the table plant ends track 2's run at 60", () => {
  const tracks = madeDisc(plainWindow, "table");
  const [piece] = runPieces(tracks, { track: 2, frame: 30 }, { track: 2, frame: 120 });
  assert.equal(piece.to, 60);
});

test("keepingElement hands back the very element startPlayer made", () => {
  const made = plainWindow.document.made;
  const before = made.length;
  const { kept, window } = keepingElement(plainWindow);
  assert.equal(kept.audio, null);
  const apply = startPlayer(window, madeDisc(plainWindow), { info() {} });
  assert.equal(made.length, before + 1);
  assert.equal(kept.audio, made[before]);
  apply([PLAY, 2, 30, 2, 120]);
  assert.match(kept.audio.src, /^blob:/);
  URL.revokeObjectURL(kept.audio.src);
});
