#!/usr/bin/env node
// SPDX-License-Identifier: MIT
// The remembered speaker in real browsers (H5): the real page picks the second of two
// null sinks, is loaded again and started again; or it starts with a stored id no device
// has, then picks. Every sound stays inside the private server, as `speaker-browser.mjs`'s
// do, and this refuses outside it. It prints one JSON line an arm; a separate script
// scores it.
//
// The store is read in the page and leaves only as answers: is it the picked speaker's id,
// and does it hold a label. No device's id or label is printed.
//
// Call: PLAYWRIGHT=<playwright's index.mjs> \
//   node remember-browser.mjs <static folder> <out file> <temp folder>
import { execFile } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { createServer } from "node:https";
import { promisify } from "node:util";

import { launch, listen, makeCert, makeRoute, playwrightFromEnv } from "./browser.mjs";

// The private server's second sink, by the description its maker gave it.
const SECOND_LABEL = "LaneProbeB";
const SINKS = { probe_a: "A", probe_b: "B" };
const KEY = "layer-check.speaker";
// An id no device has. The page cannot tell it from one the browser renamed.
const GONE_ID = "no-such-speaker";
const WAIT_MS = 20000;
const SETTLE_MS = 1500;
// Firefox lists no output before a grant; these let the page's own grant pass unasked.
const FIREFOX_EXTRA = {
  prefs: {
    "media.navigator.streams.fake": true,
    "media.navigator.permission.disabled": true,
  },
};
// The browser's own devices stay listed: the fake device would swap both sinks away.
const CHROMIUM_EXTRA = { args: ["--use-fake-ui-for-media-stream"] };
const ARMS = ["remembered", "stale"];

const run = promisify(execFile);
const wait = (ms) => new Promise((done) => setTimeout(done, ms));
const lines = [];

function out(record) {
  const line = JSON.stringify(record);
  lines.push(line);
  process.stdout.write(`${line}\n`);
}

