// SPDX-License-Identifier: MIT
// The track in a real browser: every check the page can answer, read from the
// shared words, the console, screenshots and the sound server, and each one's planted
// wrong answer, applied to the served text so no shipped file holds a knob. Its own
// loopback server sends both cross-origin headers and is never shipped.
//
// Call: PLAYWRIGHT=<playwright's index.mjs> AXE=<axe.min.js> SCRATCH=<a folder>
//   [PLANT=<a name in PLANTS>] node track-browser.mjs <wasm> <output.json>
// A plant runs only its own check. Each check catches its own throw, so the output
// says which went red. Exit 3: another lane's sound run is live, so nothing ran.
// Exit 4: a stream of this run appeared, a stop, its index in the output.
import { execFileSync, spawnSync } from "node:child_process";
import {
  closeSync,
  ftruncateSync,
  mkdtempSync,
  openSync,
  readFileSync,
  renameSync,
  rmSync,
  writeFileSync,
  writeSync,
} from "node:fs";
import { createServer } from "node:http";
import { join } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { fileURLToPath } from "node:url";
import { inflateSync } from "node:zlib";

import {
  DISC,
  DISC_MOVED,
  DISC_NONE,
  DISC_READ,
  EDGES,
  FRAMES,
  LAST_KEY,
  MARKERS,
  MARKER_AT,
  MARK_LEFT,
  PAGE_TICKS,
  POLLS_DOWN,
  YIELDED,
} from "./crossing.mjs";

const PAGE_DIR = fileURLToPath(new URL(".", import.meta.url));
const TYPES = {
  html: "text/html; charset=utf-8",
  css: "text/css",
  mjs: "text/javascript",
};
const PAGE_FILES = [
  "track.html",
  "track.css",
  "track.mjs",
  "track-worker.mjs",
  "crossing.mjs",
  "keys.mjs",
  "track-logger.mjs",
];
const WASM_NAME = "page_track.wasm";
const VIEW = { width: 800, height: 700 };
const FLOOR = ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"];
const REFLOW_WIDTH = 320;
const MARK_PIXELS = 32 * 32;
const WHITE = 200;
const SHOTS = 5;
const SHOT_GAP_MS = 150;
const TAPS = 20;
const LEAST_POLL_SHARE = 0.9;
const KEY_A = 0x41;
const DOWN_BIT = 0x100;
const OTHER_RUNS = ["[f]h1_run.sh", "[l]ane_a_sound"];
const PICTURE_TRIES = 20;
const PICTURE_WAIT_MS = 8000;
const SETTLE_MS = 300;
const PRESS_MS = 200;
const STILL_READ_MS = 300;
const PICTURE_ASK =
  'const gl = canvas.getContext("webgl2", { alpha: false, antialias: false });';
const PICTURE_ASK_ONCE_REFUSED =
  'const gl = attempt === 1 ? null : canvas.getContext("webgl2", { alpha: false, antialias: false });';
const NO_PICTURE_WORDS =
  '"This browser would not give the track its WebGL 2 picture, so the track stopped. Close other tabs that draw pictures, then reload this page."';

const BLOCK = 2048;
const FILE_SECTOR = 19;
const FILE_BYTES = 300 << 20;
const MARKER_PLACES = [
  0,
  1 << 20,
  100 << 20,
  200 << 20,
  FILE_BYTES - (1 << 20),
];
const MARKER_BYTES = [0xa5, 0x5a, 0x3c, 0xc3, 0x7e];

