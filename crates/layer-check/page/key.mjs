#!/usr/bin/env node
// SPDX-License-Identifier: MIT
// A key in real browsers with none of the other crates' code: a browser driver's key on
// a bare page, read by the page's own listeners. A wrong answer here clears the layer and
// names the driver or the screen.
//
// It prints what each arm read, and a separate script scores the output. Every key here
// enters past the device, and nobody pressed one: this says nothing of a hand or a key cap.
//
// Call: PLAYWRIGHT=<playwright's index.mjs> node key.mjs <out file>
import { writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { launch, playwrightFromEnv } from "./browser.mjs";

export const RED =
  "A key with no oxide95 code never arrived. The browser's driver or the screen, not the layer.";
export const KEYS = [
  ["a", "KeyA"],
  ["Escape", "Escape"],
  ["Shift", "ShiftLeft"],
];

const EVENTS = ["keydown", "keyup"];
const BROWSERS = ["chromium", "firefox"];
const ARMS = ["clean", "plant-dispatched", "plant-silent"];
const BARE =
  '<!doctype html><body><canvas id="c" width="32" height="20" tabindex="0"></canvas>';

/** What is wrong with `seen`: each key wants one trusted down and one trusted up, in its own code. */
export function keyFaults(seen, keys) {
  const faults = [];
  for (const [, code] of keys) {
    for (const type of EVENTS) {
      const found = seen.filter((v) => v.type === type && v.code === code);
      if (found.length !== 1) {
        faults.push(`${code} ${type}: ${found.length} events`);
      } else if (!found[0].trusted) {
        faults.push(`${code} ${type}: isTrusted=false`);
      }
    }
  }
  return faults;
}

async function send(page, arm) {
  for (const [key, code] of KEYS) {
    if (arm === "clean") {
      await page.keyboard.down(key);
      await page.keyboard.up(key);
    } else if (arm === "plant-dispatched") {
      await page.evaluate(
        ([k, c, types]) => {
          for (const type of types) {
            window.dispatchEvent(new KeyboardEvent(type, { key: k, code: c }));
          }
        },
        [key, code, EVENTS],
      );
    }
  }
}

async function runArm(playwright, kind, arm) {
  const browser = await launch(playwright, kind);
  const context = await browser.newContext();
  const page = await context.newPage();
  await page.setContent(BARE);
  await page.evaluate((types) => {
    window.seen = [];
    for (const type of types) {
      window.addEventListener(type, (event) => {
        window.seen.push({
          type,
          code: event.code,
          trusted: event.isTrusted,
        });
      });
    }
  }, EVENTS);
  await page.focus("#c");
  await send(page, arm);
  const seen = await page.evaluate(() => window.seen);
  const version = browser.version();
  await browser.close();
  const faults = keyFaults(seen, KEYS);
  return {
    browser: kind,
    version,
    arm,
    seen,
    faults,
    red: faults.length ? RED : null,
  };
}

async function main() {
  const playwright = await playwrightFromEnv();
  const lines = [];
  for (const arm of ARMS) {
    for (const kind of BROWSERS) {
      const line = await runArm(playwright, kind, arm).catch((error) => ({
        browser: kind,
        arm,
        crashed: error.message,
      }));
      lines.push(JSON.stringify(line));
      process.stdout.write(`${lines.at(-1)}\n`);
    }
  }
  writeFileSync(process.argv[2], `${lines.join("\n")}\n`);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) await main();
