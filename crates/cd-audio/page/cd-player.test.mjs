// SPDX-License-Identifier: MIT
// `node --test`: the fence on `cd-player.mjs`' contract. The window here is a fake
// that throws at anything the script must never do: read the element's time, listen,
// show a control, move a focus. A real `<audio>` in a real browser is not tested here.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import { runPieces, startPlayer, trackSource, wavHeader } from "./cd-player.mjs";

const PLAY = 1;
const PAUSE = 2;
const STOP = 3;
const VOLUME = 4;

function never(what) {
  return () => {
    throw new Error(`the script must not ${what}`);
  };
}

// A file whose bytes the script must never read itself: it only slices.
function fakeFile(name) {
  const slices = [];
  return {
    name,
    slices,
    slice(from, to) {
      slices.push([from, to]);
      return { sliceOf: name, from, to };
    },
    arrayBuffer: never("read a file"),
    text: never("read a file"),
    stream: never("read a file"),
  };
}

class FakeAudio {
  constructor(log) {
    this.log = log;
    this.volume = 1;
    this.played = 0;
    this.paused = 0;
    this.answer = () => Promise.resolve();
    this.seeks = [];
  }
  set src(value) {
    this.log.push(["src", value]);
  }
  set currentTime(value) {
    this.seeks.push(value);
  }
  get currentTime() {
    return never("read the element's time")();
  }
  get duration() {
    return never("read the element's length")();
  }
  get ended() {
    return never("read the element's state")();
  }
  set controls(_) {
    never("show a control")();
  }
  play() {
    this.played += 1;
    return this.answer();
  }
  pause() {
    this.paused += 1;
  }
  focus = never("move a focus");
  addEventListener = never("listen on the element");
  setAttribute = never("set an attribute on the element");
}

function fakeWindow() {
  const log = [];
  const elements = [];
  const timers = new Map();
  let nextTimer = 1;
  let nextUrl = 1;
  const urls = new Map();
  const revoked = [];
  const document = {
    createElement(tag) {
      assert.equal(tag, "audio");
      const element = new FakeAudio(log);
      elements.push(element);
      return element;
    },
    addEventListener: never("listen for a key"),
    get activeElement() {
      return never("read the focus")();
    },
    get body() {
      return never("join the document")();
    },
  };
  return {
    log,
    elements,
    timers,
    urls,
    revoked,
    document,
    addEventListener: never("listen for a key"),
    Blob: class {
      constructor(parts, options) {
        this.parts = parts;
        this.type = options.type;
      }
    },
    URL: {
      createObjectURL(source) {
        const url = `blob:${nextUrl++}`;
        urls.set(url, source);
        return url;
      },
      revokeObjectURL(url) {
        revoked.push(url);
      },
    },
    setTimeout(call, ms) {
      timers.set(nextTimer, { call, ms });
      return nextTimer++;
    },
    clearTimeout(id) {
      timers.delete(id);
    },
    // Fires the one timer running, as its time passing would.
    fire() {
      assert.equal(timers.size, 1, "one timer at a time");
      const [[id, { call }]] = timers;
      timers.delete(id);
      call();
    },
  };
}

function fakeLogger() {
  const lines = [];
  return { lines, info: (...values) => lines.push(values) };
}

function disc() {
  return [
    { number: 2, frames: 75, file: fakeFile("two.bin"), at: 4704, bigEndian: false },
    { number: 3, frames: 150, file: fakeFile("three.bin"), at: 0, bigEndian: true },
    { number: 4, frames: 300, file: fakeFile("four.ogg") },
    { number: 5, frames: 30, file: fakeFile("five.bin"), at: 0, bigEndian: false },
  ];
}

function setUp() {
  const window = fakeWindow();
  const tracks = disc();
  const logger = fakeLogger();
  const apply = startPlayer(window, tracks, logger);
  return { window, tracks, logger, apply, audio: window.elements[0] };
}

