#!/usr/bin/env node
// SPDX-License-Identifier: MIT
// Copies this page's shipped files into one static folder, with the one logger the
// crates share. Refuses when the crates' loggers differ, since then none is the shared
// one. Takes no test and no harness.
//
// Call: node copy-page.mjs <out folder>
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
} from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

export const SHIPPED = [
  "beep.mjs",
  "computer.mjs",
  "devices.mjs",
  "index.html",
  "layer-check.css",
  "layer-check.mjs",
  "report.mjs",
];

/** Files another crate owns, each as [crate, name]: one source, copied as it stands. */
export const BORROWED = [["joystick", "gamepads.mjs"]];

/** Every crate's `page/logger.mjs` under `cratesDir`, sorted by path. */
function loggerPaths(cratesDir) {
  return readdirSync(cratesDir)
    .map((crate) => join(cratesDir, crate, "page", "logger.mjs"))
    .filter((path) => existsSync(path))
    .sort();
}

/** Copies the page; throws when no logger exists or two differ. */
export function copyPage(cratesDir, pageDir, outDir) {
  const loggers = loggerPaths(cratesDir);
  if (loggers.length === 0) throw new Error(`no logger under ${cratesDir}`);
  const first = readFileSync(loggers[0]);
  for (const path of loggers) {
    if (!readFileSync(path).equals(first)) {
      throw new Error(`loggers differ: ${loggers[0]} and ${path}`);
    }
  }
  mkdirSync(outDir, { recursive: true });
  for (const name of SHIPPED)
    copyFileSync(join(pageDir, name), join(outDir, name));
  copyFileSync(loggers[0], join(outDir, "logger.mjs"));
  for (const [crate, name] of BORROWED) {
    copyFileSync(join(cratesDir, crate, "page", name), join(outDir, name));
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const pageDir = fileURLToPath(new URL(".", import.meta.url));
  copyPage(join(pageDir, "..", ".."), pageDir, process.argv[2]);
}
