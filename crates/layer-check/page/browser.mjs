#!/usr/bin/env node
// SPDX-License-Identifier: MIT
// The page in real browsers, muted, once clean and once per planted fault. A plant is
// an exact text swap the server makes in a served file, or a script run before the
// page's, so no plant ships; a swap that matches nothing stops the run. It prints what
// each arm read and judges nothing: a separate script scores the output.
//
// A green run is no conformance claim: nobody walked it and nobody heard it. The
// microphone here is a stub, and the count of beeps is answered by this script.
//
// Call: PLAYWRIGHT=<playwright's index.mjs> AXE=<axe.min.js> \
//   node browser.mjs <static folder> <out folder> <temp folder>
import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { createServer as plainServer } from "node:http";
import { createServer as tlsServer } from "node:https";
import { networkInterfaces } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

const FIREFOX_PATH = join(
  process.env.HOME,
  ".cache/ms-playwright/firefox-1522/firefox/firefox",
);
const FIREFOX_PREFS = {
  "media.autoplay.default": 1,
  "media.autoplay.blocking_policy": 0,
  "media.volume_scale": "0",
};
const CHROME_ARGS = [
  "--mute-audio",
  "--autoplay-policy=document-user-activation-required",
];
const FLOOR = ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"];
const FAKE_LABEL = "Ann Example's Headphones";
const WAIT_MS = 20000;
const SETTLE_MS = 500;
const TYPES = {
  ".html": "text/html",
  ".css": "text/css",
  ".mjs": "text/javascript",
};

// Runs before the page's own scripts, in every arm: what the browser was asked to do,
// kept apart from what the page says it did.
const SPY = `
window.spy = { contexts: 0, starts: [], mics: 0, violations: [] };
document.addEventListener("securitypolicyviolation", (event) => {
  window.spy.violations.push(event.effectiveDirective);
});
const RealContext = window.AudioContext;
window.AudioContext = class extends RealContext {
  constructor(...values) { super(...values); window.spy.contexts += 1; }
};
const realStart = AudioScheduledSourceNode.prototype.start;
AudioScheduledSourceNode.prototype.start = function (...values) {
  window.spy.starts.push(this.frequency ? this.frequency.value : 0);
  return realStart.apply(this, values);
};
if (navigator.mediaDevices) {
  navigator.mediaDevices.enumerateDevices = async () => [
    { kind: "audiooutput", label: ${JSON.stringify(FAKE_LABEL)}, deviceId: "x", groupId: "y" },
  ];
}
`;

// The microphone stubs: a person's real microphone is owed, and no browser here has one.
const MIC_LOOP = `
navigator.mediaDevices.getUserMedia = async () => {
  window.spy.mics += 1;
  const made = new RealContext();
  const out = made.createMediaStreamDestination();
  const tone = made.createOscillator();
  tone.connect(out);
  realStart.call(tone);
  return out.stream;
};`;
const MIC_SILENT = `
navigator.mediaDevices.getUserMedia = async () => {
  window.spy.mics += 1;
  return new RealContext().createMediaStreamDestination().stream;
};`;
const MIC_NEVER = `
navigator.mediaDevices.getUserMedia = () => {
  window.spy.mics += 1;
  return new Promise(() => {});
};`;
const NO_CONTEXT = "delete window.AudioContext; window.AudioContext = undefined;";
const STILL_CLOCK = `Object.defineProperty(BaseAudioContext.prototype, "currentTime", { get: () => 0 });`;
const BUTTON = '<button class="js-start" type="button">Start the check</button>';
const REPORT_FLAG = "const REPORT_FOR_EVERYONE = false;";
const ANY_KEY = `${REPORT_FLAG}
document.addEventListener("keydown", () => { new AudioContext().createOscillator().start(); });`;

const ARMS = [
  { name: "clean", init: MIC_LOOP, count: "right", mic: true, query: "?report=1" },
  { name: "skip", count: "skip" },
  { name: "C1-lan", origin: "lan" },
  { name: "C2-gone", init: NO_CONTEXT },
  { name: "C3-nowasm", swap: ["index.html", " 'wasm-unsafe-eval'", ""] },
  { name: "C4-still", init: STILL_CLOCK },
  { name: "C5-gain", swap: ["computer.mjs", "const TONE_GAIN = 0.2;", "const TONE_GAIN = 0;"] },
  { name: "C5-pitch", swap: ["computer.mjs", "tone.frequency.value = TONE_HZ;", "tone.frequency.value = TONE_HZ + 200;"] },
  { name: "C6-wrong", count: "wrong" },
  { name: "C10-silent", init: MIC_SILENT, mic: true },
  { name: "C10-never", init: MIC_NEVER, mic: true },
  { name: "P1-anykey", swap: ["layer-check.mjs", REPORT_FLAG, ANY_KEY] },
  { name: "P2-send", send: true },
  { name: "P3-leak", swap: ["report.mjs", "return `${kind} ${index + 1}`;", "return device.label;"], query: "?report=1" },
  { name: "P4-noname", swap: ["index.html", BUTTON, '<button class="js-start" type="button"></button>'] },
  { name: "policy", policyPlant: true },
  { name: "report-query", query: "?report=1", copy: true },
  { name: "report-everyone", swap: ["layer-check.mjs", REPORT_FLAG, "const REPORT_FOR_EVERYONE = true;"], copy: true },
];

