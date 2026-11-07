#!/usr/bin/env node
// SPDX-License-Identifier: MIT
// The worked script in a real browser, muted, one run a call: the wasm's own reads, then
// what `Page::read`, the canvas and a screenshot each hold against what the picture must
// be. It prints one JSON line a link and judges nothing; a separate script scores them.
// The page plays no sound and holds no element that could.
//
// Call: PLAYWRIGHT=<playwright's index.mjs> \
//   node surfaces-browser.mjs <wasm> <chromium|firefox> <DS1|DS2|DS3> <plant> <frames>
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { inflateSync } from "node:zlib";

const WIDTH = 320;
const HEIGHT = 200;
const CELLS = WIDTH * HEIGHT;
/// One `evaluate`'s frames, and the frame live memory is first read past.
const BATCH = 10000;
const FIRST_READ = 101;
const SETTLE_MS = 150;
const CHECKS = ["DS1", "DS2", "DS3"];
const PLANTS = ["none", "palette", "copy", "flip-stops", "leak"];
/// The wasm's `OUTSIDE_LINKS`, by index.
const SCREENSHOT = 2;
const MEMORY_BYTES = 3;
const PAGE = `<!doctype html><body style="margin:0;background:#000"><canvas id=c width=${WIDTH} height=${HEIGHT} style="display:block"></canvas>`;
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
const PNG_BYTE_DEPTH = 8;
const PNG_RGB = 2;
const PNG_RGBA = 6;

const { chromium, firefox } = await import(process.env.PLAYWRIGHT);
const TYPES = { chromium, firefox };

/** An 8-bit, non-interlaced PNG to its rows, by node alone: the browser never reads its own
 * picture here. Any other PNG is refused, never guessed at. */
function png(bytes) {
  let at = 8;
  let width, height, depth, kind, interlaced;
  const chunks = [];
  while (at < bytes.length) {
    const size = bytes.readUInt32BE(at);
    const type = bytes.toString("latin1", at + 4, at + 8);
    if (type === "IHDR") {
      width = bytes.readUInt32BE(at + 8);
      height = bytes.readUInt32BE(at + 12);
      depth = bytes[at + 16];
      kind = bytes[at + 17];
      interlaced = bytes[at + 20];
    }
    if (type === "IDAT") chunks.push(bytes.subarray(at + 8, at + 8 + size));
    at += size + 12;
  }
  if (depth !== PNG_BYTE_DEPTH || interlaced || (kind !== PNG_RGB && kind !== PNG_RGBA)) {
    throw new Error(`a PNG of depth ${depth}, kind ${kind}, interlaced ${interlaced}`);
  }
  const size = kind === PNG_RGBA ? 4 : 3;
  const line = width * size;
  const raw = inflateSync(Buffer.concat(chunks));
  const rows = Buffer.alloc(line * height);
  for (let y = 0; y < height; y++) {
    const filter = raw[y * (line + 1)];
    for (let x = 0; x < line; x++) {
      const value = raw[y * (line + 1) + 1 + x];
      const a = x >= size ? rows[y * line + x - size] : 0;
      const b = y ? rows[(y - 1) * line + x] : 0;
      const c = x >= size && y ? rows[(y - 1) * line + x - size] : 0;
      const p = a + b - c;
      const [pa, pb, pc] = [Math.abs(p - a), Math.abs(p - b), Math.abs(p - c)];
      const paeth = pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
      rows[y * line + x] = (value + [0, a, b, (a + b) >> 1, paeth][filter]) & 255;
    }
  }
  return { rows, width, height, size };
}

/** Pixels a screenshot holds other than RGBA `reference`, colour by colour. */
function differFromShot(shot, reference) {
  if (shot.width !== WIDTH || shot.height !== HEIGHT) return CELLS;
  let differ = 0;
  for (let cell = 0; cell < CELLS; cell++) {
    for (let channel = 0; channel < 3; channel++) {
      if (shot.rows[cell * shot.size + channel] !== reference[cell * 4 + channel]) {
        differ++;
        break;
      }
    }
  }
  return differ;
}

