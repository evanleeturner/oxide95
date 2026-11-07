#!/usr/bin/env node
// SPDX-License-Identifier: MIT
// A picture in real browsers with none of the other crates' code: bytes put on a bare
// canvas, read back from it, and read again from the browser's own screenshot. A wrong
// answer here clears the layer and names the computer or the browser.
//
// It prints what each arm read, and a separate script scores the output. A plant is a
// browser setting or a script run before the page's, so no plant ships. The screenshot is
// the browser's word for itself: nothing here reads a display, and nobody looked.
//
// Call: PLAYWRIGHT=<playwright's index.mjs> node picture.mjs <out file>
import { writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { inflateSync } from "node:zlib";

import { launch, playwrightFromEnv } from "./browser.mjs";

export const WIDTH = 320;
export const HEIGHT = 200;
/** Every channel loses its low bits on a 5-6-5 screen, as pure colours would not. */
export const BANDS = [
  [0x13, 0x37, 0x5b],
  [0x8d, 0x2e, 0xc1],
  [0x45, 0xd9, 0x27],
  [0xe9, 0x6b, 0xa3],
];
export const RED =
  "A picture with no oxide95 code came back wrong. The computer or the browser, not the layer.";

const SETTLE_MS = 150;
const VIEW = { width: 640, height: 480 };
const OPAQUE = 255;
const PNG_SIGNATURE_BYTES = 8;
const PNG_CHUNK_OVERHEAD = 12;
const BARE = (style) =>
  `<!doctype html><style>${style}</style><body style="margin:0;background:#000"><canvas id="c" width="${WIDTH}" height="${HEIGHT}" style="display:block"></canvas>`;
const ONE_BYTE_LIE = `(() => {
  const real = CanvasRenderingContext2D.prototype.getImageData;
  CanvasRenderingContext2D.prototype.getImageData = function (...values) {
    const got = real.apply(this, values);
    got.data[0] ^= 1;
    return got;
  };
})();`;

const ARMS = [
  { name: "clean", browsers: ["chromium", "firefox"] },
  {
    name: "plant-rfp",
    browsers: ["firefox"],
    prefs: { "privacy.resistFingerprinting": true },
  },
  { name: "plant-canvas", browsers: ["chromium"], init: ONE_BYTE_LIE },
  {
    name: "plant-shot",
    browsers: ["chromium", "firefox"],
    style: "canvas { filter: invert(1); }",
  },
  {
    name: "read-profile",
    browsers: ["chromium"],
    args: ["--force-color-profile=display-p3-d65"],
  },
];

/** The worked picture, RGBA by rows; its lower half turns the bands by one, so a flip shows. */
export function pictureBytes() {
  const bytes = Buffer.alloc(WIDTH * HEIGHT * 4);
  for (let y = 0; y < HEIGHT; y += 1) {
    const turn = y >= HEIGHT / 2 ? 1 : 0;
    for (let x = 0; x < WIDTH; x += 1) {
      const band = BANDS[(Math.floor((x * BANDS.length) / WIDTH) + turn) % 4];
      bytes.set([...band, OPAQUE], (y * WIDTH + x) * 4);
    }
  }
  return bytes;
}

function paeth(left, up, corner) {
  const guess = left + up - corner;
  const toLeft = Math.abs(guess - left);
  const toUp = Math.abs(guess - up);
  const toCorner = Math.abs(guess - corner);
  if (toLeft <= toUp && toLeft <= toCorner) return left;
  return toUp <= toCorner ? up : corner;
}

/** An 8-bit, non-interlaced RGB or RGBA PNG to its rows, by node alone; throws on any other. */
export function decodePng(bytes) {
  let at = PNG_SIGNATURE_BYTES;
  let head = null;
  const parts = [];
  while (at < bytes.length) {
    const size = bytes.readUInt32BE(at);
    const type = bytes.toString("latin1", at + 4, at + 8);
    if (type === "IHDR") {
      head = {
        width: bytes.readUInt32BE(at + 8),
        height: bytes.readUInt32BE(at + 12),
        depth: bytes[at + 16],
        kind: bytes[at + 17],
        interlace: bytes[at + 20],
      };
    }
    if (type === "IDAT") parts.push(bytes.subarray(at + 8, at + 8 + size));
    at += size + PNG_CHUNK_OVERHEAD;
  }
  if (
    !head ||
    head.depth !== 8 ||
    ![2, 6].includes(head.kind) ||
    head.interlace !== 0
  ) {
    throw new Error(`a PNG this reader does not take: ${JSON.stringify(head)}`);
  }
  const step = head.kind === 6 ? 4 : 3;
  const line = head.width * step;
  const raw = inflateSync(Buffer.concat(parts));
  const rows = Buffer.alloc(line * head.height);
  for (let y = 0; y < head.height; y += 1) {
    const filter = raw[y * (line + 1)];
    for (let x = 0; x < line; x += 1) {
      const left = x >= step ? rows[y * line + x - step] : 0;
      const up = y ? rows[(y - 1) * line + x] : 0;
      const corner = x >= step && y ? rows[(y - 1) * line + x - step] : 0;
      const add = [0, left, up, (left + up) >> 1, paeth(left, up, corner)][
        filter
      ];
      rows[y * line + x] = (raw[y * (line + 1) + 1 + x] + add) & 255;
    }
  }
  return { width: head.width, height: head.height, step, rows };
}

/** How many of the wanted RGBA bytes differ; `step` 3 reads colour alone, as a PNG may drop alpha. */
export function differingBytes(wanted, got, step) {
  let differ = 0;
  const pixels = wanted.length / 4;
  for (let pixel = 0; pixel < pixels; pixel += 1) {
    for (let channel = 0; channel < Math.min(step, 3); channel += 1) {
      if (wanted[pixel * 4 + channel] !== got[pixel * step + channel])
        differ += 1;
    }
    if (step === 4 && wanted[pixel * 4 + 3] !== got[pixel * 4 + 3]) differ += 1;
  }
  return differ;
}

function paintAndRead(page, picture) {
  return page.evaluate(
    ([text, width, height]) => {
      const bytes = Uint8ClampedArray.from(atob(text), (c) => c.charCodeAt(0));
      const context = document.querySelector("#c").getContext("2d");
      context.putImageData(new ImageData(bytes, width, height), 0, 0);
      const back = context.getImageData(0, 0, width, height).data;
      let held = "";
      for (let at = 0; at < back.length; at += 8192) {
        held += String.fromCharCode(...back.subarray(at, at + 8192));
      }
      return btoa(held);
    },
    [picture.toString("base64"), WIDTH, HEIGHT],
  );
}

async function runArm(playwright, kind, arm, picture) {
  const browser = await launch(playwright, kind, arm);
  const context = await browser.newContext({
    viewport: VIEW,
    deviceScaleFactor: 1,
  });
  if (arm.init) await context.addInitScript(arm.init);
  const page = await context.newPage();
  await page.goto("about:blank");
  await page.setContent(BARE(arm.style ?? ""));
  const held = Buffer.from(await paintAndRead(page, picture), "base64");
  await page.waitForTimeout(SETTLE_MS);
  const shot = decodePng(
    await page.screenshot({
      clip: { x: 0, y: 0, width: WIDTH, height: HEIGHT },
    }),
  );
  const version = browser.version();
  await browser.close();
  const canvas = differingBytes(picture, held, 4);
  const screenshot = differingBytes(picture, shot.rows, 3 + (shot.step === 4));
  return {
    browser: kind,
    version,
    arm: arm.name,
    canvas,
    screenshot,
    shotStep: shot.step,
    red: canvas + screenshot > 0 ? RED : null,
  };
}

async function main() {
  const playwright = await playwrightFromEnv();
  const picture = pictureBytes();
  const lines = [];
  for (const arm of ARMS) {
    for (const kind of arm.browsers) {
      const line = await runArm(playwright, kind, arm, picture).catch(
        (error) => ({ browser: kind, arm: arm.name, crashed: error.message }),
      );
      lines.push(JSON.stringify(line));
      process.stdout.write(`${lines.at(-1)}\n`);
    }
  }
  writeFileSync(process.argv[2], `${lines.join("\n")}\n`);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) await main();
