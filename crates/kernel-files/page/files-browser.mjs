#!/usr/bin/env node
// SPDX-License-Identifier: MIT
// The file fronts in headless Chromium, on the page's main thread, over the memory folder. It
// prints every row as `CHECK p <row>` and exits non-zero if a row without a told reason left the
// table; the table and the judging live in the export, shared with the host tests.
//
// Run it inside a private sound server: the flag `launch` passes keeps no stream from opening.
//
// Call: PLAYWRIGHT=<playwright's index.mjs> node files-browser.mjs <page_files.wasm>
import { readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

const { chromium } = await import(pathToFileURL(process.env.PLAYWRIGHT).href);
const wasm = readFileSync(process.argv[2]);
const imports = WebAssembly.Module.imports(new WebAssembly.Module(wasm));
const browser = await chromium.launch({ args: ["--mute-audio", "--no-sandbox"] });
let read;
try {
  const page = await browser.newPage();
  await page.setContent('<!doctype html><html lang="en"><title>Files</title><body></body></html>');
  read = await page.evaluate(async (encoded) => {
    const bytes = Uint8Array.from(atob(encoded), (c) => c.charCodeAt(0));
    const { instance } = await WebAssembly.instantiate(bytes, {});
    const { answers, answers_len, memory, wrong } = instance.exports;
    const at = answers();
    const text = new TextDecoder().decode(new Uint8Array(memory.buffer, at, answers_len()));
    return { text, wrong: wrong() };
  }, wasm.toString("base64"));
  console.log(`MEASURE page browser = ${browser.version()}`);
} finally {
  await browser.close();
}
console.log(`MEASURE page imports = ${imports.length}`);

for (const row of read.text.split("\n")) {
  console.log(`CHECK p ${row}`);
}
if (read.wrong !== 0) {
  console.log(`the page left the table on ${read.wrong} rows`);
  process.exitCode = 1;
}