const PLANTS = {
  "1-row": { check: "1", dropRow: "env.sit" },
  "2-blocked": {
    check: "2",
    swaps: [
      [
        "track.mjs",
        'const renderer = screen.getContext("bitmaprenderer");',
        "const planted = screen.transferControlToOffscreen();\nconst renderer = null;",
      ],
      [
        "track.mjs",
        "worker.postMessage(data);",
        'document.createElement("canvas").getContext("webgl2");\n  worker.postMessage({ ...data, planted }, [planted]);',
      ],
      [
        "track-worker.mjs",
        "onmessage = ({ data }) => {",
        "onmessage = ({ data }) => {\n  globalThis.planted = data.planted;",
      ],
      [
        "track-worker.mjs",
        "new OffscreenCanvas(WIDTH, HEIGHT)",
        "globalThis.planted",
      ],
      [
        "track-worker.mjs",
        "const bitmap = gl.canvas.transferToImageBitmap();",
        "gl.flush();",
      ],
      ["track-worker.mjs", "postMessage({ bitmap }, [bitmap]);", ""],
    ],
  },
  "2-yield": {
    check: "2",
    swaps: [
      [
        "track-worker.mjs",
        "sit: (millis) => Atomics.wait(words, SIT, 0, millis),",
        'sit: () => {\n      throw new Error("planted");\n    },',
      ],
    ],
  },
  "3-slow": {
    check: "3",
    swaps: [["track-worker.mjs", "const POLL_MS = 4;", "const POLL_MS = 40;"]],
  },
  "4-line": {
    check: "4",
    swaps: [
      [
        "track.mjs",
        "function pressed(place) {",
        'function pressed(place) {\n  console.info("a planted second line");',
      ],
    ],
  },
  "5-marker": { check: "5", expected: [0xa5, 0x5a, 0x00, 0xc3, 0x7e] },
  "6-sentence": {
    check: "6",
    swaps: [
      [
        "track.html",
        "This page cannot start from a file opened directly. Open it through its\n        local server instead.",
        "",
      ],
    ],
  },
  "6-moved": {
    check: "6",
    swaps: [
      [
        "track.mjs",
        '"The disc image could not be read. It may have been moved, renamed or changed since you picked it. Reload this page and pick it again."',
        '""',
      ],
    ],
  },
  "7-nameless": {
    check: "7",
    swaps: [["track.html", "disabled>Start</button>", "disabled></button>"]],
  },
  "9-key": {
    check: "9",
    swaps: [["keys.mjs", "KeyA: 0x41", "KeyA: 0x42"]],
  },
  "11-audio": {
    check: "11",
    swaps: [
      [
        "track.mjs",
        "const TICK_MS = 16;",
        "const TICK_MS = 16;\nnew AudioContext();",
      ],
    ],
  },
  // 12's two paths stay green, each reading its own numbers; its two plants go red.
  "12-once": {
    check: "12",
    swaps: [["track-worker.mjs", PICTURE_ASK, PICTURE_ASK_ONCE_REFUSED]],
  },
  "12-never": {
    check: "12",
    swaps: [["track-worker.mjs", PICTURE_ASK, "const gl = null;"]],
  },
  "12-silent": {
    check: "12",
    swaps: [
      ["track-worker.mjs", PICTURE_ASK, "const gl = null;"],
      ["track.mjs", NO_PICTURE_WORDS, '""'],
    ],
  },
  "12-unlogged": {
    check: "12",
    swaps: [
      ["track-worker.mjs", PICTURE_ASK, PICTURE_ASK_ONCE_REFUSED],
      ["track-worker.mjs", 'logger.warn("gl.refused", { attempt });', ""],
    ],
  },
  "13-pause": {
    check: "13",
    swaps: [["track.mjs", "Atomics.store(words, STILL, still ? 1 : 0);", ""]],
  },
  "13-reduced": {
    check: "13",
    swaps: [
      [
        "track.mjs",
        'window.matchMedia("(prefers-reduced-motion: reduce)").matches',
        "false",
      ],
    ],
  },
};

/** The page's files as served, a plant's swaps applied; a swap whose text is absent throws. */
function servedFiles(plant) {
  const files = new Map(
    PAGE_FILES.map((name) => [
      name,
      readFileSync(join(PAGE_DIR, name), "utf8"),
    ]),
  );
  for (const [name, from, to] of plant?.swaps ?? []) {
    const text = files.get(name);
    if (!text.includes(from))
      throw new Error(`the plant's text is absent: ${from}`);
    files.set(
      name,
      text.replace(from, () => to),
    );
  }
  return files;
}