const lines = [];
function out(line) {
  lines.push(line);
  process.stdout.write(`${line}\n`);
}

function servedFile(folder, path, arm) {
  const name = path === "/" ? "index.html" : path.slice(1);
  const type = TYPES[name.slice(name.lastIndexOf("."))];
  if (!type) return null;
  let text;
  try {
    text = readFileSync(join(folder, name), "utf8");
  } catch {
    return null;
  }
  for (const [file, from, to] of arm.swaps ?? []) {
    if (file !== name) continue;
    if (!text.includes(from)) throw new Error(`plant matches nothing: ${arm.name}`);
    text = text.replace(from, () => to);
  }
  return { type, text };
}

function makeRoute(state) {
  return (request, response) => {
    const path = request.url.split("?")[0];
    state.hits.push(path);
    const file = servedFile(state.folder, path, state.arm);
    if (!file) {
      response.writeHead(404).end();
      return;
    }
    response.writeHead(200, { "Content-Type": file.type }).end(file.text);
  };
}

function listen(server, host) {
  return new Promise((done) => {
    server.listen(0, host, () => done(server.address().port));
  });
}

function lanAddress() {
  for (const list of Object.values(networkInterfaces())) {
    for (const { family, internal, address } of list) {
      if (family === "IPv4" && !internal && !address.startsWith("172.")) return address;
    }
  }
  throw new Error("no LAN address on this box");
}

function launch(playwright, kind) {
  if (kind === "firefox") {
    return playwright.firefox.launch({
      headless: true,
      firefoxUserPrefs: FIREFOX_PREFS,
      executablePath: FIREFOX_PATH,
    });
  }
  const channel = kind === "chrome" ? { channel: "chrome" } : {};
  return playwright.chromium.launch({ headless: true, args: CHROME_ARGS, ...channel });
}

async function axeNames(page, source) {
  await page.evaluate(source);
  const found = await page.evaluate(
    async (values) =>
      (await window.axe.run(document, { runOnly: { type: "tag", values } })).violations.map(
        (violation) => violation.id,
      ),
    FLOOR,
  );
  return found;
}

const clickWhenReady = (page, selector) =>
  page.click(selector, { timeout: WAIT_MS });

async function answerCount(page, how) {
  await clickWhenReady(page, ".js-beeps");
  await page.waitForSelector("body[data-beeped]", { state: "attached", timeout: WAIT_MS });
  if (how === "skip") {
    await clickWhenReady(page, ".js-count-skip");
    return;
  }
  const played = await page.evaluate(
    () => window.spy.starts.filter((hz) => hz === 880).length,
  );
  const answer = how === "right" ? played : played === 2 ? 3 : 2;
  await clickWhenReady(page, `.js-count-${answer}`);
}

async function sendPlants(page, otherPort) {
  await page.evaluate((port) => {
    const base = `http://127.0.0.2:${port}`;
    void fetch(`${base}/planted-fetch`).catch(() => {});
    navigator.sendBeacon(`${base}/planted-beacon`, "x");
    new Image().src = `${base}/planted-image`;
  }, otherPort);
  await page.waitForTimeout(SETTLE_MS);
}

async function readRows(page) {
  return page.evaluate(() =>
    [...document.querySelectorAll("[data-check]")].map((row) => ({
      check: row.dataset.check,
      state: row.dataset.state,
      words: row.textContent,
    })),
  );
}

