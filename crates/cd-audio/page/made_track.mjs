// SPDX-License-Identifier: MIT
// A made disc for the page's CD player: tones from a formula, in whole numbers, so this
// script, the bench's Rust and a second program that reads neither agree without a shared
// sine. Not part of the player: only the bench and its tests import this.
//
// A place is told by its sound. Each track has its own odd-prime periods, and the height
// moves on a 64-frame stair, so the tones repeat only past any track's end.
//
// `n` counts pairs from a track's INDEX 01, so a pregap misread shows as a wrong place.
//
// Hazard: `keepingElement` hands the player a window of the five things it uses. A player
// that starts using a sixth fails here first, by design: that is the moment to read it.

export const PAIRS_A_FRAME = 588;

const BYTES_A_SAMPLE = 2;
const SAMPLES_A_PAIR = 2;
const FRAME_BYTES = PAIRS_A_FRAME * SAMPLES_A_PAIR * BYTES_A_SAMPLE;
const LEVEL_BASE = 6000;
const LEVEL_STAIRS = 64;
const LEVEL_STEP = 64;
const PLANT_FRAMES_ON = 7;
const PLANT_FRAMES_CLAIMED = 60;
const HASH_START = 0xcbf29ce484222325n;
const HASH_PRIME = 0x00000100000001b3n;
const HASH_BITS = 64;
const HEX_DIGITS = 16;

const TRACKS = [
  { number: 2, frames: 150, left: 101, right: 73, pregap: 2, bigEndian: false },
  { number: 3, frames: 150, left: 89, right: 67, pregap: 0, bigEndian: true },
  { number: 4, frames: 225, left: 79, right: 61, pregap: 0, bigEndian: false },
  { number: 5, frames: 75, left: 71, right: 59, pregap: 0, bigEndian: false },
];

const trackOf = (number) => TRACKS.find((track) => track.number === number);

function tri(n, period, level) {
  const q = Math.floor((4 * (n % period) * level) / period);
  if (q < level) return q;
  if (q < 3 * level) return 2 * level - q;
  return q - 4 * level;
}

const levelAt = (frame) => LEVEL_BASE + (frame % LEVEL_STAIRS) * LEVEL_STEP;

// Pair `n` of a track, counted from its INDEX 01, as `[left, right]`.
export function recipe(number, n) {
  const track = trackOf(number);
  const level = levelAt(Math.floor(n / PAIRS_A_FRAME));
  return [tri(n, track.left, level), tri(n, track.right, level)];
}

// FNV-1a 64 over a whole track's samples, little-endian, whatever its file's order.
export function trackHash(number) {
  const pairs = trackOf(number).frames * PAIRS_A_FRAME;
  let value = HASH_START;
  for (let n = 0; n < pairs; n += 1) {
    for (const sample of recipe(number, n)) {
      for (const byte of [sample & 0xff, (sample >> 8) & 0xff]) {
        value = BigInt.asUintN(HASH_BITS, (value ^ BigInt(byte)) * HASH_PRIME);
      }
    }
  }
  return value.toString(HEX_DIGITS).padStart(HEX_DIGITS, "0");
}

// A track's file: its pregap of zeros, then its sound, in the order the file claims.
export function trackBytes(number) {
  const track = trackOf(number);
  const pairs = track.frames * PAIRS_A_FRAME;
  const startByte = track.pregap * FRAME_BYTES;
  const view = new DataView(new ArrayBuffer(startByte + track.frames * FRAME_BYTES));
  for (let n = 0; n < pairs; n += 1) {
    const [left, right] = recipe(number, n);
    const at = startByte + n * SAMPLES_A_PAIR * BYTES_A_SAMPLE;
    view.setInt16(at, left, !track.bigEndian);
    view.setInt16(at + BYTES_A_SAMPLE, right, !track.bigEndian);
  }
  return new Uint8Array(view.buffer);
}

// The disc as `startPlayer` takes it, its files `Blob`s, a plant lying in one track's row:
//   silent: every file holds zeros, of its own length
//   track:  track 2's slot holds track 4's bytes
//   place:  track 2's `at` is 7 frames on
//   table:  track 2 claims 60 frames
export function madeDisc(window, plant = "clean") {
  const tracks = TRACKS.map((track) => {
    const bytes = trackBytes(track.number);
    return {
      number: track.number,
      frames: track.frames,
      file: new window.Blob([plant === "silent" ? new Uint8Array(bytes.length) : bytes]),
      at: track.pregap * FRAME_BYTES,
      bigEndian: track.bigEndian,
    };
  });
  const [two, , four] = tracks;
  if (plant === "track") {
    two.file = four.file;
    two.at = four.at;
  } else if (plant === "place") {
    two.at += PLANT_FRAMES_ON * FRAME_BYTES;
  } else if (plant === "table") {
    two.frames = PLANT_FRAMES_CLAIMED;
  }
  return tracks;
}

// The one way past `cd-player.mjs`'s wall, which it stays behind untouched: a window whose
// `createElement` keeps the element the player makes, so a bench can tap its sound.
export function keepingElement(window) {
  const kept = { audio: null };
  return {
    kept,
    window: {
      document: {
        createElement(name) {
          kept.audio = window.document.createElement(name);
          return kept.audio;
        },
      },
      Blob: window.Blob,
      URL: window.URL,
      setTimeout: window.setTimeout.bind(window),
      clearTimeout: window.clearTimeout.bind(window),
    },
  };
}