async function serve(files, wasm) {
  const server = createServer((request, response) => {
    const plain = request.url.startsWith("/plain/");
    const name = new URL(request.url, "http://track").pathname
      .replace(/^\/plain\//, "/")
      .slice(1);
    const isWasm = name === WASM_NAME;
    if (!isWasm && !files.has(name)) {
      response.writeHead(404);
      response.end();
      return;
    }
    const headers = {
      "Content-Type": isWasm
        ? "application/wasm"
        : TYPES[name.split(".").pop()],
    };
    if (!plain) {
      headers["Cross-Origin-Opener-Policy"] = "same-origin";
      headers["Cross-Origin-Embedder-Policy"] = "require-corp";
    }
    response.writeHead(200, headers);
    response.end(isWasm ? wasm : files.get(name));
  });
  await new Promise((listening) => server.listen(0, "127.0.0.1", listening));
  return { server, base: `http://127.0.0.1:${server.address().port}` };
}

/** A directory record, as `logical_disc` reads one. */
function record(name, extent, size, folder) {
  const length = 33 + name.length + ((name.length + 1) % 2);
  const bytes = Buffer.alloc(length);
  bytes[0] = length;
  bytes.writeUInt32LE(extent, 2);
  bytes.writeUInt32BE(extent, 6);
  bytes.writeUInt32LE(size, 10);
  bytes.writeUInt32BE(size, 14);
  bytes[25] = folder ? 2 : 0;
  bytes[32] = name.length;
  name.copy(bytes, 33);
  return bytes;
}

/** A sparse iso holding one 300 MiB file with its markers planted: no game's bytes. */
function writeDisc(path) {
  const file = openSync(path, "w");
  try {
    ftruncateSync(file, FILE_SECTOR * BLOCK + FILE_BYTES);
    const put = (sector, at, bytes) =>
      writeSync(file, bytes, 0, bytes.length, sector * BLOCK + at);
    const root = record(Buffer.from([0]), 18, BLOCK, true);
    const sizeLow = Buffer.alloc(2);
    sizeLow.writeUInt16LE(BLOCK);
    const sizeHigh = Buffer.alloc(2);
    sizeHigh.writeUInt16BE(BLOCK);
    put(16, 0, Buffer.from([1]));
    put(16, 1, Buffer.from("CD001"));
    put(16, 6, Buffer.from([1]));
    put(16, 40, Buffer.alloc(32, 0x20));
    put(16, 128, sizeLow);
    put(16, 130, sizeHigh);
    put(16, 156, root);
    put(17, 0, Buffer.from([255]));
    put(17, 1, Buffer.from("CD001"));
    put(17, 6, Buffer.from([1]));
    put(
      18,
      0,
      Buffer.concat([
        root,
        record(Buffer.from([1]), 18, BLOCK, true),
        record(Buffer.from("MARKERS.BIN;1"), FILE_SECTOR, FILE_BYTES, false),
      ]),
    );
    MARKER_PLACES.forEach((place, index) => {
      put(FILE_SECTOR, place, Buffer.from([MARKER_BYTES[index]]));
    });
  } finally {
    closeSync(file);
  }
}

/** The filter byte's prediction from a pixel's left, upper and upper-left neighbours. */
function predicted(filter, left, up, upLeft) {
  if (filter === 1) return left;
  if (filter === 2) return up;
  if (filter === 3) return (left + up) >> 1;
  if (filter !== 4) return 0;
  const guess = left + up - upLeft;
  const fromLeft = Math.abs(guess - left);
  const fromUp = Math.abs(guess - up);
  const fromUpLeft = Math.abs(guess - upLeft);
  if (fromLeft <= fromUp && fromLeft <= fromUpLeft) return left;
  return fromUp <= fromUpLeft ? up : upLeft;
}

/** A screenshot's pixels: 8-bit RGB or RGBA, not interlaced, as a browser writes one. */
function decodePng(buffer) {
  let at = 8;
  let width = 0;
  let height = 0;
  let colour = 0;
  const parts = [];
  while (at < buffer.length) {
    const size = buffer.readUInt32BE(at);
    const type = buffer.toString("latin1", at + 4, at + 8);
    if (type === "IHDR") {
      width = buffer.readUInt32BE(at + 8);
      height = buffer.readUInt32BE(at + 12);
      colour = buffer[at + 17];
    }
    if (type === "IDAT") parts.push(buffer.subarray(at + 8, at + 8 + size));
    at += 12 + size;
  }
  const packed = inflateSync(Buffer.concat(parts));
  const channels = colour === 6 ? 4 : 3;
  const stride = width * channels;
  const pixels = Buffer.alloc(stride * height);
  for (let row = 0; row < height; row += 1) {
    const filter = packed[row * (stride + 1)];
    for (let byte = 0; byte < stride; byte += 1) {
      const left =
        byte >= channels ? pixels[row * stride + byte - channels] : 0;
      const up = row > 0 ? pixels[(row - 1) * stride + byte] : 0;
      const upLeft =
        byte >= channels && row > 0
          ? pixels[(row - 1) * stride + byte - channels]
          : 0;
      const value = packed[row * (stride + 1) + 1 + byte];
      pixels[row * stride + byte] =
        (value + predicted(filter, left, up, upLeft)) & 255;
    }
  }
  return { width, height, channels, pixels };
}

/** The white square in a screenshot: its count of pixels and its least column and row. */
function findMark(buffer) {
  const { width, height, channels, pixels } = decodePng(buffer);
  let left = -1;
  let top = -1;
  let count = 0;
  for (let row = 0; row < height; row += 1) {
    for (let column = 0; column < width; column += 1) {
      const at = (row * width + column) * channels;
      if (
        pixels[at] > WHITE &&
        pixels[at + 1] > WHITE &&
        pixels[at + 2] > WHITE
      ) {
        if (left < 0 || column < left) left = column;
        if (top < 0) top = row;
        count += 1;
      }
    }
  }
  return { left, top, count };
}

function readStreams() {
  const found = [];
  for (const kind of ["sink-inputs", "source-outputs"]) {
    const text = execFileSync(
      "env",
      ["-u", "PULSE_SERVER", "pactl", "list", "short", kind],
      { encoding: "utf8", timeout: 2000, stdio: ["ignore", "pipe", "ignore"] },
    );
    for (const line of text.split("\n")) {
      const index = line.split("\t")[0];
      if (index) found.push({ kind, index });
    }
  }
  return found;
}

/** The program behind a stream, by the sound server's long listing. */
function streamProgram({ kind, index }) {
  const text = execFileSync(
    "env",
    ["-u", "PULSE_SERVER", "pactl", "list", kind],
    { encoding: "utf8", timeout: 2000, stdio: ["ignore", "pipe", "ignore"] },
  );
  const block = text
    .split(/\n(?=\S)/)
    .find((part) => part.includes(`#${index}\n`));
  return (
    /application\.process\.binary = "([^"]*)"/.exec(block ?? "")?.[1] ??
    "unknown"
  );
}

