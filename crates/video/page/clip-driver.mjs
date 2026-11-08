// SPDX-License-Identifier: MIT
// Drives `clip_page`'s wasm in one browser engine and writes what came back as JSON: the
// page's answers, each frame's md5 taken here in Node, and stamps by a stopwatch outside the
// page. It judges nothing: `bench/page_score.py` reads this file. The page never hashes, and
// its clock is a wasm import over `performance.now`, so nothing it says about time is a judge.
//
//   PLAYWRIGHT=<its index.mjs> node clip-driver.mjs --engine chromium --wasm <file> \
//     --probes <folder> --cuts <folder> --paced <folder> --out <file>
//     [--arms seek,refuse,screen,pace] [--clock-scale 1.05] [--pace-clips a,b] [--runs 3]
import { createHash } from "node:crypto";
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { parseArgs } from "node:util";
import { inflateSync } from "node:zlib";

const PAGE = `<!doctype html><body style="margin:0;background:#000"><canvas id=c style="display:block"></canvas>`;
const CACHE = join(process.env.HOME, ".cache/ms-playwright");
const ARMS = {
  chromium: {
    executablePath: join(CACHE, "chromium-1223/chrome-linux64/chrome"),
    args: ["--mute-audio", "--no-sandbox"],
  },
  firefox: {
    executablePath: join(CACHE, "firefox-1522/firefox/firefox"),
    firefoxUserPrefs: { "media.volume_scale": "0" },
  },
};
/// `seek`'s clips: every palette entry painted and rewritten, and pixels that change.
const SEEK_CLIPS = ["palette", "blocks"];
const SCREEN_CLIP = "palette";
const PACE_CLIPS = ["long-6666", "long-7100"];
const RUNS_WANTED = 3;
const TRIES_A_RUN = 5;
/// Far under half the shortest period, so a blocked page shows as a long gap.
const BEAT_MS = 5;
const CLOCK_SPIN_MS = 300;
const SETTLE_MS = 50;
const STAMPS_DRAIN_MS = 200;

const { values: options } = parseArgs({
  options: {
    engine: { type: "string" },
    wasm: { type: "string" },
    probes: { type: "string" },
    cuts: { type: "string" },
    paced: { type: "string" },
    out: { type: "string" },
    arms: { type: "string", default: "seek,refuse,screen,pace" },
    "clock-scale": { type: "string", default: "1" },
    "pace-clips": { type: "string", default: PACE_CLIPS.join(",") },
    runs: { type: "string", default: String(RUNS_WANTED) },
  },
});

const md5 = (bytes) => createHash("md5").update(bytes).digest("hex");
const clip64 = (folder, name) => readFileSync(join(folder, `${name}.smk`)).toString("base64");

/** An 8-bit, non-interlaced PNG to RGBA rows, by Node alone: the browser never reads its own
 * picture here. Any other PNG is refused, never guessed at. */
function pngToRgba(bytes) {
  let at = 8;
  let width, height, channels;
  const chunks = [];
  while (at < bytes.length) {
    const size = bytes.readUInt32BE(at);
    const type = bytes.toString("latin1", at + 4, at + 8);
    if (type === "IHDR") {
      width = bytes.readUInt32BE(at + 8);
      height = bytes.readUInt32BE(at + 12);
      const [depth, kind, , , interlaced] = bytes.subarray(at + 16, at + 21);
      channels = { 2: 3, 6: 4 }[kind];
      if (depth !== 8 || !channels || interlaced !== 0) {
        throw new Error(`a PNG of depth ${depth} and kind ${kind}, interlace ${interlaced}`);
      }
    } else if (type === "IDAT") {
      chunks.push(bytes.subarray(at + 8, at + 8 + size));
    }
    at += 12 + size;
  }
  const raw = inflateSync(Buffer.concat(chunks));
  const rowBytes = width * channels;
  const rows = Buffer.alloc(height * rowBytes);
  const paeth = (a, b, c) => {
    const p = a + b - c;
    const [pa, pb, pc] = [Math.abs(p - a), Math.abs(p - b), Math.abs(p - c)];
    return pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
  };
  for (let y = 0; y < height; y++) {
    const filter = raw[y * (rowBytes + 1)];
    for (let x = 0; x < rowBytes; x++) {
      const value = raw[y * (rowBytes + 1) + 1 + x];
      const left = x >= channels ? rows[y * rowBytes + x - channels] : 0;
      const up = y > 0 ? rows[(y - 1) * rowBytes + x] : 0;
      const corner = y > 0 && x >= channels ? rows[(y - 1) * rowBytes + x - channels] : 0;
      const guess = [0, left, up, (left + up) >> 1, paeth(left, up, corner)][filter];
      rows[y * rowBytes + x] = (value + guess) & 0xff;
    }
  }
  if (channels === 4) return { width, height, rgba: rows };
  const rgba = Buffer.alloc(width * height * 4, 0xff);
  for (let cell = 0; cell < width * height; cell++) {
    rows.copy(rgba, cell * 4, cell * 3, cell * 3 + 3);
  }
  return { width, height, rgba };
}

