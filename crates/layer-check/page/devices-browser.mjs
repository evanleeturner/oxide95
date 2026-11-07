#!/usr/bin/env node
// SPDX-License-Identifier: MIT
// The device checks in real browsers: the real page, opened and never started, so no
// sound context and no beep runs in any arm. A plant is a text swap the server makes in a
// served file, or a script run before the page's, so no plant ships. It prints what each
// arm read, one JSON line an arm, and a separate script scores the output.
//
// A stub shows what the page does with an answer. A person's plug and grant stay
// owed. A device's label is never printed, and every arm but `list-real` runs with the
// sound server out of reach. A new stream this run opened stops the run with exit 3.
//
// Call: PLAYWRIGHT=<playwright's index.mjs> AXE=<axe.min.js> \
//   node devices-browser.mjs <static folder> <out folder> <temp folder>
import { execFile } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { createServer } from "node:https";
import { join } from "node:path";
import { promisify } from "node:util";

import {
  axeNames,
  launch,
  listen,
  makeCert,
  makeRoute,
  playwrightFromEnv,
} from "./browser.mjs";

const FAKE_LABEL = "Ann Example's Headset";
const DEAD_SERVER = "/nonexistent/pulse";
const WAIT_MS = 20000;
const QUESTION_WAIT_MS = 35000;
const SETTLE_MS = 900;
const POLL_MS = 200;
const FAKE_PREFS = {
  "media.navigator.streams.fake": true,
  "media.navigator.permission.disabled": true,
};
const FAKE_ARGS = [
  "--use-fake-device-for-media-stream",
  "--use-fake-ui-for-media-stream",
];
// A device for Firefox to ask about, and its real question, which nobody here answers.
// With no device at all, Firefox refuses at once and no question is ever open.
const QUESTION_PREFS = {
  "media.navigator.streams.fake": true,
  "media.navigator.permission.fake": true,
};
const LISTENER = `mediaDevices.addEventListener("devicechange", () => void readNow("plug"));`;
const ASK_WORDS = "Let this page see my devices";

const run = promisify(execFile);
const wait = (ms) => new Promise((done) => setTimeout(done, ms));

// Runs before the page's own scripts, in every arm. The track's label is read here and
// only a boolean leaves; a refusal leaves by its error's name, which holds no device.
// Firefox names its fake microphone "Default Audio Device", read from its own library.
const SPY = `
window.spy = { contexts: 0, starts: 0, listings: 0, micFake: null, micRefused: null };
window.micTracks = [];
const RealContext = window.AudioContext;
window.AudioContext = class extends RealContext {
  constructor(...values) { super(...values); window.spy.contexts += 1; }
};
const realStart = AudioScheduledSourceNode.prototype.start;
AudioScheduledSourceNode.prototype.start = function (...values) {
  window.spy.starts += 1;
  return realStart.apply(this, values);
};
const media = navigator.mediaDevices;
if (media) {
  const realList = media.enumerateDevices.bind(media);
  media.enumerateDevices = () => {
    window.spy.listings += 1;
    return realList();
  };
  const realAsk = media.getUserMedia.bind(media);
  media.getUserMedia = async (wanted) => {
    try {
      const stream = await realAsk(wanted);
      window.micTracks = stream.getTracks();
      window.spy.micFake = window.micTracks.some((track) =>
        /fake|sine|monochrome|^Default Audio Device$/i.test(track.label),
      );
      return stream;
    } catch (error) {
      window.spy.micRefused = error.name;
      throw error;
    }
  };
}
`;

// The list the stubbed arms read.
function stubs(arm) {
  return `
if (media) {
  let outputs = ${arm.outputs ?? 1};
  media.enumerateDevices = async () => {
    window.spy.listings += 1;
    return Array.from({ length: outputs }, (_, at) => ({
      kind: "audiooutput",
      label: at === 0 ? ${JSON.stringify(FAKE_LABEL)} : "",
      deviceId: String(at),
      groupId: "g",
    }));
  };
  window.plug = () => {
    outputs += 1;
    media.dispatchEvent(new Event("devicechange"));
  };
  ${arm.never ? "media.getUserMedia = () => new Promise(() => {});" : ""}
}
`;
}