/** The sound server read every second across the run, so a stream cannot come and go unseen. */
function watchStreams() {
  const key = (stream) => `${stream.kind}#${stream.index}`;
  const baseline = new Set(readStreams().map(key));
  const watch = {
    baseline: [...baseline],
    reads: 0,
    failed: 0,
    others: [],
    ours: [],
  };
  const seen = new Set(baseline);
  const timer = setInterval(() => {
    try {
      const streams = readStreams();
      watch.reads += 1;
      for (const stream of streams.filter((one) => !seen.has(key(one)))) {
        seen.add(key(stream));
        const program = streamProgram(stream);
        const row = { stream: key(stream), program };
        const browserOwned = /chrom|headless|unknown/i.test(program);
        (browserOwned ? watch.ours : watch.others).push(row);
      }
    } catch {
      watch.failed += 1;
    }
  }, 1000);
  return { watch, stop: () => clearInterval(timer) };
}

/** Counts Worker posts for check 3 and audio contexts for check 11; the stub opens no real stream. */
function counting() {
  window.__counts = { audio: 0, posts: 0 };
  class CountedAudio {
    constructor() {
      window.__counts.audio += 1;
    }
  }
  window.AudioContext = CountedAudio;
  window.webkitAudioContext = CountedAudio;
  const post = Worker.prototype.postMessage;
  Worker.prototype.postMessage = function counted(...values) {
    window.__counts.posts += 1;
    return post.apply(this, values);
  };
}

async function open(run, address = "/track.html", view = VIEW) {
  const context = await run.browser.newContext({ viewport: view });
  context.setDefaultTimeout(20000);
  await context.addInitScript(counting);
  const page = await context.newPage();
  const lines = [];
  page.on("console", (message) => {
    lines.push({ type: message.type(), text: message.text() });
  });
  page.on("pageerror", (error) => run.reads.pageErrors.push(error.message));
  const url = address.startsWith("file:") ? address : `${run.base}${address}`;
  await page.goto(url);
  if (!address.startsWith("file:")) {
    await page.waitForSelector("html[data-ready]", { state: "attached" });
  }
  const first = run.reads.firstAsk === null;
  return { context, page, lines, first };
}

async function close(run, session) {
  const { context, page, lines, first } = session;
  if (first) {
    run.reads.firstAsk = lines.filter((line) =>
      line.text.startsWith("gl.refused"),
    ).length;
  }
  run.reads.errorLines.push(
    ...lines
      .filter((line) =>
        /^(track\.(wasm|stopped|panic)|gl\.gave)/.test(line.text),
      )
      .map((line) => line.text),
  );
  run.audio += await page.evaluate(() => window.__counts.audio);
  await Promise.race([context.close(), delay(10000)]);
}

async function during(run, address, view, work) {
  const session = await open(run, address, view);
  try {
    return await work(session);
  } finally {
    await close(run, session);
  }
}

const wordsOf = (page) =>
  page.evaluate(() => Array.from(window.trackPage.words.slice(0, 32)));

async function waitWord(page, word, test, millis) {
  const end = Date.now() + millis;
  let value = (await wordsOf(page))[word];
  while (!test(value) && Date.now() < end) {
    await delay(5);
    value = (await wordsOf(page))[word];
  }
  return value;
}

const start = (page) => page.click(".js-start");
const sentenceOf = (page) => page.locator(".js-sentence").innerText();
/** The canvas's whole pixels alone: a box at a fractional place rounds a row of the page in. */
async function shoot(page) {
  const box = await page.locator(".js-screen").boundingBox();
  const x = Math.ceil(box.x);
  const y = Math.ceil(box.y);
  const width = Math.floor(box.x + box.width) - x;
  const height = Math.floor(box.y + box.height) - y;
  return findMark(await page.screenshot({ clip: { x, y, width, height } }));
}
const onScreen = (page) =>
  page.evaluate(
    () => document.activeElement?.classList.contains("js-screen") === true,
  );

