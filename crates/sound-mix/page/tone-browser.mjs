// SPDX-License-Identifier: MIT
// The page in a real browser, with the real autoplay gate: Chromium muted by its own
// flag, so no sound leaves this box. `page.evaluate` only reads, but for one `hush`,
// which is how the check ends the frames so it can add them up. Input goes by key
// presses and pointer moves. Each part has its own load and its own browser context,
// so one part's gesture never carries into the next. Exits non-zero at the first
// check that fails, the output so far kept.
//
// A green run is not a conformance claim: no person walked it, no screen reader ran,
// and no one heard it. Axe is necessary, never sufficient.
//
// Call: PLAYWRIGHT=<playwright's index.mjs> [AXE=<axe.min.js>]
//   node tone-browser.mjs <page_tone.wasm> [<output folder>]
// With no AXE the gate says it did not run. Writes tone-output.txt to the folder.
import assert from "node:assert/strict";
import { readFile, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const HERE = fileURLToPath(new URL(".", import.meta.url));
const SERVED = {
  "/tone.html": ["tone.html", "text/html"],
  "/tone.css": ["tone.css", "text/css"],
  "/tone.mjs": ["tone.mjs", "text/javascript"],
  "/sound.mjs": ["sound.mjs", "text/javascript"],
  "/logger.mjs": ["logger.mjs", "text/javascript"],
};
const FLOOR = ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"];
const ARGS = ["--mute-audio", "--autoplay-policy=document-user-activation-required"];
const QUIET_MS = 400;
const ROUNDING_S = 1e-9;
const HIDDEN_MS = 3000;

function serve(wasmPath) {
  const server = createServer(async (request, response) => {
    const path = request.url.split("?")[0];
    const [file, type] =
      path === "/page_tone.wasm"
        ? [wasmPath, "application/wasm"]
        : (SERVED[path] ?? []);
    if (!file) {
      response.writeHead(404).end();
      return;
    }
    const bytes = await readFile(file === wasmPath ? file : join(HERE, file));
    response.writeHead(200, { "Content-Type": type }).end(bytes);
  });
  return new Promise((done) => {
    server.listen(0, "127.0.0.1", () => done(server));
  });
}

// Runs before the page's own scripts: what the browser was really asked to do, kept
// apart from what the script says it did.
function spy() {
  const seen = { nodes: [], prevented: 0 };
  window.toneSpy = seen;
  const start = AudioBufferSourceNode.prototype.start;
  AudioBufferSourceNode.prototype.start = function (when, ...rest) {
    seen.nodes.push({
      when,
      now: this.context.currentTime,
      frames: this.buffer.length,
      rate: this.buffer.sampleRate,
    });
    return start.call(this, when, ...rest);
  };
  const prevent = Event.prototype.preventDefault;
  Event.prototype.preventDefault = function () {
    seen.prevented += 1;
    return prevent.call(this);
  };
}

const read = (page) =>
  page.evaluate(() => {
    const { wasm, sound } = window.toneProof;
    const { nodes, prevented } = window.toneSpy;
    return {
      state: sound.state,
      muted: sound.muted,
      now: sound.now,
      stats: { ...sound.stats },
      kept: wasm.kept(),
      clock: wasm.clock_ms(),
      ended: wasm.short_ended(),
      nodes: nodes.length,
      given: nodes.reduce((sum, node) => sum + node.frames, 0),
      prevented,
      told: document.querySelector(".js-state").textContent,
      visible: document.visibilityState,
      focus: document.activeElement.tagName,
    };
  });

const waitFor = (page, call) => page.waitForFunction(call, null, { timeout: 5000 });
const running = (page) =>
  waitFor(page, () => window.toneProof.sound.state === "running");

async function load(browser, port, axePath) {
  const context = await browser.newContext({
    viewport: { width: 800, height: 600 },
  });
  const page = await context.newPage();
  await page.addInitScript(spy);
  const errors = [];
  page.on("pageerror", (error) => errors.push(String(error)));
  await page.goto(`http://127.0.0.1:${port}/tone.html`);
  await page.waitForSelector("html[data-ready]");
  if (axePath) await page.addScriptTag({ path: axePath });
  return { context, page, errors };
}

async function gate(page, out, axePath, state) {
  if (!axePath) {
    out(`  axe, ${state}: NOT RUN, no AXE given`);
    return;
  }
  const found = await page.evaluate(
    async (values) =>
      (
        await window.axe.run(document, { runOnly: { type: "tag", values } })
      ).violations.map(({ id, nodes }) => [id, nodes.map((n) => n.target)]),
    FLOOR,
  );
  out(`  axe, ${state}: ${found.length} violations ${JSON.stringify(found)}`);
  assert.deepEqual(found, [], `axe, ${state}`);
}

// Before a gesture, Escape, then a key.
async function theKey(browser, port, out, axePath) {
  out("the key, on its own load:");
  const { context, page, errors } = await load(browser, port, axePath);
  const before = await read(page);
  assert.equal(before.state, "suspended", "the real gate holds the context");
  await waitFor(page, () => window.toneProof.wasm.short_ended() === 1);
  await page.waitForTimeout(QUIET_MS);
  const waited = await read(page);
  out(
    `  before any gesture: ${waited.state}, ${waited.nodes} nodes, clock ${before.clock.toFixed(0)} -> ${waited.clock.toFixed(0)} ms, short voice ended ${waited.ended}, kept ${waited.kept}`,
  );
  assert.equal(waited.state, "suspended");
  assert.equal(waited.nodes, 0, "0 nodes started");
  assert.ok(waited.clock > before.clock, "the program's clock moves");
  assert.equal(waited.ended, 1, "the short voice ended unheard");
  assert.equal(waited.kept, 0, "nothing kept to replay");
  await gate(page, out, axePath, "before the gesture");

  await page.keyboard.press("Escape");
  await page.waitForTimeout(QUIET_MS);
  const escaped = await read(page);
  out(`  Escape: ${escaped.state}, ${escaped.nodes} nodes`);
  assert.equal(escaped.state, "suspended", "Escape starts nothing");
  assert.equal(escaped.nodes, 0);

  await page.keyboard.press("a");
  await running(page);
  await waitFor(page, () => window.toneSpy.nodes.length > 0);
  const first = await page.evaluate(() => window.toneSpy.nodes[0]);
  const blockS = 1024 / first.rate;
  out(
    `  a key: running; the first node at ${first.when.toFixed(4)} s, the context at ${first.now.toFixed(4)} s, one block ${blockS.toFixed(4)} s, ${first.frames} frames at ${first.rate}`,
  );
  assert.ok(first.when >= first.now, "it does not start in the past");
  assert.ok(first.when <= first.now + blockS + ROUNDING_S, "within one block");

  await page.waitForTimeout(1000);
  await page.evaluate(() => window.toneProof.wasm.hush());
  await waitFor(page, () => {
    const { wasm, sound } = window.toneProof;
    const { nodes } = window.toneSpy;
    const given = nodes.reduce((sum, node) => sum + node.frames, 0);
    return wasm.kept() > 0 && given + sound.stats.dropped === wasm.kept();
  });
  const done = await read(page);
  out(
    `  after 1 s and a hush: kept ${done.kept}, given to ${done.nodes} nodes ${done.given}, the script says ${done.stats.frames}, the planner dropped ${done.stats.dropped}`,
  );
  assert.equal(done.given, done.kept, "frames given equal frames kept");
  assert.equal(done.stats.frames, done.given, "the script and the spy agree");
  const nodes = await page.evaluate(() => window.toneSpy.nodes);
  const overlaps = nodes.filter(
    (node, at) =>
      at > 0 &&
      node.when <
        nodes[at - 1].when + nodes[at - 1].frames / node.rate - ROUNDING_S,
  );
  const gaps = nodes.filter(
    (node, at) =>
      at > 0 &&
      node.when >
        nodes[at - 1].when + nodes[at - 1].frames / node.rate + ROUNDING_S,
  );
  out(`  ${overlaps.length} overlaps, ${gaps.length} gaps among ${nodes.length} nodes`);
  assert.equal(overlaps.length, 0, "no node overlaps the one before");
  assert.equal(done.prevented, 0, "no listener called preventDefault");
  assert.equal(done.focus, "BODY", "the state moved no focus");
  assert.equal(done.told, "Sound is on.");
  await gate(page, out, axePath, "after the gesture");
  assert.deepEqual(errors, [], "no page error");
  await context.close();
}

// A pointer counts at `click`, and never at a down-event.
async function theClick(browser, port, out) {
  out("the click, on its own load:");
  const { context, page, errors } = await load(browser, port);
  await page.mouse.move(400, 500);
  await page.mouse.down();
  await page.waitForTimeout(QUIET_MS);
  const held = await read(page);
  out(`  the pointer down: ${held.state}, ${held.nodes} nodes`);
  assert.equal(held.state, "suspended", "a down-event starts nothing");
  assert.equal(held.nodes, 0);
  await page.mouse.up();
  await running(page);
  await waitFor(page, () => window.toneSpy.nodes.length > 0);
  const clicked = await read(page);
  out(`  the click: ${clicked.state}, ${clicked.nodes} nodes`);
  assert.equal(clicked.prevented, 0);
  assert.deepEqual(errors, [], "no page error");
  await context.close();
}

// The mute by Tab and Space, and out of it again by Tab.
async function theMute(browser, port, out) {
  out("the mute, by the keyboard alone, on its own load:");
  const { context, page, errors } = await load(browser, port);
  const button = () => page.getByRole("button", { name: "Mute" });
  const focused = () =>
    page.evaluate(() => document.activeElement.classList.contains("js-mute"));
  await page.keyboard.press("Tab");
  assert.ok(await focused(), "Tab reaches the mute");
  await running(page);
  const tabbed = await read(page);
  out(`  Tab: on the mute, ${tabbed.state}: Tab is a key, so it started the sound`);
  await page.keyboard.press("Space");
  const pressed = await read(page);
  assert.equal(await button().getAttribute("aria-pressed"), "true");
  assert.equal(pressed.muted, true);
  assert.equal(pressed.told, "Sound is on, muted.");
  assert.ok(await focused(), "the focus stays on the mute");
  assert.equal(await page.getByRole("status").textContent(), pressed.told);
  out(`  Space: pressed, the status says "${pressed.told}", the focus unmoved`);
  await page.keyboard.press("Space");
  const again = await read(page);
  assert.equal(await button().getAttribute("aria-pressed"), "false");
  assert.equal(again.muted, false);
  out(`  Space: not pressed, the status says "${again.told}"`);
  await page.keyboard.press("Tab");
  assert.equal(await focused(), false, "Tab leaves it: no trap");
  out("  Tab: off the mute, no trap");
  const end = await read(page);
  assert.equal(end.prevented, 0, "no listener called preventDefault");
  assert.deepEqual(errors, [], "no page error");
  await context.close();
}

// A hidden tab, measured once. A second tab at the front is what a person
// does; if this browser keeps the first one visible, that is said, and owed.
async function theHiddenTab(browser, port, out) {
  out("a hidden tab, measured once:");
  const { context, page, errors } = await load(browser, port);
  await page.keyboard.press("a");
  await running(page);
  await page.waitForTimeout(1000);
  const shown = await read(page);
  const other = await context.newPage();
  await other.goto("about:blank");
  await other.bringToFront();
  await new Promise((done) => setTimeout(done, HIDDEN_MS));
  const hidden = await read(page);
  out(
    `  shown:  ${shown.visible}, reads ${shown.stats.reads}, kept ${shown.kept}, given ${shown.given}, context ${shown.now.toFixed(3)} s, program clock ${shown.clock.toFixed(0)} ms`,
  );
  out(
    `  after ${HIDDEN_MS} ms behind another tab: ${hidden.visible}, reads ${hidden.stats.reads}, kept ${hidden.kept}, given ${hidden.given}, context ${hidden.now.toFixed(3)} s, program clock ${hidden.clock.toFixed(0)} ms`,
  );
  if (hidden.visible !== "hidden") {
    out(
      "  OWED, never a pass: this headless browser kept the tab visible, so a hidden tab was not measured",
    );
  }
  await page.bringToFront();
  await other.close();
  await page.waitForTimeout(1000);
  const back = await read(page);
  out(
    `  1 s after coming back: ${back.visible}, reads ${back.stats.reads}, kept ${back.kept}, given ${back.given}, planner dropped ${back.stats.dropped}, context ${back.now.toFixed(3)} s, program clock ${back.clock.toFixed(0)} ms`,
  );
  assert.deepEqual(errors, [], "no page error");
  await context.close();
}

async function main() {
  const [wasmPath, folder] = process.argv.slice(2);
  const lines = [];
  const out = (line) => {
    lines.push(line);
    console.log(line);
  };
  const server = await serve(wasmPath);
  const { port } = server.address();
  const { chromium } = await import(pathToFileURL(process.env.PLAYWRIGHT));
  const browser = await chromium.launch({ headless: true, args: ARGS });
  let failed = false;
  try {
    const axePath = process.env.AXE;
    out(`chromium ${browser.version()}, axe ${axePath ? "on" : "off"}`);
    const module = await WebAssembly.compile(await readFile(wasmPath));
    assert.deepEqual(WebAssembly.Module.imports(module), [], "the wasm imports nothing");
    out("the wasm imports nothing");
    await theKey(browser, port, out, axePath);
    await theClick(browser, port, out);
    await theMute(browser, port, out);
    await theHiddenTab(browser, port, out);
    out("tone: every check held. Owed: anyone hearing it, a person's walk, a screen reader");
  } catch (error) {
    failed = true;
    out(`tone FAILED: ${error.message}`);
  } finally {
    await browser.close();
    server.close();
    if (folder) {
      await writeFile(join(folder, "tone-output.txt"), `${lines.join("\n")}\n`);
    }
  }
  process.exitCode = failed ? 1 : 0;
}

await main();