/** A picture's clip part as rgb24 and its md5, what lies outside it, and its alpha. */
function readPicture(rgba, { width, height, x, y, clipWidth, clipHeight }) {
  const crop = Buffer.alloc(clipWidth * clipHeight * 3);
  const margin = new Set();
  let alphaOff = 0;
  for (let row = 0; row < height; row++) {
    for (let column = 0; column < width; column++) {
      const at = (row * width + column) * 4;
      if (rgba[at + 3] !== 255) alphaOff++;
      const inside = column >= x && column < x + clipWidth && row >= y && row < y + clipHeight;
      if (inside) {
        rgba.copy(crop, ((row - y) * clipWidth + (column - x)) * 3, at, at + 3);
      } else {
        margin.add(rgba.readUIntBE(at, 3));
      }
    }
  }
  return { md5: md5(crop), marginColours: margin.size, alphaOff };
}

/** Runs in the page: the helpers every arm uses. */
function pageHelpers({ wasm, scale }) {
  const bytes = Uint8Array.from(atob(wasm), (c) => c.charCodeAt(0));
  window.fresh = async () => {
    const imports = { page: { clock_us: () => performance.now() * 1000 * scale } };
    const { instance } = await WebAssembly.instantiate(bytes, imports);
    return instance.exports;
  };
  window.put = (e, clip) => {
    const data = Uint8Array.from(atob(clip), (c) => c.charCodeAt(0));
    const at = e.alloc(data.length);
    new Uint8Array(e.memory.buffer, at, data.length).set(data);
  };
  window.text = (e, at, length) =>
    new TextDecoder().decode(new Uint8Array(e.memory.buffer, at, length));
  window.said = (e) => window.text(e, e.said_ptr(), e.said_len());
  window.b64 = (e, at, length) => {
    const view = new Uint8Array(e.memory.buffer, at, length);
    let out = "";
    for (let from = 0; from < length; from += 0x8000) {
      out += String.fromCharCode.apply(null, view.subarray(from, from + 0x8000));
    }
    return btoa(out);
  };
}

/** Runs in the page: `seek`'s rows over one clip, a fresh instance a row, as a trap ends one. */
async function seekRows({ clip }) {
  const row = async (body) => {
    const e = await window.fresh();
    window.put(e, clip);
    const reads = [];
    const read = (frame) => {
      const answer = e.clip_decode();
      reads.push({
        frame,
        rgb: answer === 0 ? window.b64(e, e.rgb_ptr(), e.rgb_len()) : null,
        refused: answer === 0 ? null : window.said(e),
      });
    };
    try {
      if (e.clip_open() !== 0) return { refusedOpen: window.said(e), reads };
      const extra = body(e, e.clip_frames(), read);
      return { reads, frames: e.clip_frames(), ...extra };
    } catch (fault) {
      return { reads, trap: String(fault) };
    }
  };
  return {
    plain: await row((_, frames, read) => {
      for (let n = 0; n < frames; n++) read(n);
    }),
    // Each frame by a jump from a just-opened clip.
    forward: await row((e, frames, read) => {
      for (let n = 0; n < frames; n++) {
        e.clip_open();
        e.clip_seek(n);
        read(n);
      }
    }),
    rewind: await row((e, frames, read) => {
      for (let n = frames - 1; n >= 1; n--) {
        e.clip_seek(n);
        read(n);
      }
      read(2);
    }),
    own: await row((e, frames, read) => {
      for (let n = 0; n < frames; n++) {
        e.clip_seek(e.clip_cursor());
        read(n);
      }
    }),
    past: await row((e, frames, read) => {
      read(0);
      const before = e.clip_cursor();
      const answer = e.clip_seek(frames) === 0 ? "moved" : window.said(e);
      const after = e.clip_cursor();
      read(before);
      return { answer, before, after };
    }),
  };
}

/** Runs in the page: one clip's refusal in a new instance. The memory's size either side of
 * `parse` is read here, as a read of the instance's own buffer judges nothing but itself. */
