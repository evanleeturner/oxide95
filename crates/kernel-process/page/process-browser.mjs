#!/usr/bin/env node
// SPDX-License-Identifier: MIT
// The page's process in headless Chromium, on its main thread. It prints every answer as
// `CHECK p <name> want=<a> got=<b>` and exits non-zero if a row of the table differs.
//
// The table is written here, apart from the crate, and holds the same answers the host checks ask
// of Wine's own calls; a page has no target of its own to ask. The memory map is handed in, so the
// pointer rows prove the front's turn and no map.
//
// A row marked KNOWN is a difference already told: the crate does not yet answer as the target
// does, and the row keeps the target's answer. It prints with the others and leaves the exit alone,
// so a later fix shows as want equal to got and needs no edit here.
//
// Run it inside a private sound server: the flag `launch` passes keeps no stream from opening.
//
// Call: PLAYWRIGHT=<playwright's index.mjs> node process-browser.mjs <page_process.wasm>
import { readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

const KNOWN = true;
const FOUND = `${0x20000000} error 0`;

const WANT = [
  ["start.command_line", "PROG.EXE -x"],
  ["start.version", `${0x23f00206}`],
  ["start.own", `${0x02000000}`],
  ["start.current_process", `${0xffffffff}`],
  ["start.flags", "0"],
  ["start.show", "1"],
  ["start.block.len", "14"],
  ["start.block.end", "0 0"],
  ["start.block_wide.len", "14"],
  ["start.block_wide.end", "0 0"],
  ["start.path.count", "15"],
  ["start.path.same", "true"],
  ["start.path.zero", "0"],
  ["start.free_block", "1"],
  ["start.free_block_wide", "1"],
  ["start.room4", "4 error 122", KNOWN],
  ["start.room4.last_is_zero", "false", KNOWN],
  ["names.load_miss", "0 error 126"],
  ["names.handle_miss", "0 error 126"],
  ["names.proc_miss", "0 error 127"],
  ["names.proc_found", "7 error 0"],
  ["names.load_full", FOUND],
  ["names.load_lower", FOUND],
  ["names.load_no_extension", FOUND],
  ["names.free_found", "1 error 0"],
  ["names.proc_bad", "0 error 126", KNOWN],
  ["names.free_bad", "0 error 126", KNOWN],
  ["names.file_bad", "0 error 126", KNOWN],
  ["names.terminate_bad", "0 error 6"],
  ["names.path_miss", "0 error 126", KNOWN],
  ["round.page.set", "1234"],
  ["round.page.clear", "0"],
  ["round.page.refused", "126"],
  ["round.page.back", "126"],
  ["pointers.page.read_null_1", "1"],
  ["pointers.page.read_null_0", "0"],
  ["pointers.page.read_good", "0"],
  ["pointers.page.read_past", "1"],
  ["pointers.page.write_good", "0"],
  ["pointers.page.write_past", "1"],
  ["pointers.page.code_good", "0"],
  ["pointers.page.code_past", "1"],
  ["files.memory.own", "2"],
  ["files.memory.through", "2"],
  ["files.disc.own", "2"],
  ["files.disc.through", "2"],
  ["threads.page.own", "87"],
  ["threads.page.through", "87"],
  ["time.page.own", "87"],
  ["time.page.through", "87"],
  ["text.refused", "true"],
  ["text.handed", "87"],
];

const { chromium } = await import(pathToFileURL(process.env.PLAYWRIGHT).href);
const wasm = readFileSync(process.argv[2]);
const imports = WebAssembly.Module.imports(new WebAssembly.Module(wasm));
const browser = await chromium.launch({ args: ["--mute-audio", "--no-sandbox"] });
let read;
try {
  const page = await browser.newPage();
  await page.setContent('<!doctype html><html lang="en"><title>Process</title><body></body></html>');
  read = await page.evaluate(async (encoded) => {
    const bytes = Uint8Array.from(atob(encoded), (c) => c.charCodeAt(0));
    const { instance } = await WebAssembly.instantiate(bytes, {});
    const { answers, answers_len, memory } = instance.exports;
    const at = answers();
    return new TextDecoder().decode(new Uint8Array(memory.buffer, at, answers_len()));
  }, wasm.toString("base64"));
  console.log(`MEASURE page browser = ${browser.version()}`);
} finally {
  await browser.close();
}
console.log(`MEASURE page imports = ${imports.length}`);

const got = read
  .trim()
  .split("\n")
  .map((line) => line.split(" = "));
let wrong = got.length === WANT.length ? 0 : 1;
if (wrong) {
  console.log(`the page answered ${got.length} rows where the table holds ${WANT.length}`);
}
got.forEach(([name, answer], at) => {
  const [wanted, want, known] = WANT[at] ?? ["", ""];
  console.log(`CHECK p ${name} want=${want} got=${answer}`);
  if (name !== wanted || (answer !== want && !known)) {
    wrong += 1;
  }
});
if (wrong) {
  console.log("the page left the table");
  process.exitCode = 1;
}