/** The private server's playing streams, counted by made sink and by corked or not. */
async function streams() {
  const env = { ...process.env, LC_ALL: "C" };
  const names = new Map();
  const { stdout: short } = await run("pactl", ["list", "short", "sinks"], { env });
  for (const row of short.trim().split("\n")) {
    const [index, name] = row.split("\t");
    names.set(index, SINKS[name] ?? "other");
  }
  const counts = {};
  const { stdout } = await run("pactl", ["list", "sink-inputs"], { env });
  for (const block of stdout.split(/\n(?=\S)/)) {
    if (!/#\d+/.test(block)) continue;
    const sink = names.get(block.match(/^\s*Sink: (\d+)/m)?.[1]) ?? "other";
    const state = block.match(/Corked: yes/) ? "corked" : "uncorked";
    counts[sink] ??= { corked: 0, uncorked: 0 };
    counts[sink][state] += 1;
  }
  return counts;
}

const attached = (page, selector) =>
  page.waitForSelector(selector, { state: "attached", timeout: WAIT_MS });
const pageUrl = (ctx) => `https://127.0.0.1:${ctx.port}/index.html?log=debug`;

/** Loads and starts the page, and grants where the browser lists no speaker before one. */
async function start(page, kind, ctx) {
  await page.goto(pageUrl(ctx));
  await page.click(".js-start", { timeout: WAIT_MS });
  await attached(page, 'body[data-ran="true"]');
  if (kind === "firefox") {
    await page.click(".js-ask", { timeout: WAIT_MS });
    await attached(page, '[data-check="H3"]');
  }
}

async function pickSecond(page) {
  await page.waitForFunction(
    (label) =>
      [...document.querySelectorAll(".js-speaker-choice option")].some(
        (row) => row.textContent === label,
      ),
    SECOND_LABEL,
    { timeout: WAIT_MS },
  );
  await page.selectOption(".js-speaker-choice", { label: SECOND_LABEL });
  await attached(page, "body[data-speaker-taken]");
  await wait(SETTLE_MS);
}

/** What the page shows now: its rows, the body's marks, and the picker and store as answers. */
function readPage(page) {
  return page.evaluate(
    ({ label, key }) => {
      const box = document.querySelector(".js-speaker-choice");
      const options = [...box.options];
      const second = options.find((row) => row.textContent === label);
      let stored = null;
      try {
        stored = localStorage.getItem(key);
      } catch {
        stored = null;
      }
      const named = options.filter((row) => row.value);
      return {
        rows: [...document.querySelectorAll("[data-check]")].map((row) => ({
          check: row.dataset.check,
          state: row.dataset.state,
          words: row.textContent,
        })),
        taken: document.body.dataset.speakerTaken ?? null,
        remembered: document.body.dataset.remembered ?? null,
        onDefault: document.body.dataset.speakerDefault ?? null,
        picker: box.value === "" ? "default" : box.value === second?.value ? "second" : "other",
        store: stored === null ? "empty" : stored === second?.value ? "second" : "other",
        storeHoldsLabel:
          stored !== null && named.some((row) => row.textContent && stored.includes(row.textContent)),
        secrets: named.flatMap((row) => [row.value, row.textContent]),
        report: document.querySelector(".js-report-text").textContent,
      };
    },
    { label: SECOND_LABEL, key: KEY },
  );
}

/** Picks B, then loads and starts the page again: H5 tries the stored id. */
async function remembered(page, kind, ctx) {
  await start(page, kind, ctx);
  await pickSecond(page);
  const picked = await readPage(page);
  await start(page, kind, ctx);
  await attached(page, "body[data-remembered]");
  await wait(SETTLE_MS);
  return { picked, streamsAfter: await streams(), after: await readPage(page) };
}

/** Starts with an id no device has, then picks B: the fall back, then a pick that stands. */
async function stale(page, kind, ctx) {
  await page.goto(pageUrl(ctx));
  await page.evaluate(({ key, id }) => localStorage.setItem(key, id), { key: KEY, id: GONE_ID });
  await start(page, kind, ctx);
  await attached(page, "body[data-remembered]");
  await wait(SETTLE_MS);
  const fellBack = await readPage(page);
  const streamsFellBack = await streams();
  await pickSecond(page);
  return { fellBack, streamsFellBack, streamsAfter: await streams(), after: await readPage(page) };
}

const shown = (read) => {
  if (!read) return null;
  const { secrets: _secrets, report: _report, ...rest } = read;
  return rest;
};

async function runArm(ctx, kind, name) {
  ctx.state.arm = { name, swaps: [] };
  ctx.state.hits = [];
  const errors = [];
  const logged = [];
  let found = null;
  let crashed = null;
  const browser = await launch(
    ctx.playwright,
    kind,
    kind === "firefox" ? FIREFOX_EXTRA : CHROMIUM_EXTRA,
  );
  try {
    const context = await browser.newContext({ ignoreHTTPSErrors: true });
    const page = await context.newPage();
    page.on("pageerror", (error) => errors.push(error.message));
    page.on("console", (message) => logged.push(message.text()));
    found = await (name === "remembered" ? remembered : stale)(page, kind, ctx);
  } catch (error) {
    crashed = error.message.split("\n")[0];
  } finally {
    await browser.close().catch(() => {});
  }
  const reads = [found?.picked, found?.fellBack, found?.after].filter(Boolean);
  const secrets = reads.flatMap((read) => read.secrets);
  const holds = (text) => secrets.some((secret) => text.includes(secret));
  const record = {
    browser: kind,
    arm: name,
    crashed,
    picked: shown(found?.picked),
    fellBack: shown(found?.fellBack),
    after: shown(found?.after),
    streamsFellBack: found?.streamsFellBack ?? null,
    streamsAfter: found?.streamsAfter ?? null,
    secretsListed: secrets.length,
    secretInConsole: logged.some(holds),
    secretInReport: reads.some((read) => holds(read.report)),
    logLines: logged.length,
    errors,
  };
  const secretInOutput = holds(JSON.stringify(record));
  out(secretInOutput ? { browser: kind, arm: name, secretInOutput } : { ...record, secretInOutput });
}

async function main() {
  if (process.env.PRIVATE_SOUND !== "1") {
    process.stderr.write("not inside the private server\n");
    process.exit(2);
  }
  const [folder, outFile, tempDir] = process.argv.slice(2);
  mkdirSync(tempDir, { recursive: true });
  const state = { folder, arm: {}, hits: [] };
  const server = createServer(makeCert(tempDir), makeRoute(state));
  const ctx = {
    state,
    port: await listen(server, "127.0.0.1"),
    playwright: await playwrightFromEnv(),
  };
  out({ stage: "sinks", streams: await streams() });
  try {
    for (const kind of ["chromium", "firefox"]) {
      for (const name of ARMS) await runArm(ctx, kind, name);
    }
  } finally {
    writeFileSync(outFile, `${lines.join("\n")}\n`);
    server.close();
  }
}

await main();
