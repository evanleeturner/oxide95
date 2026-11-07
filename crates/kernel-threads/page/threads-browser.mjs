#!/usr/bin/env node
// SPDX-License-Identifier: MIT
// The page's threads in headless Chromium, on its main thread: no Worker, and so one thread.
// It prints every answer and exits non-zero if one differs from the table.
//
// The table is memory, marked, and judged by nothing on the page: one thread cannot contend,
// so a held-out thread is not read here. A page has a second thread only from a Worker, which
// also needs shared memory, and this bare page has neither.
//
// A freed slot's rows are this back's own refusal, as the host back's are. Wine's own calls
// answer a read and a write of one, so these 2 rows hold what the crate does and not a judge's.
//
// Run it inside a private sound server: the flag `launch` passes keeps no stream from opening.
//
// Call: PLAYWRIGHT=<playwright's index.mjs> node threads-browser.mjs <page_threads.wasm>
import { readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

const WANT = [
  ["id", "1"],
  ["lock.twice", "2"],
  ["lock.one_left", "1"],
  ["lock.both_left", "0"],
  ["lock.over_left", "0"],
  ["lock.many", "1000"],
  ["lock.many_left", "0"],
  ["lock.deleted", "0"],
  ["slot.unset", "0 error 0"],
  ["slot.set", "1 error 0"],
  ["slot.get", "42 error 0"],
  ["slot.free", "1 error 0"],
  ["slot.freed.get", "0 error 87"],
  ["slot.freed.set", "0 error 87"],
  ["slot.freed.free", "0 error 87"],
  ["slot.bad.get", "0 error 87"],
  ["slot.bad.set", "0 error 87"],
  ["slot.bad.free", "0 error 87"],
  ["slot.count", "64"],
  ["slot.out", "4294967295"],
  ["slot.again.index", "same"],
  ["slot.again.unset", "0 error 0"],
  ["counter.inc.5", "6 held 6"],
  ["counter.dec.6", "5 held 5"],
  ["counter.inc.max", "-2147483648 held -2147483648"],
  ["counter.dec.min", "2147483647 held 2147483647"],
];

const { chromium } = await import(pathToFileURL(process.env.PLAYWRIGHT).href);
const wasm = readFileSync(process.argv[2]);
const imports = WebAssembly.Module.imports(new WebAssembly.Module(wasm));
const browser = await chromium.launch({ args: ["--mute-audio", "--no-sandbox"] });
let read;
try {
  const page = await browser.newPage();
  await page.setContent('<!doctype html><html lang="en"><title>Threads</title><body></body></html>');
  read = await page.evaluate(async (encoded) => {
    const bytes = Uint8Array.from(atob(encoded), (c) => c.charCodeAt(0));
    const { instance } = await WebAssembly.instantiate(bytes, {});
    const { answers, answers_len, memory } = instance.exports;
    const at = answers();
    const text = new TextDecoder().decode(new Uint8Array(memory.buffer, at, answers_len()));
    return { text, isolated: self.crossOriginIsolated, shared: typeof SharedArrayBuffer };
  }, wasm.toString("base64"));
  console.log(`MEASURE TH4 page browser = ${browser.version()}`);
} finally {
  await browser.close();
}

console.log(`MEASURE TH4 page imports = ${imports.length}`);
console.log(`MEASURE TH4 page isolated = ${read.isolated}`);
console.log(`MEASURE TH4 page shared_memory = ${read.shared}`);
console.log("MEASURE TH4 page judge = memory: one thread cannot contend");

const got = read.text
  .trim()
  .split("\n")
  .map((line) => line.split(" = "));
for (const [name, answer] of got) {
  console.log(`CHECK TH4 page ${name} = ${answer}`);
}
const same =
  got.length === WANT.length &&
  got.every(([name, answer], at) => name === WANT[at][0] && answer === WANT[at][1]);
if (!same) {
  console.log("the page left the table");
  process.exitCode = 1;
}
