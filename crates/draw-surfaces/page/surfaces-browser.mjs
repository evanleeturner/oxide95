#!/usr/bin/env node
// SPDX-License-Identifier: MIT
// The worked script in a real browser, muted, one run a call: the wasm's own reads, then
// what `Page::read`, the canvas and a screenshot each hold against what the picture must
// be. It prints one JSON line a link and judges nothing; a separate script scores them.
// The page plays no sound and holds no element that could.
//
// A key check goes its own way: the browser's own key presses through a listener, the keys and
// the front, one JSON row a read, then a last `done` row. The rewrite of a key by a remap row
// stands in that listener alone, as no crate writes it yet.
//
// Call: PLAYWRIGHT=<playwright's index.mjs> \
//   node surfaces-browser.mjs <wasm> <chromium|firefox> <DS1|DS2|DS3> <plant> <frames>
//   node surfaces-browser.mjs <wasm> <chromium|firefox> <WD2|WD3|WD8> <plant>
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { inflateSync } from "node:zlib";

import { virtualKey } from "../../web-shell/page/keys.mjs";

const WIDTH = 320;
const HEIGHT = 200;
const CELLS = WIDTH * HEIGHT;
/// One `evaluate`'s frames, and the frame live memory is first read past.
const BATCH = 10000;
const FIRST_READ = 101;
const SETTLE_MS = 150;
const CHECKS = ["DS1", "DS2", "DS3"];
const PLANTS = ["none", "palette", "copy", "flip-stops", "leak"];
/// A key check's one plant, by check. These indexes are not the script's.
const KEY_PLANTS = { WD2: "map-keya-b", WD3: "active-unwired", WD8: "row-swapped" };
const PRESSED_CODES = ["KeyA", "Digit1", "ArrowUp", "ShiftLeft", "F1"];
const CHORDS = [
  ["Ctrl+E", "KeyE"],
  ["Ctrl+K", "KeyK"],
];
const CTRL_CODE = "ControlLeft";
const KEY_B = 0x42;
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

/** The wasm in the page with the listeners on it, and `window.rig` to read from outside. `map`
 * is the codes' numbers as node read them; `blurWired` and `rewrite` are the listeners' terms,
 * `rowPlant` the row the rewrite reads. */
async function keyRig(page, wasmPath, map, blurWired, rewrite, rowPlant) {
  await page.evaluate(
    async ({ wasm, map, blurWired, rewrite, rowPlant }) => {
      const KEY_COUNT = 256;
      const KEY_A = 0x41;
      const ASYNC_DOWN = 0x8000;
      const KEY_MASK = 0xff;
      const CTRL_MASK = 1 << 8;
      const ALT_MASK = 1 << 9;
      const bytes = Uint8Array.from(atob(wasm), (c) => c.charCodeAt(0));
      const { instance } = await WebAssembly.instantiate(bytes, {});
      const e = instance.exports;
      window.e = e;
      let seen = [];
      let handed = null;
      let chord = null;
      const listen = (event) => {
        event.preventDefault();
        const down = event.type === "keydown";
        if (down) seen.push({ type: event.type, code: event.code, isTrusted: event.isTrusted });
        let key = map[event.code];
        if (rewrite) {
          const packed = e.remap_read(rowPlant, key, Number(event.ctrlKey), Number(event.altKey));
          key = packed & KEY_MASK;
          if (down) {
            chord = { key, ctrl: (packed & CTRL_MASK) !== 0, alt: (packed & ALT_MASK) !== 0 };
          }
        }
        if (down) handed = key;
        e.key_hand(key, Number(down));
      };
      window.addEventListener("keydown", listen);
      window.addEventListener("keyup", listen);
      if (blurWired) window.addEventListener("blur", () => e.key_active(0));
      window.rig = {
        fresh: () => {
          seen = [];
        },
        read: () => {
          const down = [];
          const toggled = [];
          for (let key = 0; key < KEY_COUNT; key++) {
            if (e.key_answer(key) & ASYNC_DOWN) down.push(key);
          }
          for (let key = 0; key < KEY_COUNT; key++) {
            if (e.key_toggled(key)) toggled.push(key);
          }
          return { seen, handed, chord, down, toggled, a: down.includes(KEY_A) };
        },
        blurred: () => {
          window.dispatchEvent(new Event("blur"));
          e.key_pump();
          return window.rig.read();
        },
      };
    },
    { wasm: readFileSync(wasmPath).toString("base64"), map, blurWired, rewrite, rowPlant },
  );
}

/** One key check, one JSON row a read. A plant is this script's wrong number, its listener left
 * off, or the wasm's wrong row: none is in a crate's own code. */
async function keyCheck(page, wasmPath, check, plant) {
  if (plant !== "none" && plant !== KEY_PLANTS[check]) {
    throw new Error(`no plant ${plant} on ${check}`);
  }
  const codes = [...PRESSED_CODES, ...CHORDS.map(([, code]) => code), CTRL_CODE];
  const map = Object.fromEntries(codes.map((code) => [code, virtualKey(code)]));
  if (plant === "map-keya-b") map.KeyA = KEY_B;
  const blurWired = plant !== "active-unwired";
  const rewrite = check === "WD8";
  const rowPlant = plant === "row-swapped" ? 1 : 0;
  await keyRig(page, wasmPath, map, blurWired, rewrite, rowPlant);
  const read = () => page.evaluate(() => window.rig.read());
  const row = (fields) => console.log(JSON.stringify(fields));
  if (check === "WD2") {
    for (const code of PRESSED_CODES) {
      await page.evaluate(() => window.rig.fresh());
      await page.keyboard.down(code);
      const held = await read();
      await page.keyboard.up(code);
      const after = await read();
      const { seen, handed, down } = held;
      row({ row: "key", code, seen, handed, down, after: after.down });
    }
  } else if (check === "WD3") {
    await page.keyboard.down("KeyA");
    const held = await read();
    row({ row: "read", stage: "held", a: held.a, toggled: held.toggled });
    const blurred = await page.evaluate(() => window.rig.blurred());
    row({ row: "read", stage: "blurred", a: blurred.a, toggled: blurred.toggled });
    await page.keyboard.up("KeyA");
  } else {
    for (const [pressed, code] of CHORDS) {
      await page.keyboard.down(CTRL_CODE);
      await page.keyboard.down(code);
      const held = await read();
      row({ row: "chord", pressed, read: held.chord, down: held.down });
      await page.keyboard.up(code);
      await page.keyboard.up(CTRL_CODE);
    }
  }
  row({ row: "done" });
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
  if (Object.hasOwn(KEY_PLANTS, check)) {
    await keyCheck(page, wasmPath, check, plant);
    console.log("version", browser.version());
    await browser.close();
    return;
  }
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