function check1(run) {
  const module = new WebAssembly.Module(run.wasm);
  const imports = WebAssembly.Module.imports(module).map(
    (one) => `${one.module}.${one.name}`,
  );
  let lines = run.crateHead.split("\n");
  if (run.plant?.dropRow) {
    const row = `//! | \`${run.plant.dropRow}\` |`;
    if (!lines.some((line) => line.startsWith(row))) {
      throw new Error(`the plant's row is absent: ${row}`);
    }
    lines = lines.filter((line) => !line.startsWith(row));
  }
  const reasons = new Map();
  for (const line of lines) {
    const found = /^\/\/! \| `(env\.[a-z_]+)` \| (.*?) \|$/.exec(line);
    if (found) reasons.set(found[1], found[2].trim());
  }
  const unlisted = imports.filter((name) => !reasons.has(name));
  const stale = [...reasons.keys()].filter((name) => !imports.includes(name));
  const noReason = [...reasons.entries()]
    .filter(([, reason]) => reason === "")
    .map(([name]) => name);
  return {
    pass: unlisted.length + stale.length + noReason.length === 0,
    imports,
    unlisted,
    stale,
    noReason,
  };
}

function check2(run) {
  return during(run, "/track.html", VIEW, async ({ page }) => {
    await start(page);
    const reached = await waitWord(page, FRAMES, (count) => count >= 5, 8000);
    const before = await wordsOf(page);
    const marks = [];
    for (let shot = 0; shot < SHOTS; shot += 1) {
      marks.push(await shoot(page));
      await delay(SHOT_GAP_MS);
    }
    const after = await wordsOf(page);
    const whole = (mark) => mark.count === MARK_PIXELS;
    const seen = marks.filter(whole).length;
    const moves = marks
      .slice(1)
      .filter(
        (mark, at) =>
          whole(mark) && whole(marks[at]) && mark.left !== marks[at].left,
      ).length;
    const framesMoved = after[FRAMES] - before[FRAMES];
    const ticksMoved = after[PAGE_TICKS] - before[PAGE_TICKS];
    return {
      pass:
        reached >= 5 &&
        framesMoved > 0 &&
        ticksMoved > 0 &&
        after[YIELDED] === 0 &&
        seen === SHOTS &&
        moves === SHOTS - 1,
      framesMoved,
      ticksMoved,
      yielded: after[YIELDED],
      seen,
      moves,
      marks,
    };
  });
}

function check3(run) {
  return during(run, "/track.html", VIEW, async ({ page }) => {
    await start(page);
    await waitWord(page, FRAMES, (count) => count >= 5, 8000);
    const postsBefore = await page.evaluate(() => window.__counts.posts);
    const still = [await shoot(page), await shoot(page)];
    await page.keyboard.down("ArrowUp");
    await waitWord(page, POLLS_DOWN, (count) => count > 0, 3000);
    const lifted = await shoot(page);
    const before = await wordsOf(page);
    await delay(1000);
    const after = await wordsOf(page);
    const held = await shoot(page);
    await page.keyboard.up("ArrowUp");
    const postsAfter = await page.evaluate(() => window.__counts.posts);
    const ticks = after[PAGE_TICKS] - before[PAGE_TICKS];
    const polls = after[POLLS_DOWN] - before[POLLS_DOWN];
    const share = polls / ticks;
    const stayedStill = still[0].top === still[1].top;
    const moved = still[1].top !== lifted.top || still[1].top !== held.top;
    return {
      pass:
        share >= LEAST_POLL_SHARE &&
        postsAfter === postsBefore &&
        stayedStill &&
        moved,
      polls,
      ticks,
      share,
      postsBefore,
      postsAfter,
      tops: [still[0].top, still[1].top, lifted.top, held.top],
    };
  });
}

/** One walk of the page's presses: how many INFO lines each act gave, within 200 ms. */
function pressWalk(run, query) {
  return during(run, `/track.html${query}`, VIEW, async ({ page, lines }) => {
    const infoLines = async (act) => {
      const from = lines.length;
      await act();
      await delay(200);
      return lines.slice(from).filter((line) => line.type === "info").length;
    };
    const counts = {
      file: await infoLines(() => page.setInputFiles(".js-file", run.disc)),
      start: await infoLines(() => start(page)),
      readAgain: await infoLines(() => page.click(".js-read-again")),
      heldKey: await infoLines(async () => {
        await page.keyboard.down("KeyA");
        await delay(100);
        await page.keyboard.up("KeyA");
      }),
    };
    await delay(100);
    const impCalls = lines.filter((line) =>
      line.text.startsWith("imp.call"),
    ).length;
    const ready = lines.find((line) => line.text.startsWith("gl.ready"));
    if (ready) run.reads.renderer = ready.text;
    return { counts, impCalls };
  });
}

async function check4(run) {
  const plain = await pressWalk(run, "");
  const debug = await pressWalk(run, "?log=debug");
  const right = ({ counts }) =>
    counts.file === 1 &&
    counts.start === 1 &&
    counts.readAgain === 1 &&
    counts.heldKey === 2;
  return {
    pass:
      right(plain) &&
      right(debug) &&
      plain.impCalls === 0 &&
      debug.impCalls > 0,
    plain,
    debug,
  };
}

