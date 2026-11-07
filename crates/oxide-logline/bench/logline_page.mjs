// SPDX-License-Identifier: MIT
// A headless page calls a page's logger, and what reaches the console is judged from outside
// it: each message's text by `logline_grammar.py`, each message's type and arguments here. No
// check reads the logger's own code, and each plant is a copy of a logger, served by its path.
//
// Call: PLAYWRIGHT=<playwright's index.mjs> node logline_page.mjs --work <folder> --out <file>
//   --keep <folder> [--secret-scan <secret_scan.py>]
// Exits 1 when a clean row or a plant does not hold. A `counted` row never moves the exit: a
// page writing another grammar than the crate's is a finding, not a fault of the run. Prints
// and keeps counts; the only message text it keeps is the kept files', scanned by name.
import { spawnSync } from "node:child_process";
import { randomBytes } from "node:crypto";
import { mkdirSync, readFileSync, unlinkSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { parseArgs } from "node:util";

const BENCH = dirname(fileURLToPath(import.meta.url));
const WORKSPACE = resolve(BENCH, "..", "..", "..");
const GRAMMAR = join(BENCH, "logline_grammar.py");
const PAGE_LIMIT_MS = 30_000;
const CANARY_NAME = "LOGLINE_PAGE_CANARY";
const SCAN_PLANT = "API_TOKEN=plantedplantedplanted\n";

// Written here, never taken from the logger or the crate.
const EVENT_ID = /^[a-z]+(\.[a-z]+)+$/;
const CONSOLE_TYPE = { error: "error", warn: "warning", info: "info", debug: "debug" };
const LEVEL_TYPES = new Set(Object.values(CONSOLE_TYPE));

const PAGE = `<!doctype html><html lang="en"><title>logger</title>
<link rel="icon" href="data:,"><script type="module" src="/calls.mjs"></script></html>`;

const CALLS = [
  { level: "info", form: "id", values: ["key.down", { code: "KeyA" }] },
  { level: "info", form: "id", values: ["place.press", { place: "file" }] },
  { level: "info", form: "id", values: ["page.open"] },
  { level: "debug", form: "id", values: ["key.up", { code: "KeyA" }] },
  { level: "info", form: "sentence", values: ["box opened", 7, "A title"] },
  { level: "debug", form: "sentence", values: ["box closed", 7] },
];
const LOUD_CALLS = [
  { level: "warn", form: "id", values: ["track.wasm.late", { ms: 12 }] },
  { level: "error", form: "id", values: ["track.wasm.refused", { reason: "CompileError" }] },
];

// `logger` is the row's name, the same bytes as its sister. `track-logger` is the one with
// WARN and ERROR, which the pages on event ids call.
const LOGGERS = [
  {
    name: "logger",
    path: "crates/window-dialogs/page/logger.mjs",
    sister: "crates/sound-mix/page/logger.mjs",
    calls: CALLS,
  },
  {
    name: "track-logger",
    path: "crates/web-shell/page/track-logger.mjs",
    calls: [...CALLS, ...LOUD_CALLS],
  },
];

const PLANTS = [
  { name: "info-by-log", from: "console.info(", to: "console.log(" },
  { name: "debug-always", from: "if (debugEnabled) console.debug", to: "console.debug" },
  { name: "debug-never", from: "if (debugEnabled) console.debug", to: "if (false) console.debug" },
];

class Score {
  rows = [];

  add(group, name, held, detail = "") {
    this.rows.push({ group, name, held, detail });
    console.log(`${group} ${name} ${held ? "ok" : "RED"} ${detail}`.trimEnd());
  }

  failed() {
    return this.rows.filter((row) => !row.held && row.group !== "counted");
  }

  write(path, version) {
    const lines = ["host page", `browser chromium ${version}`];
    for (const row of this.rows) {
      const word = row.held ? "ok" : "RED";
      lines.push(`${row.group} ${row.name} ${word} ${row.detail}`.trimEnd());
    }
    lines.push(`score rows=${this.rows.length} failed=${this.failed().length}`);
    writeFileSync(path, `${lines.join("\n")}\n`);
  }
}

/** Serves one logger's bytes and a page calling it, on a port of its own. */
function serve(loggerBytes, calls) {
  const script = [
    'import { logger } from "/logger.mjs";',
    `for (const { level, values } of ${JSON.stringify(calls)}) logger[level](...values);`,
    "window.callsDebugEnabled = logger.debugEnabled;",
    "window.callsDone = true;",
  ].join("\n");
  const routes = {
    "/": ["text/html", PAGE],
    "/calls.mjs": ["text/javascript", script],
    "/logger.mjs": ["text/javascript", loggerBytes],
  };
  const server = createServer((request, response) => {
    const route = routes[new URL(request.url, "http://127.0.0.1").pathname];
    if (route === undefined) {
      response.writeHead(404);
      response.end();
      return;
    }
    response.writeHead(200, { "content-type": route[0] });
    response.end(route[1]);
  });
  return new Promise((listening) => {
    server.listen(0, "127.0.0.1", () => listening(server));
  });
}

/** One console message: its type, its text, and each argument's kind as the page holds it. */
async function readMessage(message) {
  const args = await Promise.all(
    message.args().map((handle) =>
      handle.evaluate((value) => {
        if (typeof value === "string") return { kind: "string", text: value };
        const plain =
          value !== null &&
          typeof value === "object" &&
          Object.getPrototypeOf(value) === Object.prototype;
        return { kind: plain ? "plain" : "other" };
      }),
    ),
  );
  return { type: message.type(), text: message.text(), args };
}

/** Opens a fresh page on one logger's bytes and returns what its console received. */
async function runPage(browser, loggerBytes, calls, query) {
  const server = await serve(loggerBytes, calls);
  const page = await browser.newPage();
  const messages = [];
  const errors = [];
  page.on("console", (message) => messages.push(readMessage(message)));
  page.on("pageerror", (error) => errors.push(error.name));
  try {
    await page.goto(`http://127.0.0.1:${server.address().port}/${query}`);
    await page.waitForFunction(() => window.callsDone === true, null, {
      timeout: PAGE_LIMIT_MS,
    });
    const debugEnabled = await page.evaluate(() => window.callsDebugEnabled);
    return { messages: await Promise.all(messages), debugEnabled, errors };
  } finally {
    await page.close();
    server.close();
  }
}

/** Why a message is off the page's own form, or "" when it is on it. */
function offForm(message, call) {
  if (!LEVEL_TYPES.has(message.type)) return "no-level";
  if (message.type !== CONSOLE_TYPE[call.level]) return "wrong-level";
  const [first, second, ...more] = message.args;
  if (first?.kind !== "string" || !EVENT_ID.test(first.text)) return "no-id";
  if (more.length > 0 || (second !== undefined && second.kind !== "plain")) return "not-one-object";
  return "";
}

/** A logger's two runs, with DEBUG off and on, counted by what its console received. */
async function readLogger(browser, loggerBytes, calls) {
  const runs = [
    { debug: false, ...(await runPage(browser, loggerBytes, calls, "")) },
    { debug: true, ...(await runPage(browser, loggerBytes, calls, "?log=debug")) },
  ];
  const read = {
    wrongCount: 0,
    errors: 0,
    id: { total: 0, off: {} },
    sentence: { total: 0, off: {} },
    debugOff: 0,
    debugOn: 0,
    debugWant: calls.filter((call) => call.level === "debug").length,
    enabled: runs.map((run) => run.debugEnabled),
    texts: [],
  };
  for (const run of runs) {
    read.errors += run.errors.length;
    const wanted = calls.filter((call) => run.debug || call.level !== "debug");
    if (run.messages.length !== wanted.length) read.wrongCount += 1;
    const debugged = run.messages.filter((message) => message.type === "debug").length;
    if (run.debug) read.debugOn = debugged;
    else read.debugOff = debugged;
    run.messages.forEach((message, at) => {
      read.texts.push(message.text);
      const call = wanted[at];
      if (call === undefined) return;
      const form = read[call.form];
      form.total += 1;
      const reason = offForm(message, call);
      if (reason) form.off[reason] = (form.off[reason] ?? 0) + 1;
    });
  }
  return read;
}

const offCount = (form) => Object.values(form.off).reduce((sum, count) => sum + count, 0);

/** Each plant a copy, by path, before the clean run counts. */
async function checkPlants(browser, logger, work, score) {
  const bytes = readFileSync(join(WORKSPACE, logger.path), "utf8");
  for (const plant of PLANTS) {
    const planted = bytes.replace(plant.from, plant.to);
    if (planted === bytes) throw new Error(`plant ${plant.name} changed no byte`);
    const copy = join(work, `${logger.name}-${plant.name}.mjs`);
    writeFileSync(copy, planted);
    const read = await readLogger(browser, readFileSync(copy), logger.calls);
    const name = `${plant.name}-${logger.name}`;
    if (plant.name === "info-by-log") {
      score.add("plant", name, offCount(read.id) > 0, `off=${JSON.stringify(read.id.off)}`);
    } else if (plant.name === "debug-always") {
      score.add("plant", name, read.debugOff > 0, `debug-off-read=${read.debugOff}`);
    } else {
      score.add("plant", name, read.debugOn === 0, `debug-on-read=${read.debugOn}`);
    }
  }
}

function grammarCounts(path) {
  const run = spawnSync("python3", [GRAMMAR, path], { encoding: "utf8" });
  if (run.status !== 0) throw new Error(`the grammar exited ${run.status}`);
  return JSON.parse(run.stdout);
}

/** The scanner on a kept file, and on a copy planted with a token to prove it reads. */
function judgeScan(kept, name, scanTool, work, score) {
  if (scanTool === undefined) {
    score.add("owed", `secret-scan-${name}`, true, "no --secret-scan given");
    return;
  }
  const clean = spawnSync("python3", [scanTool, kept]);
  score.add("clean", `secret-scan-${name}`, clean.status === 0, `exit=${clean.status}`);
  const planted = join(work, `scan-plant-${name}.log`);
  writeFileSync(planted, Buffer.concat([readFileSync(kept), Buffer.from(SCAN_PLANT)]));
  const red = spawnSync("python3", [scanTool, planted]);
  score.add("plant", `secret-scan-${name}`, red.status === 1, `exit=${red.status}`);
  unlinkSync(planted);
}

async function checkClean(browser, logger, options, canary, score) {
  if (logger.sister !== undefined) {
    const same = spawnSync("cmp", [
      join(WORKSPACE, logger.path),
      join(WORKSPACE, logger.sister),
    ]);
    score.add("clean", "logger-bytes", same.status === 0, `cmp=${same.status}`);
  }
  const { name } = logger;
  const read = await readLogger(browser, readFileSync(join(WORKSPACE, logger.path)), logger.calls);
  score.add("clean", `page-errors-${name}`, read.errors === 0, `errors=${read.errors}`);
  score.add(
    "clean",
    `one-call-${name}`,
    read.wrongCount === 0,
    `runs-off-count=${read.wrongCount}`,
  );
  score.add(
    "clean",
    `id-form-${name}`,
    read.id.total > 0 && offCount(read.id) === 0,
    `off=${offCount(read.id)} of ${read.id.total}`,
  );
  score.add(
    "counted",
    `sentence-form-${name}`,
    offCount(read.sentence) === 0,
    `off=${JSON.stringify(read.sentence.off)} of ${read.sentence.total}`,
  );
  score.add(
    "clean",
    `debug-off-${name}`,
    read.debugOff === 0 && read.enabled[0] === false,
    `read=${read.debugOff} want=0`,
  );
  score.add(
    "clean",
    `debug-on-${name}`,
    read.debugOn === read.debugWant && read.enabled[1] === true,
    `read=${read.debugOn} want=${read.debugWant}`,
  );
  const kept = join(options.keep, `grammar-page-${name}.log`);
  writeFileSync(kept, `${read.texts.join("\n")}\n`);
  const counts = grammarCounts(kept);
  score.add(
    "counted",
    `grammar-${name}`,
    Object.keys(counts.rules).length === 0,
    `messages=${read.texts.length} events=${counts.events} rules=${JSON.stringify(counts.rules)}`,
  );
  const found = readFileSync(kept).includes(canary);
  score.add("clean", `environment-canary-${name}`, !found);
  judgeScan(kept, name, options["secret-scan"], options.work, score);
}

async function main() {
  const { values: options } = parseArgs({
    options: {
      work: { type: "string" },
      out: { type: "string" },
      keep: { type: "string" },
      "secret-scan": { type: "string" },
    },
  });
  mkdirSync(options.work, { recursive: true });
  const canary = `canary-${randomBytes(8).toString("hex")}`;
  const score = new Score();
  const { chromium } = await import(pathToFileURL(process.env.PLAYWRIGHT));
  const browser = await chromium.launch({
    headless: true,
    args: ["--mute-audio"],
    env: { ...process.env, [CANARY_NAME]: canary },
  });
  try {
    for (const logger of LOGGERS) {
      await checkPlants(browser, logger, options.work, score);
    }
    for (const logger of LOGGERS) {
      await checkClean(browser, logger, options, canary, score);
    }
    score.write(options.out, browser.version());
  } finally {
    await browser.close();
  }
  return score.failed().length === 0 ? 0 : 1;
}

process.exitCode = await main();
