// SPDX-License-Identifier: MIT
// `node --test`: the fence on `gamepads.mjs`' contract. The frames are the ones
// `pads_tests.rs` hands to the crate, so the two halves read one recording.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import { padWords, startPolling } from "./gamepads.mjs";

const RECORDING = readFileSync(
  new URL("./recorded-pad.txt", import.meta.url),
  "utf8",
);

function numbers(text) {
  return text.split(",").filter(Boolean).map(Number);
}

// One line of the recording, as the pad it describes and the words it must give.
function frameFromLine(line) {
  const [name, mapping, count, axes, down, words] = line
    .split("|")
    .map((cell) => cell.trim());
  const pressed = new Set(numbers(down));
  const pad =
    mapping === "null"
      ? null
      : {
          mapping,
          connected: true,
          axes: numbers(axes),
          buttons: Array.from({ length: Number(count) }, (_, at) => ({
            pressed: pressed.has(at),
          })),
        };
  return { name, pad, words: numbers(words) };
}

const FRAMES = RECORDING.split("\n")
  .filter((line) => line.trim() !== "" && !line.startsWith("#"))
  .map(frameFromLine);

const REST = FRAMES.find((frame) => frame.name === "rest");
const PULLED = FRAMES.find((frame) => frame.name === "pulled");

function aLogger() {
  const lines = [];
  return { lines, info: (...values) => lines.push(values) };
}

// A window that throws at anything but what the script may touch: no element,
// no key listener and no focus can be reached through it.
function strictWindow(navigator) {
  const queue = [];
  const allowed = {
    navigator,
    requestAnimationFrame: (run) => queue.push(run),
    cancelAnimationFrame: () => queue.splice(0),
  };
  const window = new Proxy(allowed, {
    get(target, name) {
      if (name in target) return target[name];
      throw new Error(`the script touched window.${String(name)}`);
    },
    set(_, name) {
      throw new Error(`the script set window.${String(name)}`);
    },
  });
  const nextFrame = () => queue.shift()();
  return { window, nextFrame, waiting: () => queue.length };
}

test("the recording says it is a stand-in in its first line", () => {
  const first = RECORDING.split("\n")[0];
  assert.match(first, /A stand-in/);
  assert.match(first, /no device/);
});

for (const { name, pad, words } of FRAMES) {
  test(`recorded frame: ${name}`, () => {
    assert.deepEqual(padWords([pad]), words);
  });
}

test("every word a frame gives is a whole number a u32 holds", () => {
  for (const { pad } of FRAMES) {
    for (const word of padWords([pad])) {
      assert.ok(Number.isInteger(word) && word >= 0 && word <= 0xffffffff);
    }
  }
});

test("no pads, in every way a browser says so, is whole empty slots", () => {
  assert.deepEqual(padWords(undefined), []);
  assert.deepEqual(padWords(null), []);
  assert.deepEqual(padWords([]), []);
  assert.deepEqual(padWords([null, null]), new Array(22).fill(0));
  assert.deepEqual(padWords([{ ...REST.pad, connected: false }]), PULLED.words);
});

test("a pad's slot is kept, and only 4 are read", () => {
  const words = padWords([null, REST.pad]);
  assert.deepEqual(words.slice(0, 11), PULLED.words);
  assert.deepEqual(words.slice(11), REST.words);
  assert.equal(padWords(new Array(9).fill(REST.pad)).length, 44);
});

test("a stick with no number reads at rest", () => {
  const pad = { ...REST.pad, axes: [NaN, Infinity, undefined, -Infinity] };
  assert.deepEqual(padWords([pad]), REST.words);
});

test("a pad that lacks its axes or buttons reads at rest and does not throw", () => {
  const words = padWords([{ mapping: "standard", connected: true }]);
  assert.deepEqual(words.slice(0, 7), REST.words.slice(0, 7));
  assert.deepEqual(words.slice(8), [0, 0, 0xffff]);
});

test("getGamepads absent, or refused, is no pads and no fault", () => {
  const throws = {
    getGamepads() {
      throw new Error("not allowed here");
    },
  };
  for (const navigator of [undefined, {}, { getGamepads: () => null }, throws]) {
    const posted = [];
    const logger = aLogger();
    const { window, nextFrame } = strictWindow(navigator);
    startPolling(window, (words) => posted.push(words), logger);
    nextFrame();
    nextFrame();
    assert.deepEqual(posted, [[], []]);
    assert.equal(logger.lines.length, navigator === throws ? 1 : 0);
  }
});

test("a walk: pad, pulled, back; each frame posted, with nothing else touched", () => {
  const frames = [[REST.pad], [null], [REST.pad]];
  let at = 0;
  const navigator = new Proxy(
    { getGamepads: () => frames[at++] },
    {
      get(target, name) {
        if (name in target) return target[name];
        throw new Error(`the script touched navigator.${String(name)}`);
      },
    },
  );
  const posted = [];
  const logger = aLogger();
  const { window, nextFrame, waiting } = strictWindow(navigator);
  const stop = startPolling(window, (words) => posted.push(words), logger);
  assert.equal(posted.length, 0, "nothing runs before the first frame");
  nextFrame();
  nextFrame();
  nextFrame();
  assert.deepEqual(posted, [REST.words, PULLED.words, REST.words]);
  assert.deepEqual(logger.lines, []);
  assert.equal(waiting(), 1, "one frame asked for at a time");
  stop();
  assert.equal(waiting(), 0, "stop ends it");
});