async function runArm(ctx, kind, armDef) {
  const { playwright, servers, axeSource, otherPort, lan } = ctx;
  const arm = { ...armDef, swaps: armDef.swap ? [armDef.swap] : [] };
  if (arm.policyPlant) {
    arm.swaps.push([
      "index.html",
      "<title>",
      `<base href="http://127.0.0.2:${otherPort}/" /><style>main { color: rgb(1, 2, 3); }</style><title>`,
    ]);
  }
  servers.state.arm = arm;
  servers.state.hits = [];
  servers.other.hits = [];
  const browser = await launch(playwright, kind);
  const context = await browser.newContext({ ignoreHTTPSErrors: true });
  await context.addInitScript(`(() => {${SPY}\n${arm.init ?? ""}\n})();`);
  const page = await context.newPage();
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  const origin = arm.origin === "lan" ? `http://${lan}:${servers.lanPort}` : `https://127.0.0.1:${servers.tlsPort}`;
  await page.goto(`${origin}/index.html${arm.query ?? ""}`);
  await page.keyboard.press("Tab");
  await page.keyboard.press("a");
  await page.waitForTimeout(SETTLE_MS);
  const before = await page.evaluate(() => ({
    contexts: window.spy.contexts,
    starts: window.spy.starts.length,
    mics: window.spy.mics,
  }));
  const styled = await page.evaluate(() => ({
    maxInline: getComputedStyle(document.querySelector("main")).maxInlineSize,
    color: getComputedStyle(document.querySelector("main")).color,
    baseIsOwn: document.baseURI === window.location.href,
  }));
  const axeAtLoad = await axeNames(page, axeSource);
  const everyone = await page.evaluate(() => !document.querySelector(".js-report").hidden);
  if (arm.name === "P4-noname") await page.evaluate(() => document.querySelector(".js-start").click());
  else await clickWhenReady(page, ".js-start");
  await page.waitForSelector("body[data-ran]", { state: "attached", timeout: WAIT_MS });
  if (arm.count) await answerCount(page, arm.count);
  let micMs = null;
  if (arm.mic) {
    const began = Date.now();
    await clickWhenReady(page, ".js-mic-start");
    await page.waitForSelector("body[data-mic-done]", { state: "attached", timeout: WAIT_MS });
    micMs = Date.now() - began;
  }
  if (arm.send) await sendPlants(page, otherPort);
  let copyRoute = null;
  if (arm.copy) {
    await clickWhenReady(page, ".js-copy");
    await page.waitForSelector(".js-copy[data-route]", { state: "attached", timeout: WAIT_MS });
    copyRoute = await page.evaluate(() => document.querySelector(".js-copy").dataset.route);
  }
  const axeAfter = await axeNames(page, axeSource);
  await page.waitForTimeout(SETTLE_MS);
  const rows = await readRows(page);
  const after = await page.evaluate(() => ({
    violations: window.spy.violations,
    secure: window.isSecureContext,
    report: document.querySelector(".js-report-text").textContent,
  }));
  await browser.close();
  out(JSON.stringify({
    browser: kind,
    arm: arm.name,
    rows,
    before,
    styled,
    axeAtLoad,
    axeAfter,
    everyone,
    micMs,
    copyRoute,
    violations: after.violations,
    secure: after.secure,
    reportHasLabel: after.report.includes(FAKE_LABEL),
    report: after.report,
    hits: [...new Set(servers.state.hits)].sort(),
    otherHits: servers.other.hits,
    errors,
  }));
}

async function main() {
  const [folder, outDir, tempDir] = process.argv.slice(2);
  mkdirSync(outDir, { recursive: true });
  mkdirSync(tempDir, { recursive: true });
  const key = join(tempDir, "key.pem");
  const cert = join(tempDir, "cert.pem");
  execFileSync(
    "openssl",
    ["req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout", key, "-out", cert, "-days", "1", "-subj", "/CN=localhost"],
    { stdio: "ignore" },
  );
  const state = { folder, arm: {}, hits: [] };
  const route = makeRoute(state);
  const tls = tlsServer({ key: readFileSync(key), cert: readFileSync(cert) }, route);
  const plain = plainServer(route);
  const lan = lanAddress();
  const other = { hits: [] };
  const otherServer = plainServer((request, response) => {
    other.hits.push(request.url);
    response.writeHead(204).end();
  });
  const servers = {
    state,
    other,
    tlsPort: await listen(tls, "127.0.0.1"),
    lanPort: await listen(plain, lan),
  };
  const otherPort = await listen(otherServer, "127.0.0.2");
  const playwright = await import(pathToFileURL(process.env.PLAYWRIGHT).href);
  const axeSource = readFileSync(process.env.AXE, "utf8");
  const ctx = { playwright, servers, axeSource, otherPort, lan };
  try {
    for (const kind of ["chromium", "chrome", "firefox"]) {
      for (const arm of ARMS) {
        try {
          await runArm(ctx, kind, arm);
        } catch (error) {
          out(JSON.stringify({ browser: kind, arm: arm.name, crashed: error.message }));
        }
      }
    }
  } finally {
    writeFileSync(join(outDir, "layer-check-output.jsonl"), `${lines.join("\n")}\n`);
    tls.close();
    plain.close();
    otherServer.close();
  }
}

await main();