test("the header is 44 bytes, byte for byte, for 1 s of a disc's sound", () => {
  const riffSize = [0x34, 0xb1, 0x02, 0x00]; // 36 + 176400
  const dataSize = [0x10, 0xb1, 0x02, 0x00]; // 75 frames of 2352
  assert.deepEqual(
    Array.from(wavHeader(176400)),
    [
      0x52, 0x49, 0x46, 0x46, ...riffSize,
      0x57, 0x41, 0x56, 0x45,
      0x66, 0x6d, 0x74, 0x20,
      0x10, 0x00, 0x00, 0x00, // the format's own size
      0x01, 0x00, // plain PCM
      0x02, 0x00, // 2 channels
      0x44, 0xac, 0x00, 0x00, // 44100
      0x10, 0xb1, 0x02, 0x00, // 176400 bytes a second
      0x04, 0x00, // 4 bytes a pair
      0x10, 0x00, // 16 bits
      0x64, 0x61, 0x74, 0x61, ...dataSize,
    ],
  );
});

test("an image's track is a Blob of the header and the file's slice, by offsets, no read", () => {
  const window = fakeWindow();
  const [two] = disc();
  const blob = trackSource(window, two, 10, 30);
  assert.equal(blob.type, "audio/wav");
  assert.deepEqual(Array.from(blob.parts[0]), Array.from(wavHeader(20 * 2352)));
  assert.deepEqual(two.file.slices, [[4704 + 10 * 2352, 4704 + 30 * 2352]]);
  assert.deepEqual(blob.parts[1], {
    sliceOf: "two.bin",
    from: 4704 + 10 * 2352,
    to: 4704 + 30 * 2352,
  });
});

test("through a real Blob the track's slice follows the header, exactly", async () => {
  const bytes = Uint8Array.from({ length: 3 * 2352 }, (_, at) => at % 251);
  const track = { number: 2, frames: 2, file: new Blob([bytes]), at: 2352 };
  const blob = trackSource({ Blob }, track, 0, 2);
  const whole = new Uint8Array(await blob.arrayBuffer());
  assert.equal(whole.length, 44 + 2 * 2352);
  assert.deepEqual(whole.slice(0, 44), wavHeader(2 * 2352));
  assert.deepEqual(whole.slice(44), bytes.slice(2352));
});

test("a store's track is its file, started at the frame's time", () => {
  const { window, tracks, apply, audio } = setUp();
  apply([PLAY, 4, 150, 4, 300]);
  assert.equal(window.urls.get("blob:1"), tracks[2].file);
  assert.deepEqual(tracks[2].file.slices, []);
  assert.deepEqual(audio.seeks, [2]);
  assert.equal(audio.played, 1);
});

test("an image's track starts by its slice, so the element is never told a time", () => {
  const { tracks, apply, audio } = setUp();
  apply([PLAY, 2, 10, 2, 75]);
  assert.deepEqual(tracks[0].file.slices, [[4704 + 10 * 2352, 4704 + 75 * 2352]]);
  assert.deepEqual(audio.seeks, []);
});

test("one element however many plays, each play letting go of the last", () => {
  const { window, apply, audio } = setUp();
  apply([PLAY, 2, 0, 2, 75]);
  apply([PLAY, 5, 0, 5, 30]);
  apply([PLAY, 2, 5, 2, 75]);
  assert.equal(window.elements.length, 1);
  assert.equal(audio.played, 3);
  assert.deepEqual(window.revoked, ["blob:1", "blob:2"]);
  assert.equal(window.timers.size, 1);
});

test("a run crosses tracks on the frames' own time and ends at its end", () => {
  const { window, tracks, apply, audio } = setUp();
  apply([PLAY, 4, 225, 5, 15]);
  const [[, first]] = window.timers;
  assert.equal(first.ms, 1000);
  window.fire();
  const [[, second]] = window.timers;
  assert.equal(second.ms, 200);
  assert.deepEqual(tracks[3].file.slices, [[0, 15 * 2352]]);
  assert.equal(audio.played, 2);
  window.fire();
  assert.equal(window.timers.size, 0);
  assert.equal(audio.played, 2);
  assert.deepEqual(window.revoked, ["blob:1", "blob:2"]);
});