async function refuseRow({ clip }) {
  const e = await window.fresh();
  e.logger_on();
  window.put(e, clip);
  const before = e.memory.buffer.byteLength;
  const out = { before, after: null, name: null, lines: "", trap: null };
  try {
    const parsed = e.clip_open();
    out.after = e.memory.buffer.byteLength;
    const answer = parsed === 0 ? e.read_rest() : 1;
    out.name = answer === 0 ? "read-whole" : window.said(e);
  } catch (fault) {
    out.trap = String(fault);
    out.after ??= e.memory.buffer.byteLength;
  }
  try {
    out.lines = window.text(e, e.lines_ptr(), e.lines_len());
  } catch (fault) {
    out.linesFault = String(fault);
  }
  return out;
}

/** Runs in the page: opens a clip for the screen. */
async function screenOpen({ clip }) {
  const e = await window.fresh();
  window.e = e;
  window.put(e, clip);
  if (e.screen_open() !== 0) return null;
  const canvas = document.getElementById("c");
  canvas.width = e.screen_width();
  canvas.height = e.screen_height();
  window.context2d = canvas.getContext("2d", { willReadFrequently: true });
  return {
    frames: e.screen_frames(),
    width: e.screen_width(),
    height: e.screen_height(),
    x: e.screen_clip_x(),
    y: e.screen_clip_y(),
    clipWidth: e.screen_clip_width(),
    clipHeight: e.screen_clip_height(),
    pitchMine: e.screen_pitch_mine(),
    pitchRead: e.screen_pitch_read(),
  };
}

/** Runs in the page: one frame through `Page::read`, then onto the canvas and back. */
function screenFrame() {
  const e = window.e;
  const read = e.screen_frame();
  const length = e.screen_rgba_len();
  const held = new Uint8ClampedArray(e.memory.buffer, e.screen_rgba_ptr(), length).slice();
  const [width, height] = [e.screen_width(), e.screen_height()];
  window.context2d.putImageData(new ImageData(held, width, height), 0, 0);
  const back = window.context2d.getImageData(0, 0, width, height).data;
  let canvas = "";
  for (let from = 0; from < back.length; from += 0x8000) {
    canvas += String.fromCharCode.apply(null, back.subarray(from, from + 0x8000));
  }
  return {
    read,
    rgba: window.b64(e, e.screen_rgba_ptr(), length),
    canvas: btoa(canvas),
    pixels: window.b64(e, e.screen_pixels_ptr(), e.screen_pixels_len()),
  };
}

/** Runs in the page: the loop a caller runs, one `smack_wait` a pass, a yield between. */
async function paceLoop({ clip, beatMs }) {
  const e = await window.fresh();
  window.put(e, clip);
  const beat = setInterval(() => window.stamp("beat", performance.now()), beatMs);
  if (e.pace_open() !== 0) {
    clearInterval(beat);
    return { refused: window.said(e) };
  }
  window.stamp("open", performance.now());
  const frames = e.pace_frames();
  return new Promise((resolve) => {
    const channel = new MessageChannel();
    let frame = 0;
    channel.port1.onmessage = () => {
      if (e.pace_wait() === 0) {
        window.stamp(
          "due",
          performance.now(),
          frame,
          e.pace_not_due(),
          e.pace_due_us() - e.pace_open_us(),
          e.pace_late_us(),
        );
        e.pace_step();
        frame += 1;
        if (frame === frames) {
          clearInterval(beat);
          channel.port1.close();
          resolve({
            frames,
            waits: e.pace_waits(),
            reanchors: e.pace_reanchors(),
            clockReads: e.pace_clock_reads(),
          });
          return;
        }
      }
      channel.port2.postMessage(0);
    };
    channel.port2.postMessage(0);
  });
}

/** Runs in the page: the clock's smallest step, over a spin. */
function clockStep(spinMs) {
  let least = Infinity;
  let last = performance.now();
  const end = last + spinMs;
  for (;;) {
    const now = performance.now();
    if (now > end) return least * 1000;
    if (now !== last) {
      least = Math.min(least, now - last);
      last = now;
    }
  }
}

async function freshPage(context, helpers) {
  const page = await context.newPage();
  await page.setContent(PAGE);
  await page.evaluate(pageHelpers, helpers);
  return page;
}

async function seekArm(context, helpers) {
  const page = await freshPage(context, helpers);
  const out = {};
  for (const name of SEEK_CLIPS) {
    const rows = await page.evaluate(seekRows, { clip: clip64(options.probes, name) });
    for (const row of Object.values(rows)) {
      for (const read of row.reads) {
        read.md5 = read.rgb === null ? null : md5(Buffer.from(read.rgb, "base64"));
        delete read.rgb;
      }
    }
    out[name] = rows;
  }
  await page.close();
  return out;
}

