#!/usr/bin/env node
// SPDX-License-Identifier: MIT
// The speaker choice in real browsers (H4): the real page, started, its picker set to the
// second of two null sinks a private sound server holds, so every sound of a run stays
// inside that server. A plant is a text swap the server makes in a served file, so no plant
// ships. It prints what each arm read, one JSON line an arm; a separate script scores it.
//
// `data-speaker-taken` and the private server's streams show the browser took the choice,
// never that sound left that speaker: the ear stays owed. No device's id or label is
// printed; a label leaves only as "is it the made sink's own". It runs inside
// `private_sound.sh` with sinks `probe_a` (the default) and `probe_b` made, and refuses
// outside it.
//
// Call: PLAYWRIGHT=<playwright's index.mjs> AXE=<axe.min.js> \
//   node speaker-browser.mjs <static folder> <out file> <temp folder>
import { execFile } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { createServer } from "node:https";
import { promisify } from "node:util";

import {
  axeNames,
  launch,
  listen,
  makeCert,
  makeRoute,
  playwrightFromEnv,
} from "./browser.mjs";

// The private server's second sink, by the description its maker gave it.
const SECOND_LABEL = "LaneProbeB";
const SECOND_FROM = "the private server's null sink probe_b, picked by its own label";
const SINKS = { probe_a: "A", probe_b: "B" };
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
const ARMS = [
  { name: "clean" },
  { name: "H4-mix", swap: ["sound.mjs", "setSink(id) {", 'setSink(_id, id = "") {'] },
  {
    name: "H4-cd",
    swap: [
      "cd-player.mjs",
      "apply.setSink = async (id) => {",
      'apply.setSink = async (_id, id = "") => {',
    ],
  },
  { name: "A-nameless", swap: ["index.html", "Speaker for the game's sound", ""] },
];

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

/** Starts the page, grants where the browser needs it, picks B, and answers the reads. */
async function driveArm(page, kind, ctx) {
  await page.goto(`https://127.0.0.1:${ctx.port}/index.html?log=debug`);
  await page.click(".js-start", { timeout: WAIT_MS });
  await attached(page, 'body[data-ran="true"]');
  if (kind === "firefox") {
    await page.click(".js-ask", { timeout: WAIT_MS });
    await attached(page, '[data-check="H3"]');
  }
  await page.waitForFunction(
    (label) =>
      [...document.querySelectorAll(".js-speaker-choice option")].some(
        (row) => row.textContent === label,
      ),
    SECOND_LABEL,
    { timeout: WAIT_MS },
  );
  await wait(SETTLE_MS);
  const before = await streams();
  await page.selectOption(".js-speaker-choice", { label: SECOND_LABEL });
  await attached(page, "body[data-speaker-taken]");
  await wait(SETTLE_MS);
  const after = await streams();
  const axe = await axeNames(page, ctx.axeSource);
  const read = await page.evaluate((label) => {
    const options = [...document.querySelectorAll(".js-speaker-choice option")];
    const second = options.find((row) => row.textContent === label);
    return {
      rows: [...document.querySelectorAll("[data-check]")].map((row) => ({
        check: row.dataset.check,
        state: row.dataset.state,
        words: row.textContent,
      })),
      taken: document.body.dataset.speakerTaken,
      pickedSecond: document.querySelector(".js-speaker-choice").value === second?.value,
      secrets: options.filter((row) => row.value).flatMap((row) => [row.value, row.textContent]),
      report: document.querySelector(".js-report-text").textContent,
    };
  }, SECOND_LABEL);
  return { before, after, axe, read };
}

async function runArm(ctx, kind, armDef) {
  const arm = { ...armDef, swaps: armDef.swap ? [armDef.swap] : [] };
  ctx.state.arm = arm;
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
    found = await driveArm(page, kind, ctx);
  } catch (error) {
    crashed = error.message.split("\n")[0];
  } finally {
    await browser.close().catch(() => {});
  }
  const secrets = found?.read.secrets ?? [];
  const holds = (text) => secrets.some((secret) => text.includes(secret));
  const record = {
    browser: kind,
    arm: arm.name,
    secondFrom: SECOND_FROM,
    crashed,
    rows: found?.read.rows ?? null,
    taken: found?.read.taken ?? null,
    pickedSecond: found?.read.pickedSecond ?? null,
    streamsBefore: found?.before ?? null,
    streamsAfter: found?.after ?? null,
    axe: found?.axe ?? null,
    secretsListed: secrets.length,
    secretInConsole: logged.some(holds),
    secretInReport: found ? holds(found.read.report) : null,
    logLines: logged.length,
    errors,
  };
  const secretInOutput = holds(JSON.stringify(record));
  out(secretInOutput ? { browser: kind, arm: arm.name, secretInOutput } : { ...record, secretInOutput });
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
    axeSource: readFileSync(process.env.AXE, "utf8"),
  };
  out({ stage: "sinks", streams: await streams() });
  const only = process.env.SPEAKER_ARMS?.split(",");
  try {
    for (const kind of ["chromium", "firefox"]) {
      for (const arm of ARMS) {
        if (!only || only.includes(arm.name)) await runArm(ctx, kind, arm);
      }
    }
  } finally {
    writeFileSync(outFile, `${lines.join("\n")}\n`);
    server.close();
  }
}

await main();