function check5(run) {
  return during(run, "/track.html", VIEW, async ({ page }) => {
    await page.setInputFiles(".js-file", run.disc);
    await start(page);
    const disc = await waitWord(
      page,
      DISC,
      (value) => value !== DISC_NONE,
      30000,
    );
    const words = await wordsOf(page);
    const read = words.slice(MARKER_AT, MARKER_AT + MARKERS);
    const expected = run.plant?.expected ?? MARKER_BYTES;
    return {
      pass:
        disc === DISC_READ && read.every((byte, at) => byte === expected[at]),
      disc,
      read,
      expected,
    };
  });
}

/** A visible sentence holding `words`, the state a player would be left reading. */
async function saysIt(page, words) {
  const visible = await page.locator(".js-sentence").isVisible();
  const text = visible ? await sentenceOf(page) : "";
  return { visible, text, says: visible && text.includes(words) };
}

async function check6(run) {
  const direct = await during(
    run,
    `file://${run.folder}/track.html`,
    VIEW,
    async ({ page }) => {
      await delay(500);
      const ready = await page.evaluate(
        () => document.documentElement.dataset.ready,
      );
      return {
        ...(await saysIt(page, "local server")),
        ranModule: ready !== undefined,
      };
    },
  );
  const plain = await during(run, "/plain/track.html", VIEW, ({ page }) =>
    saysIt(page, "local server"),
  );
  const moved = await during(run, "/track.html", VIEW, async ({ page }) => {
    await page.setInputFiles(".js-file", run.disc);
    await start(page);
    await waitWord(page, DISC, (value) => value !== DISC_NONE, 30000);
    renameSync(run.disc, `${run.disc}.moved`);
    try {
      await page.click(".js-read-again");
      const disc = await waitWord(
        page,
        DISC,
        (value) => value === DISC_MOVED,
        8000,
      );
      await delay(100);
      return { disc, ...(await saysIt(page, "moved")) };
    } finally {
      renameSync(`${run.disc}.moved`, run.disc);
    }
  });
  return {
    pass:
      direct.says &&
      !direct.ranModule &&
      plain.says &&
      moved.says &&
      moved.disc === DISC_MOVED,
    direct,
    plain,
    moved,
  };
}

async function axeNames(page) {
  await page.addScriptTag({ path: process.env.AXE });
  const found = await page.evaluate(
    (values) => window.axe.run(document, { runOnly: { type: "tag", values } }),
    FLOOR,
  );
  return found.violations.map((one) => one.id);
}

async function check7(run) {
  const axe = {};
  const walk = await during(run, "/track.html", VIEW, async ({ page }) => {
    axe.loaded = await axeNames(page);
    await page.keyboard.press("Tab");
    const chooserOpens = page.waitForEvent("filechooser", { timeout: 5000 });
    await page.keyboard.press("Space");
    const chooser = await chooserOpens;
    await chooser.setFiles(run.disc);
    axe.picked = await axeNames(page);
    await page.keyboard.press("Tab");
    await page.keyboard.press("Enter");
    const afterStart = await onScreen(page);
    await waitWord(page, DISC, (value) => value !== DISC_NONE, 30000);
    axe.running = await axeNames(page);
    await page.keyboard.press("Shift+Tab");
    await page.keyboard.press("Space");
    const afterReadAgain = await onScreen(page);
    await page.keyboard.press("Tab");
    const tabLeaves = !(await onScreen(page));
    await page.focus(".js-screen");
    renameSync(run.disc, `${run.disc}.moved`);
    try {
      await page.click(".js-read-again");
      await waitWord(page, DISC, (value) => value === DISC_MOVED, 8000);
      axe.moved = await axeNames(page);
    } finally {
      renameSync(`${run.disc}.moved`, run.disc);
    }
    return { afterStart, afterReadAgain, tabLeaves };
  });
  axe.direct = await during(
    run,
    `file://${run.folder}/track.html`,
    VIEW,
    ({ page }) => axeNames(page),
  );
  axe.plain = await during(run, "/plain/track.html", VIEW, ({ page }) =>
    axeNames(page),
  );
  const narrow = { width: REFLOW_WIDTH, height: 700 };
  const scrollWidth = await during(
    run,
    "/track.html",
    narrow,
    async ({ page }) => {
      await start(page);
      return page.evaluate(() => document.documentElement.scrollWidth);
    },
  );
  const violations = Object.values(axe).flat().length;
  return {
    pass:
      violations === 0 &&
      walk.afterStart &&
      walk.afterReadAgain &&
      walk.tabLeaves &&
      scrollWidth <= REFLOW_WIDTH,
    axe,
    walk,
    scrollWidth,
    claims:
      "the floor only: a person's keyboard and screen-reader walk is owed",
  };
}

