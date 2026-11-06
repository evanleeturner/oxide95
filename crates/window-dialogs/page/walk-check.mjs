// SPDX-License-Identifier: MIT
// The thin wasm run for real, in Node with no browser: the exports alone, no page,
// no keyboard and no reader. The built file is the one argument.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const bytes = await readFile(process.argv[2]);
const module = await WebAssembly.compile(bytes);
assert.deepEqual(
  WebAssembly.Module.imports(module),
  [],
  "the wasm imports nothing",
);
const { exports: wasm } = await WebAssembly.instantiate(module);

const text = (length) =>
  new TextDecoder().decode(
    new Uint8Array(wasm.memory.buffer, wasm.text_ptr(), length),
  );
const records = (length) =>
  text(length)
    .split("\n")
    .filter(Boolean)
    .map((line) => line.split("\t"));
const row = (handle, id) =>
  records(wasm.box_text(handle)).find((cells) => cells[2] === String(id));
const NONE = 0xffffffff;
const asked = (value) => value >>> 0;

const bar = records(wasm.bar_text());
assert.deepEqual(bar[1], ["popup", "0", "0", "1", "0", "File"]);
assert.deepEqual(bar[3], ["item", "1", "17", "0", "0", "Save"]);
assert.deepEqual(bar[4].slice(0, 2), ["separator", "1"]);
wasm.pick(0x10);
wasm.run();
assert.equal(asked(wasm.picked()), 0x10);

const handle = wasm.open_box();
assert.notEqual(handle, 0);
assert.equal(records(wasm.box_text(handle))[0].at(-1), "A planted box");
assert.deepEqual(
  row(handle, 1).slice(3, 5),
  ["0", "1"],
  "the focus opens on OK",
);
assert.equal(row(handle, 1).at(-1), "OK");
assert.equal(row(handle, 0xffff)[0], "label");

assert.equal(wasm.press(handle, 0x30), 1);
wasm.run();
assert.equal(row(handle, 0x30)[3], "1", "the press flipped the check");
assert.equal(asked(wasm.ended()), NONE);

assert.equal(wasm.key(handle, 0x09), 1);
assert.equal(row(handle, 2)[4], "1", "Tab moved the focus to Cancel");

assert.equal(wasm.key(handle, 0x1b), 1);
wasm.run();
assert.equal(
  asked(wasm.ended()),
  2,
  "Escape reached Cancel, which closed the box",
);
assert.equal(wasm.box_text(handle), 0);
assert.equal(wasm.press(handle, 1), 0);
console.log(
  "walk-check: every check held, on",
  bytes.length,
  "bytes, 0 imports",
);
