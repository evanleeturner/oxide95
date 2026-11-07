// SPDX-License-Identifier: MIT
// The page in a real browser, walked by the keyboard alone: Tab,
// Space, Return, Escape. It acts by key presses only; `page.evaluate` only reads.
// One browser and one page, so no workers. Exits non-zero at the first check that
// fails, the output so far kept. A green walk is not a conformance claim: no
// person walked it and no screen reader ran; the tree read is the browser's own.
//
// Call: PLAYWRIGHT=<playwright's index.mjs> [AXE=<axe.min.js>]
//   node walk-browser.mjs <page_walk.wasm> [<output folder>]
// With no AXE the gate says it did not run. Writes walk-output.txt and
// walk-ax.json to the folder.
import assert from "node:assert/strict";
import { readFile, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

import { boxFromText } from "./walk-records.mjs";

const HERE = fileURLToPath(new URL(".", import.meta.url));
const SERVED = {
  "/walk.html": ["walk.html", "text/html"],
  "/walk.css": ["walk.css", "text/css"],
  "/walk.mjs": ["walk.mjs", "text/javascript"],
  "/walk-records.mjs": ["walk-records.mjs", "text/javascript"],
  "/logger.mjs": ["logger.mjs", "text/javascript"],
};
const NONE = 2 ** 32 - 1;
const [IDOK, IDCANCEL, FIRST_CHOICE] = [1, 2, 0x30];
const CANVAS = ["application", "The program's screen"];
const NOT_NAMES = new Set(["none", "generic", "InlineTextBox", "LineBreak"]);
const FLOOR = ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"];
const REFLOW_WIDTH = 320;

function serve(wasmPath) {
  const server = createServer(async (request, response) => {
    const path = request.url.split("?")[0];
    const [file, type] =
      path === "/page_walk.wasm"
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

function axRow(node) {
  const row = { role: node.role.value, name: node.name?.value ?? "" };
  for (const { name, value } of node.properties ?? []) {
    if (
      ["focused", "checked", "expanded", "disabled", "pressed"].includes(name)
    ) {
      row[name] = value.value;
    }
  }
  return row;
}

async function axTree(cdp) {
  const { nodes } = await cdp.send("Accessibility.getFullAXTree");
  return nodes
    .filter((node) => !node.ignored && !NOT_NAMES.has(node.role?.value))
    .filter((node) => node.role)
    .map(axRow);
}

const named = (tree, role, name) =>
  tree.find((row) => row.role === role && row.name === name);

async function walk(page, cdp, out, kept, axePath) {
  const read = (name) =>
    page.evaluate((exportName) => window.pageWalk[exportName]() >>> 0, name);

  const focused = async () =>
    (await axTree(cdp))
      .filter((row) => row.focused && row.role !== "RootWebArea")
      .at(-1) ?? { role: "none", name: "" };

  const go = async (key, role, name) => {
    await page.keyboard.press(key);
    const at = await focused();
    out(`  ${key.padEnd(10)} -> ${at.role} "${at.name}"`);
    assert.deepEqual([at.role, at.name], [role, name], `after ${key}`);
  };

  const keep = async (state) => {
    const tree = await axTree(cdp);
    kept[state] = tree;
    return tree;
  };

  const gate = async (state) => {
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
  };

  const reflows = async (state) => {
    const size = page.viewportSize();
    await page.setViewportSize({ width: REFLOW_WIDTH, height: 480 });
    const wide = await page.evaluate(
      () => document.documentElement.scrollWidth,
    );
    await page.setViewportSize(size);
    out(`  at ${REFLOW_WIDTH} px, ${state}: page ${wide} px wide`);
    assert.ok(wide <= REFLOW_WIDTH, `reflow, ${state}`);
  };

  const closedWith = async (value) => {
    assert.equal(await read("ended"), value, "the box's EndDialog value");
    assert.equal(
      await page.evaluate(() => document.querySelector("dialog").open),
      false,
      "the dialog closed",
    );
    const at = await focused();
    assert.deepEqual([at.role, at.name], CANVAS, "focus back on the canvas");
    out(`  closed with ${value}, focus back on the canvas`);
  };

  const opened = async () => {
    assert.equal(
      await page.evaluate(() => document.querySelector("dialog").open),
      true,
      "the dialog opened",
    );
    const at = await focused();
    assert.deepEqual([at.role, at.name], ["button", "OK"], "focus on open");
    const desk = await page.evaluate(() => [
      Number(document.activeElement.dataset.handle),
      window.pageWalk.focused() >>> 0,
    ]);
    assert.equal(desk[0], desk[1], "the desk's focus is the element's");
    out("  opened, focus on OK, the desk agrees");
  };

  const openFromCanvas = async (openKey) => {
    await go("Shift+Tab", "button", "Help");
    await go("Shift+Tab", "button", "File");
    await go(openKey, "button", "File");
    await go("Tab", "button", "Open");
    await page.keyboard.press(openKey);
    out(`  ${openKey.padEnd(10)} on Open`);
    await opened();
  };

  const crateChecked = async () => {
    const text = await page.evaluate(() => {
      const wasm = window.pageWalk;
      const { handle } = document.querySelector("dialog").dataset;
      const length = wasm.box_text(Number(handle));
      return new TextDecoder().decode(
        new Uint8Array(wasm.memory.buffer, wasm.text_ptr(), length),
      );
    });
    const { controls } = boxFromText(text);
    return controls.find((control) => control.id === FIRST_CHOICE).checked;
  };

  out(
    "The bar, by Tab and Space; Save is grey and skipped; Escape gives the canvas",
  );
  await go("Tab", "button", "File");
  let tree = await keep("bar-closed");
  assert.equal(named(tree, "button", "File").expanded, false);
  assert.equal(named(tree, "button", "Open"), undefined, "rows hidden");
  await gate("bar closed");
  await go("Space", "button", "File");
  tree = await keep("bar-open");
  assert.equal(named(tree, "button", "File").expanded, true);
  assert.ok(named(tree, "button", "Open"), "Open named by its words");
  assert.equal(named(tree, "button", "Save").disabled, true, "Save is grey");
  assert.equal(named(tree, "button", "Keep").pressed, "true", "Keep is ticked");
  assert.ok(named(tree, "navigation", "Menu bar"));
  await gate("bar open");
  await reflows("bar open");
  await go("Tab", "button", "Open");
  await go("Tab", "button", "Keep");
  await go("Tab", "button", "Help");
  await go("Shift+Tab", "button", "Keep");
  await go("Escape", ...CANVAS);
  tree = await keep("after-bar-escape");
  assert.equal(named(tree, "button", "File").expanded, false);
  assert.equal(await read("picked"), NONE, "Escape picked nothing");

  out(
    "A row with no box, by Return: the pick is kept, the canvas has the focus",
  );
  await go("Shift+Tab", "button", "Help");
  await go("Enter", "button", "Help");
  await go("Tab", "button", "About");
  await go("Enter", ...CANVAS);
  assert.equal(await read("picked"), 0x13, "the program kept About's id");

  out("The box by Space; each control named; Tab, Shift+Tab, Space; Escape");
  await openFromCanvas("Space");
  tree = await keep("box-open");
  for (const [role, name] of [
    ["dialog", "A planted box"],
    ["button", "OK"],
    ["button", "Cancel"],
    ["StaticText", "A planted label"],
    ["checkbox", "First choice"],
    ["checkbox", "Second choice"],
  ]) {
    assert.ok(named(tree, role, name), `${role} named "${name}"`);
    out(`  named: ${role} "${name}"`);
  }
  assert.equal(named(tree, "button", "File"), undefined, "the bar is inert");
  assert.equal(named(tree, ...CANVAS), undefined, "the canvas is inert");
  await gate("box open");
  await reflows("box open");
  await go("Tab", "button", "Cancel");
  await go("Tab", "checkbox", "First choice");
  await go("Space", "checkbox", "First choice");
  assert.equal(
    named(await axTree(cdp), "checkbox", "First choice").checked,
    "true",
  );
  assert.equal(await crateChecked(), true, "the crate holds the check");
  assert.equal(await read("ended"), NONE, "a check closes nothing");
  await go("Tab", "checkbox", "Second choice");
  await go("Shift+Tab", "checkbox", "First choice");
  await go("Space", "checkbox", "First choice");
  assert.equal(await crateChecked(), false, "the crate holds it cleared");
  await go("Tab", "checkbox", "Second choice");
  await page.keyboard.press("Tab");
  const past = await focused();
  out(`  Tab past the last -> ${past.role} "${past.name}"`);
  const inside = await page.evaluate(
    () =>
      document.querySelector("dialog").contains(document.activeElement) ||
      document.activeElement === document.body,
  );
  assert.ok(inside, "Tab past the last stays in the box or on the page's edge");
  if (past.name !== "OK") await go("Tab", "button", "OK");
  await go("Escape", ...CANVAS);
  await closedWith(IDCANCEL);

  out("Return on OK, the default");
  await openFromCanvas("Enter");
  await go("Enter", ...CANVAS);
  await closedWith(IDOK);

  out("Return on a focused Cancel: the button takes it, never the default");
  await openFromCanvas("Space");
  await go("Tab", "button", "Cancel");
  await go("Enter", ...CANVAS);
  await closedWith(IDCANCEL);

  out("Space on a focused Cancel");
  await openFromCanvas("Space");
  await go("Tab", "button", "Cancel");
  await go("Space", ...CANVAS);
  await closedWith(IDCANCEL);

  out("Space on OK");
  await openFromCanvas("Space");
  await go("Space", ...CANVAS);
  await closedWith(IDOK);

  out("Return on a check: the default's; a new box opens unchecked");
  await openFromCanvas("Space");
  await go("Tab", "button", "Cancel");
  await go("Tab", "checkbox", "First choice");
  assert.equal(await crateChecked(), false, "the box opens as its template");
  await go("Enter", ...CANVAS);
  await closedWith(IDOK);
}

async function main() {
  const [wasmPath, folder] = process.argv.slice(2);
  const lines = [];
  const out = (line) => {
    lines.push(line);
    console.log(line);
  };
  const kept = {};
  const server = await serve(wasmPath);
  const { chromium } = await import(pathToFileURL(process.env.PLAYWRIGHT));
  const browser = await chromium.launch({ headless: true });
  let failed = false;
  try {
    const page = await browser.newPage({
      viewport: { width: 800, height: 700 },
    });
    const errors = [];
    page.on("pageerror", (error) => errors.push(String(error)));
    page.on("console", (message) => out(`  page: ${message.text()}`));
    const { port } = server.address();
    await page.goto(`http://127.0.0.1:${port}/walk.html`);
    await page.waitForSelector("html[data-ready]");
    const axePath = process.env.AXE;
    if (axePath) await page.addScriptTag({ path: axePath });
    const cdp = await page.context().newCDPSession(page);
    await cdp.send("Accessibility.enable");
    out(`chromium ${browser.version()}, axe ${axePath ? "on" : "off"}`);
    await walk(page, cdp, out, kept, axePath);
    assert.deepEqual(errors, [], "no page error");
    out("walk: every check held, by the keyboard alone");
  } catch (error) {
    failed = true;
    out(`walk FAILED: ${error.message}`);
  } finally {
    await browser.close();
    server.close();
    if (folder) {
      await writeFile(join(folder, "walk-output.txt"), `${lines.join("\n")}\n`);
      await writeFile(
        join(folder, "walk-ax.json"),
        `${JSON.stringify(kept, null, 1)}\n`,
      );
    }
  }
  process.exitCode = failed ? 1 : 0;
}

await main();