async function frames(page, total) {
  const started = Date.now();
  let done = Math.min(total, FIRST_READ);
  await page.evaluate((count) => window.e.frames(count), done);
  const first = await page.evaluate(() => window.e.memory.buffer.byteLength);
  while (done < total) {
    const batch = Math.min(total - done, BATCH);
    await page.evaluate((count) => window.e.frames(count), batch);
    done += batch;
  }
  const last = await page.evaluate(() => window.e.memory.buffer.byteLength);
  return { differ: Math.abs(last - first), of: first, ms: Date.now() - started };
}

async function main() {
  const [wasmPath, arm, check, plant, frameWord] = process.argv.slice(2);
  const total = Number(frameWord);
  const browser = await TYPES[arm].launch(ARMS[arm]);
  const context = await browser.newContext({
    viewport: { width: 640, height: 480 },
    deviceScaleFactor: 1,
  });
  const page = await context.newPage();
  await page.setContent(PAGE);
  await page.evaluate(
    async ({ wasm, check, plant, total }) => {
      const bytes = Uint8Array.from(atob(wasm), (c) => c.charCodeAt(0));
      const { instance } = await WebAssembly.instantiate(bytes, {});
      window.e = instance.exports;
      window.e.setup(check, plant, total);
    },
    {
      wasm: readFileSync(wasmPath).toString("base64"),
      check: CHECKS.indexOf(check),
      plant: PLANTS.indexOf(plant),
      total,
    },
  );
  const loopRead = check === "DS3" ? await frames(page, total) : null;
  const reference = await page.evaluate(
    ({ width, height, loopRead, memoryBytes }) => {
      const e = window.e;
      const cells = width * height;
      // `finish` reads the live bytes first, and a written line holds some.
      e.finish();
      if (loopRead) {
        e.line(memoryBytes, loopRead.differ, loopRead.of);
        e.rate(loopRead.ms);
      }
      const fresh = (at) => new Uint8ClampedArray(e.memory.buffer, at, cells * 4);
      const countDiffer = (one, other) => {
        let differ = 0;
        for (let cell = 0; cell < cells; cell++) {
          for (let channel = 0; channel < 4; channel++) {
            if (one[cell * 4 + channel] !== other[cell * 4 + channel]) {
              differ++;
              break;
            }
          }
        }
        return differ;
      };
      const got = e.read();
      const reference = new Uint8ClampedArray(fresh(e.reference_ptr()));
      const held = new Uint8ClampedArray(fresh(e.rgba_ptr()));
      e.line(0, got ? countDiffer(held, reference) : cells, cells);
      const context2d = document.getElementById("c").getContext("2d");
      context2d.putImageData(new ImageData(held, width, height), 0, 0);
      const back = context2d.getImageData(0, 0, width, height).data;
      e.line(1, countDiffer(back, reference), cells);
      return Array.from(reference);
    },
    { width: WIDTH, height: HEIGHT, loopRead, memoryBytes: MEMORY_BYTES },
  );
  await page.waitForTimeout(SETTLE_MS);
  const shot = png(await page.screenshot({ clip: { x: 0, y: 0, width: WIDTH, height: HEIGHT } }));
  const differ = differFromShot(shot, reference);
  const lines = await page.evaluate(
    ([link, differ, of]) => {
      const e = window.e;
      e.line(link, differ, of);
      return new TextDecoder().decode(new Uint8Array(e.memory.buffer, e.lines_ptr(), e.lines_len()));
    },
    [SCREENSHOT, differ, CELLS],
  );
  process.stdout.write(lines.replaceAll('"arm":"page"', `"arm":"${arm}"`));
  console.log("version", browser.version());
  await browser.close();
}

await main();
