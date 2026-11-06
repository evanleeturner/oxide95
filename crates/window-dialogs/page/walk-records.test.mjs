// SPDX-License-Identifier: MIT
// `node --test`: the fence on `walk-records.mjs`' contract. The text is what
// `view`'s own tests and `walk-check.mjs` hold the crate to.
import assert from "node:assert/strict";
import test from "node:test";

import { barFromText, boxFromText, boxKeyCode } from "./walk-records.mjs";

test("a box not open reads as null, and so does a window with no bar", () => {
  assert.equal(boxFromText(""), null);
  assert.equal(barFromText(""), null);
});

test("a box's title and a control's cells come back by name", () => {
  const text =
    "box\t9\t0\t0\t10\t10\tA & B\n" +
    "default\t11\t1\t0\t1\t0\t0\t5\t5\tOK\n" +
    "check\t12\t48\t1\t0\t0\t0\t5\t5\tFirst\n";
  const found = boxFromText(text);
  assert.equal(found.handle, 9);
  assert.equal(found.title, "A & B");
  assert.deepEqual(found.controls[0], {
    kind: "default",
    handle: 11,
    id: 1,
    checked: false,
    focused: true,
    words: "OK",
  });
  assert.equal(found.controls[1].checked, true);
});

test("words come back unescaped, an empty one too", () => {
  const text =
    "box\t9\t0\t0\t1\t1\ta\\tb\nlabel\t2\t3\t0\t0\t0\t0\t1\t1\tc\\\\d\\ne\n" +
    "label\t4\t5\t0\t0\t0\t0\t1\t1\t\n";
  const found = boxFromText(text);
  assert.equal(found.title, "a\tb");
  assert.equal(found.controls[0].words, "c\\d\ne");
  assert.equal(found.controls[1].words, "");
});

test("a popup holds the rows one deeper than it, in order", () => {
  const text =
    "bar\t119\t2\n" +
    "popup\t0\t0\t1\t0\tFile\n" +
    "item\t1\t16\t1\t0\tOpen\n" +
    "separator\t1\t0\t1\t0\t\n" +
    "item\t1\t17\t0\t1\tSave\n" +
    "popup\t0\t0\t1\t0\tHelp\n" +
    "item\t1\t19\t1\t0\tAbout\n";
  const found = barFromText(text);
  assert.equal(found.drawn, 2);
  assert.deepEqual(
    found.rows.map((row) => [row.words, row.rows.length]),
    [
      ["File", 3],
      ["Help", 1],
    ],
  );
  const save = found.rows[0].rows[2];
  assert.deepEqual([save.id, save.enabled, save.checked], [17, false, true]);
});

test("a focused button keeps Return; a check hands it to the default", () => {
  assert.equal(boxKeyCode("Enter", "button"), null);
  assert.equal(boxKeyCode("Enter", "checkbox"), 0x0d);
  assert.equal(boxKeyCode("Escape", "button"), 0x1b);
  assert.equal(boxKeyCode("Escape", "checkbox"), 0x1b);
});

test("Tab and Space stay the browser's", () => {
  for (const type of ["button", "checkbox"]) {
    assert.equal(boxKeyCode("Tab", type), null);
    assert.equal(boxKeyCode(" ", type), null);
  }
});
