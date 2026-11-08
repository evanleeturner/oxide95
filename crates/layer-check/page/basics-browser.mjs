#!/usr/bin/env node
// SPDX-License-Identifier: MIT
// `basics-page.mjs` in headless Chromium, once clean five times and once per planted fault. A
// plant is an exact text swap the server makes in a served file, so no plant ships; a swap
// that matches other than once stops the run. It writes what each arm read, one file an arm,
// and a separate script scores them.
//
// `CC1`: `basics-page.mjs`'s four rows as one check, scored outside from what each arm read.
//
// `127.0.0.1` is a secure origin, and the 2 headers isolate the page: without both a page
// gets no `SharedArrayBuffer`, as one arm shows. Run it inside a private sound server: the
// flag `launch` passes keeps no stream from opening.
//
// Call: PLAYWRIGHT=<playwright's index.mjs> node basics-browser.mjs <out folder>
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import { launch, listen, playwrightFromEnv } from "./browser.mjs";

const PAGE = "basics-page.mjs";
const WORKER = "basics-worker.mjs";
const HOME_PAGE = `<!doctype html><html lang="en"><title>Computer basics</title><script type="module" src="/${PAGE}"></script></html>`;
const ISOLATING = {
  "Cross-Origin-Opener-Policy": "same-origin",
  "Cross-Origin-Embedder-Policy": "require-corp",
};
const WAIT_MS = 120000;

const CLEAN_RUNS = 5;
const ARMS = [
  ...Array.from({ length: CLEAN_RUNS }, (_, at) => ({ name: `clean-${at + 1}` })),
  {
    name: "clock",
    swaps: [[PAGE, "const firstClock = () => performance.now();", "const firstClock = () => 0;"]],
  },
  {
    name: "file",
    swaps: [
      [
        PAGE,
        "const back = new Uint8Array(await (await handle.getFile()).arrayBuffer());",
        "const back = new Uint8Array(await (await handle.getFile()).arrayBuffer()); back[FILE_BYTES / 2] ^= 1;",
      ],
    ],
  },
  {
    name: "thread",
    swaps: [
      [WORKER, "const lockFor = (cells) => cells.lock;", "const lockFor = (cells, side) => cells.lock + side;"],
    ],
  },
  {
    name: "heap",
    swaps: [
      [
        PAGE,
        "view.set(pattern(BLOCK_BYTES, 3), BLOCK_BYTES);",
        "view.set(pattern(BLOCK_BYTES, 3), BLOCK_BYTES); view[BLOCK_BYTES / 2] ^= 1;",
      ],
    ],
  },
  {
    name: "import",
    swaps: [
      [
        PAGE,
        "const WAIT_MS = 250;",
        'await import("/crates/kernel-time/page/planted.mjs").catch(() => null);\nconst WAIT_MS = 250;',
      ],
    ],
  },
  { name: "headers-off", bare: true },
];

function pageFile(name, arm) {
  let text = readFileSync(fileURLToPath(new URL(name, import.meta.url)), "utf8");
  for (const [file, from, to] of arm.swaps ?? []) {
    if (file !== name) continue;
    const matches = text.split(from).length - 1;
    if (matches !== 1) throw new Error(`${arm.name} matches ${matches} times`);
    text = text.replace(from, () => to);
  }
  return text;
}

function route(state) {
  return (request, response) => {
    const path = request.url.split("?")[0];
    state.asked.push(path);
    const headers = state.arm.bare ? {} : ISOLATING;
    if (path === "/") {
      response.writeHead(200, { ...headers, "Content-Type": "text/html" }).end(HOME_PAGE);
    } else if (path === `/${PAGE}` || path === `/${WORKER}`) {
      response
        .writeHead(200, { ...headers, "Content-Type": "text/javascript" })
        .end(pageFile(path.slice(1), state.arm));
    } else {
      response.writeHead(404, headers).end();
    }
  };
}

/** Everything the page said, and every path it asked for, in the arm's own browser. */
async function runArm(browser, port, state, arm) {
  state.arm = arm;
  state.asked = [];
  const context = await browser.newContext();
  const page = await context.newPage();
  await page.goto(`http://127.0.0.1:${port}/`);
  const lines = await page.evaluate(
    (limit) =>
      Promise.race([
        window.basics,
        new Promise((_, failed) =>
          setTimeout(() => failed(new Error("the page did not finish")), limit),
        ),
      ]),
    WAIT_MS,
  );
  await context.close();
  return [
    `CC1 browser chromium ${browser.version()}`,
    ...(lines ?? ["CC1 refused the page's module did not run"]),
    ...state.asked.map((path) => `CC1 asked ${path}`),
  ];
}

async function main() {
  const out = process.argv[2];
  mkdirSync(out, { recursive: true });
  const state = { arm: {}, asked: [] };
  const server = createServer(route(state));
  const port = await listen(server, "127.0.0.1");
  const browser = await launch(await playwrightFromEnv(), "chromium");
  try {
    for (const arm of ARMS) {
      const lines = await runArm(browser, port, state, arm);
      writeFileSync(join(out, `p-${arm.name}.txt`), `${lines.join("\n")}\n`);
    }
  } finally {
    await browser.close();
    server.close();
  }
}

await main();