test("a piece ending a run stops at the run's frame, never past the track", () => {
  const tracks = disc();
  assert.deepEqual(
    runPieces(tracks, { track: 2, frame: 70 }, { track: 3, frame: 10 }).map((piece) => [
      piece.track.number,
      piece.from,
      piece.to,
    ]),
    [
      [2, 70, 75],
      [3, 0, 10],
    ],
  );
  assert.equal(runPieces(tracks, { track: 2, frame: 0 }, { track: 2, frame: 999 })[0].to, 75);
});

test("a track not on the disc is silence and no fault", () => {
  const { window, logger, apply, audio } = setUp();
  apply([PLAY, 9, 0, 9, 10]);
  apply([PLAY, 5, 0, 2, 10]);
  assert.equal(audio.played, 0);
  assert.equal(window.timers.size, 0);
  assert.deepEqual(logger.lines, []);
});

test("a pause and a stop end the sound and the timer", () => {
  for (const kind of [PAUSE, STOP]) {
    const { window, apply, audio } = setUp();
    apply([PLAY, 2, 0, 2, 75]);
    const before = audio.paused;
    apply([kind, 0, 0, 0, 0]);
    assert.equal(audio.paused, before + 1);
    assert.equal(window.timers.size, 0);
    assert.deepEqual(window.revoked, ["blob:1"]);
  }
});

test("a resume comes as a play from the clock's place", () => {
  const { tracks, apply, audio } = setUp();
  apply([PLAY, 2, 0, 2, 75]);
  apply([PAUSE, 0, 0, 0, 0]);
  apply([PLAY, 2, 30, 2, 75]);
  assert.equal(audio.played, 2);
  assert.deepEqual(tracks[0].file.slices[1], [4704 + 30 * 2352, 4704 + 75 * 2352]);
});

test("a big-endian track is one line and silence, and the run goes on", () => {
  const { window, logger, apply, audio } = setUp();
  apply([PLAY, 3, 0, 4, 75]);
  assert.equal(audio.played, 0);
  assert.deepEqual(logger.lines, [["cd track silent, big-endian", 3]]);
  const [[, first]] = window.timers;
  assert.equal(first.ms, 2000);
  window.fire();
  assert.equal(audio.played, 1);
  assert.equal(logger.lines.length, 1);
});

test("a refused play is one line and no fault, and the run goes on", async () => {
  const { window, logger, apply, audio } = setUp();
  audio.answer = () => Promise.reject(new Error("not allowed"));
  apply([PLAY, 2, 0, 5, 30]);
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(logger.lines, [["cd play refused", "Error: not allowed"]]);
  assert.equal(window.timers.size, 1);
});

test("a play that throws at once is the same one line", () => {
  const { window, logger, apply, audio } = setUp();
  audio.answer = () => {
    throw new Error("no such thing");
  };
  apply([PLAY, 2, 0, 2, 75]);
  assert.deepEqual(logger.lines, [["cd play refused", "Error: no such thing"]]);
  assert.equal(window.timers.size, 1);
});

test("a play cut off by the next order is no refusal", async () => {
  const { logger, apply, audio } = setUp();
  audio.answer = () => Promise.reject(Object.assign(new Error("cut"), { name: "AbortError" }));
  apply([PLAY, 2, 0, 2, 75]);
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(logger.lines, []);
});

test("a volume is the middle of the two sides", () => {
  const { apply, audio } = setUp();
  apply([VOLUME, 0xffff, 0xffff, 0, 0]);
  assert.equal(audio.volume, 1);
  apply([VOLUME, 0, 0, 0, 0]);
  assert.equal(audio.volume, 0);
  apply([VOLUME, 0xffff, 0, 0, 0]);
  assert.equal(audio.volume, 0.5);
});

test("an order it does not know is one line and no fault", () => {
  const { logger, apply } = setUp();
  apply([99, 0, 0, 0, 0]);
  assert.deepEqual(logger.lines, [["cd order unknown", 99]]);
});

test("the script's numbers are the ones page.rs posts", () => {
  const source = readFileSync(new URL("../src/page.rs", import.meta.url), "utf8");
  for (const [name, value] of [
    ["PLAY", PLAY],
    ["PAUSE", PAUSE],
    ["STOP", STOP],
    ["VOLUME", VOLUME],
  ]) {
    assert.match(source, new RegExp(`pub const ${name}: u32 = ${value};`));
  }
});