function check9(run) {
  return during(run, "/track.html", VIEW, async ({ page }) => {
    await start(page);
    await waitWord(page, FRAMES, (count) => count >= 1, 8000);
    const wantDown = KEY_A | DOWN_BIT;
    await page.keyboard.down("KeyA");
    const down = await waitWord(
      page,
      LAST_KEY,
      (value) => value === wantDown,
      2000,
    );
    await page.keyboard.up("KeyA");
    const up = await waitWord(page, LAST_KEY, (value) => value === KEY_A, 2000);
    return { pass: down === wantDown && up === KEY_A, down, up };
  });
}

async function check11(run) {
  await during(run, "/track.html", VIEW, async ({ page }) => {
    await start(page);
    await delay(1500);
  });
  await delay(1100);
  const { reads, failed, ours, others, baseline } = run.streams;
  return {
    pass: run.audio === 0 && ours.length === 0 && reads > 0 && failed === 0,
    audio: run.audio,
    reads,
    failed,
    ours,
    others,
    baseline,
  };
}

/** The picture's ask at start, every try a line: what the console and the words say of it. */
function check12(run) {
  return during(run, "/track.html?log=debug", VIEW, async ({ page, lines }) => {
    await start(page);
    const settled = () =>
      lines.some((line) => /^gl\.(ready|gave\.up)/.test(line.text));
    const end = Date.now() + PICTURE_WAIT_MS;
    while (!settled() && Date.now() < end) await delay(5);
    const named = (type, event) =>
      lines.filter((line) => line.type === type && line.text.startsWith(event));
    const ready = lines.find((line) => line.text.startsWith("gl.ready"));
    const attempt = ready
      ? Number(/attempt: (\d+)/.exec(ready.text)?.[1])
      : null;
    const refused = named("warning", "gl.refused").length;
    const gaveUp = named("error", "gl.gave.up").length;
    const frames = ready
      ? await waitWord(page, FRAMES, (count) => count > 0, PICTURE_WAIT_MS)
      : (await wordsOf(page))[FRAMES];
    await delay(SETTLE_MS);
    const sentence = await sentenceOf(page);
    const worked = gaveUp === 0 && attempt === refused + 1 && frames > 0;
    const stopped =
      !ready &&
      gaveUp === 1 &&
      refused === PICTURE_TRIES &&
      frames === 0 &&
      sentence.includes("would not give");
    return {
      pass: worked || stopped,
      refused,
      attempt,
      gaveUp,
      frames,
      sentence,
    };
  });
}

/** The pause by keys alone from the canvas, and the start under reduced motion. */
async function check13(run) {
  const walk = await during(
    run,
    "/track.html",
    VIEW,
    async ({ page, lines }) => {
      await start(page);
      await waitWord(page, FRAMES, (count) => count >= 5, PICTURE_WAIT_MS);
      // Shift is a key of the map, so its own lines are read off before the press.
      const press = async () => {
        await page.keyboard.press("Shift+Tab");
        const onPlace = await page.evaluate(
          () => document.activeElement?.classList.contains("js-pause") === true,
        );
        await delay(SETTLE_MS);
        const from = lines.length;
        await page.keyboard.press("Space");
        await delay(PRESS_MS);
        const infoLines = lines
          .slice(from)
          .filter((line) => line.type === "info").length;
        return { onPlace, infoLines, focusBack: await onScreen(page) };
      };
      const paused = await press();
      const before = await wordsOf(page);
      await delay(STILL_READ_MS);
      const after = await wordsOf(page);
      const sentence = await sentenceOf(page);
      const moving = await press();
      await delay(STILL_READ_MS);
      const again = await wordsOf(page);
      return {
        presses: [paused, moving],
        stillHeld:
          after[MARK_LEFT] === before[MARK_LEFT] &&
          after[FRAMES] > before[FRAMES],
        movesAgain: again[MARK_LEFT] !== after[MARK_LEFT],
        left: [before[MARK_LEFT], after[MARK_LEFT], again[MARK_LEFT]],
        sentence,
      };
    },
  );
  const reduced = await during(run, "/track.html", VIEW, async ({ page }) => {
    await page.emulateMedia({ reducedMotion: "reduce" });
    await start(page);
    await waitWord(page, FRAMES, (count) => count >= 5, PICTURE_WAIT_MS);
    const words = await wordsOf(page);
    return {
      reducedLeft: words[MARK_LEFT],
      reducedFrames: words[FRAMES],
      reducedSentence: await sentenceOf(page),
    };
  });
  const pressesRight = walk.presses.every(
    ({ onPlace, infoLines, focusBack }) =>
      onPlace && infoLines === 1 && focusBack,
  );
  return {
    pass:
      pressesRight &&
      walk.stillHeld &&
      walk.movesAgain &&
      reduced.reducedLeft === 0 &&
      reduced.reducedFrames >= 5,
    ...walk,
    ...reduced,
  };
}

const CHECKS = {
  1: check1,
  2: check2,
  3: check3,
  4: check4,
  5: check5,
  6: check6,
  7: check7,
  9: check9,
  11: check11,
  12: check12,
  13: check13,
};

