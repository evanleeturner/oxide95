// SPDX-License-Identifier: MIT
// The page's pure logic, under `node --test`. The browser half is `browser.mjs`.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { deflateSync } from "node:zlib";

import { countMatches, pickCount, planBeeps } from "./beep.mjs";
import { missingFeatures, peakHz, withinOneBin } from "./computer.mjs";
import { BORROWED, copyPage, SHIPPED } from "./copy-page.mjs";
import { KEYS, keyFaults } from "./key.mjs";
import {
  BANDS,
  decodePng,
  differingBytes,
  HEIGHT,
  pictureBytes,
  WIDTH,
} from "./picture.mjs";
import {
  buildReport,
  passSentence,
  reportHoldsName,
  sentence,
} from "./report.mjs";

const PAGE_DIR = fileURLToPath(new URL(".", import.meta.url));

test("missingFeatures names what the scope lacks, and nothing when whole", () => {
  const whole = {
    AudioContext: 1,
    AnalyserNode: 1,
    OscillatorNode: 1,
    WebAssembly: 1,
  };
  assert.deepEqual(missingFeatures(whole), []);
  assert.deepEqual(missingFeatures({ ...whole, AudioContext: undefined }), [
    "AudioContext",
  ]);
});

test("sentence fills its feature and refuses an unknown check", () => {
  assert.match(sentence("C2", { feature: "AudioContext" }), /no AudioContext,/);
  assert.throws(() => sentence("C2"), /needs a feature/);
  assert.throws(() => sentence("Z9"), /no sentence/);
});

test("C4's pass never claims a device or a speaker works", () => {
  assert.doesNotMatch(passSentence("C4"), /device|speaker/i);
  assert.match(passSentence("C4"), /does not show/);
});

test("the beeps run under 3 seconds at the largest count", () => {
  const beeps = planBeeps(4, 10);
  assert.equal(beeps.length, 4);
  assert.ok(beeps.at(-1).stop - 10 < 3);
  for (const { start, stop } of beeps) assert.ok(stop > start);
});

test("pickCount answers only 2, 3 or 4 across its range", () => {
  const seen = new Set();
  for (let byte = 0; byte < 256; byte += 1) seen.add(pickCount(byte / 256));
  assert.deepEqual([...seen].sort(), [2, 3, 4]);
});

test("countMatches is true only for the count played", () => {
  assert.equal(countMatches(3, 3), true);
  assert.equal(countMatches(2, 3), false);
});

test("one bin either way passes, further does not", () => {
  const bin = 44100 / 2048;
  assert.equal(withinOneBin(440 + bin, 440, 44100, 2048), true);
  assert.equal(withinOneBin(440 + 2 * bin, 440, 44100, 2048), false);
});

test("peakHz reads the loudest bin, and null from silence", () => {
  assert.equal(peakHz(new Uint8Array(8), 44100, 2048), null);
  const bins = new Uint8Array(8);
  bins[2] = 200;
  assert.equal(peakHz(bins, 2048, 2048), 2);
});

test("a report holds no label, and a leak is caught", () => {
  const label = "A Person's Headphones";
  const devices = [{ kind: "audiooutput", label }];
  const text = buildReport([{ check: "C1", state: "pass" }], devices);
  assert.match(text, /Speaker 1/);
  assert.equal(reportHoldsName(text, [label]), false);
  assert.equal(reportHoldsName(`${text}\n${label}`, [label]), true);
  assert.equal(reportHoldsName(text, [""]), false);
});

test("the copy holds exactly the shipped files and the logger", () => {
  const root = mkdtempSync(join(tmpdir(), "layer-check-"));
  const crates = join(root, "crates");
  for (const crate of ["one", "two"]) {
    mkdirSync(join(crates, crate, "page"), { recursive: true });
    writeFileSync(join(crates, crate, "page", "logger.mjs"), "same");
  }
  const borrowed = BORROWED.map(([crate, name]) => {
    mkdirSync(join(crates, crate, "page"), { recursive: true });
    writeFileSync(join(crates, crate, "page", name), "borrowed");
    return name;
  });
  const out = join(root, "out");
  copyPage(crates, PAGE_DIR, out);
  assert.deepEqual(
    readdirSync(out).sort(),
    [...SHIPPED, ...borrowed, "logger.mjs"].sort(),
  );
});

// One source: the pad script is the joystick crate's own, byte for byte.
test("the copy's pad script is the joystick crate's, and the page ships devices.mjs", () => {
  const out = join(mkdtempSync(join(tmpdir(), "layer-check-")), "out");
  const crates = join(PAGE_DIR, "..", "..");
  copyPage(crates, PAGE_DIR, out);
  assert.ok(SHIPPED.includes("devices.mjs"));
  assert.ok(
    readFileSync(join(out, "gamepads.mjs")).equals(
      readFileSync(join(crates, "joystick", "page", "gamepads.mjs")),
    ),
  );
});