async function refuseArm(context, helpers) {
  const page = await freshPage(context, helpers);
  const out = {};
  const names = readdirSync(options.cuts).filter((file) => file.endsWith(".smk"));
  for (const file of names.sort()) {
    const name = file.slice(0, -".smk".length);
    out[name] = await page.evaluate(refuseRow, { clip: clip64(options.cuts, name) });
  }
  await page.close();
  return out;
}

async function screenArm(context, helpers) {
  const page = await freshPage(context, helpers);
  const opened = await page.evaluate(screenOpen, { clip: clip64(options.probes, SCREEN_CLIP) });
  if (opened === null) return { opened };
  const frames = [];
  for (let frame = 0; frame < opened.frames; frame++) {
    const got = await page.evaluate(screenFrame);
    await page.waitForTimeout(SETTLE_MS);
    const { width, height } = opened;
    const shot = pngToRgba(await page.screenshot({ clip: { x: 0, y: 0, width, height } }));
    const pixels = Buffer.from(got.pixels, "base64");
    let marginIndexOff = 0;
    pixels.forEach((index, at) => {
      const [row, column] = [Math.floor(at / opened.pitchMine), at % opened.pitchMine];
      const inside =
        column >= opened.x &&
        column < opened.x + opened.clipWidth &&
        row >= opened.y &&
        row < opened.y + opened.clipHeight;
      if (!inside && index !== 0) marginIndexOff++;
    });
    frames.push({
      read: got.read,
      marginIndexOff,
      rgba: readPicture(Buffer.from(got.rgba, "base64"), opened),
      canvas: readPicture(Buffer.from(got.canvas, "base64"), opened),
      shot:
        shot.width === width && shot.height === height
          ? readPicture(shot.rgba, opened)
          : { fault: `a shot of ${shot.width} by ${shot.height}` },
    });
  }
  await page.close();
  return { opened, frames };
}

async function paceRun(context, helpers, clip) {
  const page = await context.newPage();
  const stamps = [];
  await page.exposeBinding("stamp", (_source, ...args) => {
    stamps.push([process.hrtime.bigint(), ...args]);
  });
  await page.setContent(PAGE);
  await page.evaluate(pageHelpers, helpers);
  const end = await page.evaluate(paceLoop, { clip, beatMs: BEAT_MS });
  await page.waitForTimeout(STAMPS_DRAIN_MS);
  await page.close();
  const first = stamps[0]?.[0] ?? 0n;
  return {
    ...end,
    stamps: stamps.map(([at, ...rest]) => [Number(at - first) / 1000, ...rest]),
  };
}

/** Each clip until its clean runs are in, or a run's tries are out; every run is kept. */
async function paceArm(context, helpers) {
  const out = {};
  const page = await freshPage(context, helpers);
  out.clockStepUs = await page.evaluate(clockStep, CLOCK_SPIN_MS);
  await page.close();
  out.clips = {};
  for (const name of options["pace-clips"].split(",")) {
    const clip = clip64(options.paced, name);
    const runs = [];
    let clean = 0;
    let tries = 0;
    const wanted = Number(options.runs);
    while (clean < wanted && tries < wanted * TRIES_A_RUN) {
      const run = await paceRun(context, helpers, clip);
      runs.push(run);
      tries += 1;
      if (run.reanchors === 0) clean += 1;
    }
    out.clips[name] = runs;
  }
  return out;
}

const { [options.engine]: engineType } = await import(process.env.PLAYWRIGHT);
const wasmBytes = readFileSync(options.wasm);
const helpers = {
  wasm: wasmBytes.toString("base64"),
  scale: Number(options["clock-scale"]),
};
const browser = await engineType.launch(ARMS[options.engine]);
const context = await browser.newContext({
  viewport: { width: 640, height: 480 },
  deviceScaleFactor: 1,
});
const arms = options.arms.split(",");
const out = {
  engine: options.engine,
  version: browser.version(),
  wasm: options.wasm,
  wasmMd5: md5(wasmBytes),
  clockScale: helpers.scale,
};
if (arms.includes("seek")) out.seek = await seekArm(context, helpers);
if (arms.includes("refuse")) out.refuse = await refuseArm(context, helpers);
if (arms.includes("screen")) out.screen = await screenArm(context, helpers);
if (arms.includes("pace")) out.pace = await paceArm(context, helpers);
await browser.close();
writeFileSync(options.out, JSON.stringify(out));
console.log(options.engine, browser.version(), options.wasm, out.wasmMd5);