const ARMS = [
  { name: "list-real", realServer: true },
  { name: "clean", stub: true },
  { name: "H1-zero", stub: true, outputs: 0 },
  { name: "H2-ignored", stub: true, swap: ["devices.mjs", LISTENER, ""] },
  { name: "H2-plug", stub: true, plug: true },
  { name: "H3-grant", fake: true, ask: true },
  { name: "H3-never", stub: true, never: true, ask: true },
  { name: "H3-nodevice", only: "firefox", ask: true },
  { name: "H3-plain", only: "firefox", held: true, ask: true },
  {
    name: "A-noname",
    stub: true,
    swap: ["index.html", ASK_WORDS, ""],
  },
];

const lines = [];
function out(record) {
  const line = JSON.stringify(record);
  lines.push(line);
  process.stdout.write(`${line}\n`);
}

function pactlEnv() {
  const env = { ...process.env, LC_ALL: "C" };
  delete env.PULSE_SERVER;
  return env;
}

/** Every stream the real sound server holds now, each as its index and its owner's pid. */
async function listStreams() {
  const found = [];
  for (const kind of ["sink-inputs", "source-outputs"]) {
    const { stdout } = await run("pactl", ["list", kind], { env: pactlEnv() });
    for (const block of stdout.split(/\n(?=\S)/)) {
      const index = block.match(/#(\d+)/);
      if (!index) continue;
      const pid = block.match(/application\.process\.id = "(\d+)"/);
      found.push({
        id: `${kind}#${index[1]}`,
        pid: pid ? Number(pid[1]) : null,
      });
    }
  }
  return found;
}

/** True when `pid`'s parents reach this process, by `/proc/<pid>/stat`'s fourth field. */
function reachesThisRun(pid) {
  let at = pid;
  while (at > 1) {
    if (at === process.pid) return true;
    let stat;
    try {
      stat = readFileSync(`/proc/${at}/stat`, "utf8");
    } catch {
      return false;
    }
    at = Number(stat.slice(stat.lastIndexOf(")") + 2).split(" ")[1]);
  }
  return false;
}

/** A stream this run opened: told at once, and nothing more runs. */
function stopRun(ctx, id, pid) {
  out({ stop: "a stream of this run", stream: id, pid });
  process.exitCode = 3;
  ctx.stopped = true;
  void ctx.browser?.close();
}

/** Reads the streams until the returned call, which answers the tally. */
function startPoll(ctx) {
  const tally = { reads: 0, failed: 0, ours: 0, others: 0 };
  let running = true;
  const done = (async () => {
    while (running) {
      try {
        const found = await listStreams();
        tally.reads += 1;
        for (const { id, pid } of found) {
          if (ctx.seen.has(id)) continue;
          ctx.seen.add(id);
          if (pid === null || reachesThisRun(pid)) {
            tally.ours += 1;
            stopRun(ctx, id, pid);
          } else {
            tally.others += 1;
          }
        }
      } catch {
        tally.failed += 1;
      }
      await wait(POLL_MS);
    }
  })();
  return async () => {
    running = false;
    await done;
    return tally;
  };
}

const readRows = (page) =>
  page.evaluate(() =>
    [...document.querySelectorAll("[data-check]")].map((row) => ({
      check: row.dataset.check,
      state: row.dataset.state,
      words: row.textContent,
    })),
  );

const countDeviceRows = (page) =>
  page.evaluate(() => document.querySelectorAll(".js-device-list li").length);

const attached = (page, selector, timeout = WAIT_MS) =>
  page.waitForSelector(selector, { state: "attached", timeout });

/** Runs one arm's steps on an open page and answers what it read. */
async function driveArm(page, arm, ctx) {
  await page.goto(`https://127.0.0.1:${ctx.tlsPort}/index.html?log=debug`);
  await attached(page, '[data-check="H2"]');
  const rowsBefore = await readRows(page);
  const deviceRowsBefore = await countDeviceRows(page);
  const listingsBefore = await page.evaluate(() => window.spy.listings);
  if (arm.plug) await page.evaluate(() => window.plug());
  if (arm.ask) {
    await page.click(".js-ask", { timeout: WAIT_MS });
    await attached(page, '[data-check="H3"]', QUESTION_WAIT_MS);
  }
  await page.waitForTimeout(SETTLE_MS);
  const axe = await axeNames(page, ctx.axeSource);
  const after = await page.evaluate(
    (label) => ({
      spy: window.spy,
      micStopped:
        window.micTracks.length > 0
          ? window.micTracks.every((track) => track.readyState === "ended")
          : null,
      labelOnRow: [...document.querySelectorAll(".js-device-list li")].some(
        (row) => row.textContent.includes(label),
      ),
      started: document.body.dataset.ran === "true",
    }),
    FAKE_LABEL,
  );
  return {
    rowsBefore,
    rows: await readRows(page),
    deviceRowsBefore,
    deviceRows: await countDeviceRows(page),
    listingsBefore,
    axe,
    ...after,
  };
}

async function runArm(ctx, kind, armDef) {
  const arm = { ...armDef, swaps: armDef.swap ? [armDef.swap] : [] };
  ctx.state.arm = arm;
  ctx.state.hits = [];
  if (arm.realServer) delete process.env.PULSE_SERVER;
  else process.env.PULSE_SERVER = DEAD_SERVER;
  const stopPoll = startPoll(ctx);
  const errors = [];
  const logged = [];
  let read = null;
  let crashed = null;
  try {
    ctx.browser = await launch(
      ctx.playwright,
      kind,
      arm.fake
        ? { prefs: FAKE_PREFS, args: FAKE_ARGS }
        : arm.held
          ? { prefs: QUESTION_PREFS }
          : {},
    );
    const context = await ctx.browser.newContext({ ignoreHTTPSErrors: true });
    const init = `${SPY}\n${arm.stub ? stubs(arm) : ""}`;
    await context.addInitScript(`(() => {${init}\n})();`);
    const page = await context.newPage();
    page.on("pageerror", (error) => errors.push(error.message));
    page.on("console", (message) => logged.push(message.text()));
    read = await driveArm(page, arm, ctx);
  } catch (error) {
    crashed = error.message;
  } finally {
    await ctx.browser?.close().catch(() => {});
    ctx.browser = null;
  }
  const tally = await stopPoll();
  out({
    browser: kind,
    arm: arm.name,
    ...read,
    crashed,
    logLines: logged.length,
    labelInConsole: logged.some((text) => text.includes(FAKE_LABEL)),
    pollReads: tally.reads,
    pollFailed: tally.failed,
    streamsOurs: tally.ours,
    streamsOthers: tally.others,
    errors,
  });
}

async function main() {
  const [folder, outDir, tempDir] = process.argv.slice(2);
  mkdirSync(outDir, { recursive: true });
  mkdirSync(tempDir, { recursive: true });
  await run("pactl", ["info"], { env: pactlEnv() });
  const state = { folder, arm: {}, hits: [] };
  const tls = createServer(makeCert(tempDir), makeRoute(state));
  const ctx = {
    state,
    tlsPort: await listen(tls, "127.0.0.1"),
    playwright: await playwrightFromEnv(),
    axeSource: readFileSync(process.env.AXE, "utf8"),
    seen: new Set((await listStreams()).map(({ id }) => id)),
    browser: null,
    stopped: false,
  };
  try {
    for (const kind of ["chromium", "firefox"]) {
      for (const arm of ARMS) {
        if (ctx.stopped) return;
        if (!arm.only || arm.only === kind) await runArm(ctx, kind, arm);
      }
    }
  } finally {
    writeFileSync(
      join(outDir, "devices-output.jsonl"),
      `${lines.join("\n")}\n`,
    );
    tls.close();
  }
}

await main();