/** Tap shorter than a poll, both ways: what reached `hand` and what a poll saw, of 20. */
function readTaps(run) {
  return during(run, "/track.html", VIEW, async ({ page }) => {
    await start(page);
    await waitWord(page, FRAMES, (count) => count >= 5, 8000);
    const tally = async (tap) => {
      let edges = 0;
      let polls = 0;
      for (let at = 0; at < TAPS; at += 1) {
        const before = await wordsOf(page);
        await tap();
        await waitWord(page, EDGES, (count) => count >= before[EDGES] + 2, 500);
        await delay(10);
        const after = await wordsOf(page);
        if (after[EDGES] - before[EDGES] === 2) edges += 1;
        if (after[POLLS_DOWN] > before[POLLS_DOWN]) polls += 1;
      }
      return { of: TAPS, edgesReachedHand: edges, pollsReadItDown: polls };
    };
    const pressed = await tally(() => page.keyboard.press("ArrowUp"));
    const oneTask = await tally(() =>
      page.evaluate(() => {
        const screen = document.querySelector(".js-screen");
        for (const kind of ["keydown", "keyup"]) {
          const options = { code: "ArrowUp", bubbles: true, cancelable: true };
          screen.dispatchEvent(new KeyboardEvent(kind, options));
        }
      }),
    );
    return { pressed, oneTask };
  });
}

/** Whether pictures back up behind a busy page: frames counted less frames shown. */
function readBitmaps(run) {
  return during(run, "/track.html", VIEW, async ({ page }) => {
    await start(page);
    await waitWord(page, FRAMES, (count) => count >= 5, 8000);
    const behind = () =>
      page.evaluate(() => {
        const { words, shown } = window.trackPage;
        return Atomics.load(words, 3) - shown();
      });
    const idleGaps = [];
    for (let second = 0; second < 5; second += 1) {
      await delay(1000);
      idleGaps.push(await behind());
    }
    const busyGaps = [];
    for (let loop = 0; loop < 5; loop += 1) {
      busyGaps.push(
        await page.evaluate(() => {
          const until = performance.now() + 500;
          while (performance.now() < until);
          const { words, shown } = window.trackPage;
          return Atomics.load(words, 3) - shown();
        }),
      );
    }
    await delay(300);
    const after = await behind();
    return { idleGaps, busyGaps, afterMs300: after };
  });
}

function otherRunsLive() {
  return OTHER_RUNS.filter(
    (pattern) => spawnSync("pgrep", ["-f", pattern]).status === 0,
  );
}

async function main() {
  const [wasmPath, outputPath] = process.argv.slice(2);
  const plantName = process.env.PLANT ?? "";
  const plant = plantName ? PLANTS[plantName] : null;
  if (plantName && !plant) throw new Error(`no plant ${plantName}`);
  const live = otherRunsLive();
  if (live.length > 0) {
    process.stderr.write(`another lane's sound run is live: ${live}\n`);
    process.exit(3);
  }

  const files = servedFiles(plant);
  const scratch = mkdtempSync(join(process.env.SCRATCH, "track-"));
  for (const [name, text] of files) writeFileSync(join(scratch, name), text);
  const disc = join(scratch, "track.iso");
  writeDisc(disc);
  const wasm = readFileSync(wasmPath);
  const { server, base } = await serve(files, wasm);
  const { watch, stop } = watchStreams();
  const { chromium } = await import(process.env.PLAYWRIGHT);
  const browser = await chromium.launch({ args: ["--mute-audio"] });
  const run = {
    browser,
    base,
    plant,
    wasm,
    disc,
    folder: scratch,
    crateHead: readFileSync(join(PAGE_DIR, "..", "src", "lib.rs"), "utf8"),
    streams: watch,
    audio: 0,
    reads: { firstAsk: null, renderer: null, pageErrors: [], errorLines: [] },
  };
  const answers = {};
  const output = {
    chromium: browser.version(),
    plant: plantName,
    answers,
    reads: run.reads,
  };
  let stopped = false;
  try {
    const names = plant ? [plant.check] : Object.keys(CHECKS);
    for (const name of names) {
      try {
        answers[name] = await CHECKS[name](run);
      } catch (error) {
        answers[name] = { pass: false, threw: error.message };
      }
      if (watch.ours.length > 0) {
        output.stopped = watch.ours;
        stopped = true;
        break;
      }
    }
    if (!plant && !stopped) {
      run.reads.taps = await readTaps(run);
      run.reads.bitmaps = await readBitmaps(run);
    }
  } finally {
    stop();
    await Promise.race([browser.close(), delay(10000)]);
    server.close();
    rmSync(scratch, { recursive: true, force: true });
    writeFileSync(outputPath, `${JSON.stringify(output, null, 1)}\n`);
  }
  process.exit(stopped ? 4 : 0);
}

await main();