// The game's two sound routes ship as their crates' own files, byte for byte.
test("the copy's sound and disc scripts are their crates' own", () => {
  const out = join(mkdtempSync(join(tmpdir(), "layer-check-")), "out");
  const crates = join(PAGE_DIR, "..", "..");
  copyPage(crates, PAGE_DIR, out);
  for (const [crate, name] of [
    ["sound-mix", "sound.mjs"],
    ["cd-audio", "cd-player.mjs"],
  ]) {
    assert.ok(
      readFileSync(join(out, name)).equals(
        readFileSync(join(crates, crate, "page", name)),
      ),
      name,
    );
  }
});

test("the copy refuses when two loggers differ", () => {
  const root = mkdtempSync(join(tmpdir(), "layer-check-"));
  const crates = join(root, "crates");
  for (const crate of ["one", "two"]) {
    mkdirSync(join(crates, crate, "page"), { recursive: true });
    writeFileSync(join(crates, crate, "page", "logger.mjs"), crate);
  }
  assert.throws(
    () => copyPage(crates, PAGE_DIR, join(root, "out")),
    /loggers differ/,
  );
});

test("the copy script exits non-zero and copies nothing when two loggers differ", () => {
  const root = mkdtempSync(join(tmpdir(), "layer-check-"));
  for (const crate of ["one", "two"]) {
    mkdirSync(join(root, crate, "page"), { recursive: true });
    writeFileSync(join(root, crate, "page", "logger.mjs"), crate);
  }
  const script = join(root, "one", "page", "copy-page.mjs");
  copyFileSync(join(PAGE_DIR, "copy-page.mjs"), script);
  const out = join(root, "out");
  const run = spawnSync(process.execPath, [script, out]);
  assert.notEqual(run.status, 0);
  assert.equal(existsSync(out), false);
});

/** A channel's value after a round trip through `bits` bits, as a shallow screen keeps it. */
function roundTrip(value, bits) {
  const kept = value >> (8 - bits);
  return (kept << (8 - bits)) | (kept >> (2 * bits - 8));
}

test("every band's every channel fails a 5-6-5 screen, as pure colours do not", () => {
  assert.equal(roundTrip(0xff, 5), 0xff);
  for (const [red, green, blue] of BANDS) {
    assert.notEqual(roundTrip(red, 5), red);
    assert.notEqual(roundTrip(green, 6), green);
    assert.notEqual(roundTrip(blue, 5), blue);
  }
});

test("the picture is opaque and a turned half shows a flip", () => {
  const bytes = pictureBytes();
  assert.equal(bytes.length, WIDTH * HEIGHT * 4);
  assert.equal(bytes[3], 255);
  assert.deepEqual([...bytes.subarray(0, 3)], BANDS[0]);
  const lastRow = (HEIGHT - 1) * WIDTH * 4;
  assert.deepEqual([...bytes.subarray(lastRow, lastRow + 3)], BANDS[1]);
});

test("differingBytes counts one wrong byte, and colour alone at step 3", () => {
  const wanted = Buffer.from([1, 2, 3, 255, 4, 5, 6, 255]);
  assert.equal(differingBytes(wanted, wanted, 4), 0);
  assert.equal(
    differingBytes(wanted, Buffer.from([1, 2, 3, 255, 4, 5, 7, 255]), 4),
    1,
  );
  assert.equal(differingBytes(wanted, Buffer.from([1, 2, 3, 4, 5, 6]), 3), 0);
});

function png(kind, depth, filtered) {
  const chunk = (type, data) => {
    const size = Buffer.alloc(4);
    size.writeUInt32BE(data.length);
    return Buffer.concat([size, Buffer.from(type), data, Buffer.alloc(4)]);
  };
  const head = Buffer.alloc(13);
  head.writeUInt32BE(2, 0);
  head.writeUInt32BE(1, 4);
  head[8] = depth;
  head[9] = kind;
  return Buffer.concat([
    Buffer.alloc(8),
    chunk("IHDR", head),
    chunk("IDAT", deflateSync(filtered)),
  ]);
}

test("decodePng undoes a row's filter and refuses a kind it does not take", () => {
  const raw = Buffer.from([1, 10, 20, 30, 1, 1, 1]);
  const { rows, step } = decodePng(png(2, 8, raw));
  assert.equal(step, 3);
  assert.deepEqual([...rows], [10, 20, 30, 11, 21, 31]);
  assert.throws(() => decodePng(png(3, 8, raw)), /does not take/);
  assert.throws(() => decodePng(png(2, 16, raw)), /does not take/);
});

test("keyFaults is empty on a whole trusted run and names each wrong event", () => {
  const whole = KEYS.flatMap(([, code]) =>
    ["keydown", "keyup"].map((type) => ({ type, code, trusted: true })),
  );
  assert.deepEqual(keyFaults(whole, KEYS), []);
  assert.equal(keyFaults([], KEYS).length, 6);
  const lying = whole.map((seen) => ({ ...seen, trusted: false }));
  assert.match(keyFaults(lying, KEYS)[0], /isTrusted=false/);
});
