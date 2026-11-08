// SPDX-License-Identifier: MIT
// The walk page under X's own test keys. A headed browser on a private screen, each
// key sent by xdotool to whatever holds that screen's focus, and after each key a
// read by `page.evaluate` alone: where the focus is, the box open or closed, the
// crate's `ended` and `picked`, the codes the page's window saw. It judges nothing:
// one flat row a key, for a scorer outside, and a stop only at its own walls.
//
// Call, inside a private screen and sound server:
//   PLAYWRIGHT=<playwright's index.mjs> PROFILES=<folder> [STOP=<file>]
//     node dialog-keys.mjs --engine chromium|firefox [--plant <name>] <page_walk.wasm>
// The page is the shipped folder, served; a plant changes the served text alone, its
// old text found exactly once. Exits 0 with every row printed, 4 at the stop file, 5
// when the plain key never reached the page, 6 when the screen read is not private,
// 1 on anything else.
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { createServer } from "node:http";
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const PAGE = fileURLToPath(new URL("../page/", import.meta.url));
const SERVED = {
  "/walk.html": ["walk.html", "text/html"],
  "/walk.css": ["walk.css", "text/css"],
  "/walk.mjs": ["walk.mjs", "text/javascript"],
  "/walk-records.mjs": ["walk-records.mjs", "text/javascript"],
  "/logger.mjs": ["logger.mjs", "text/javascript"],
};
const NONE = 2 ** 32 - 1;
const SETTLE_MS = 300;
const ROUND_MAX = 30;
// Inside the canvas: below the browser's own bars and the page's menu bar.
const CLICK_AT = ["150", "300"];
const BAR_ROW = 'bar.append(rowsElement(barFromText(textOf(wasm.bar_text())).rows));';

// A plant on the served text, [file, old, new]: each old is found exactly once.
const TEXT_PLANTS = {
  "first-skipped": [
    "walk.mjs",
    BAR_ROW,
    `${BAR_ROW}\nbar.querySelector("button").tabIndex = -1;`,
  ],
  "escape-unprevented": [
    "walk.mjs",
    "  event.preventDefault();\n  sendKey(code);",
    "  sendKey(code);",
  ],
  "escape-misnamed": ["walk.mjs", 'event.key !== "Escape"', 'event.key !== "Esc"'],
  "one-byte": [
    "walk.mjs",
    "// SPDX-License-Identifier: MIT\n",
    "// SPDX-License-Identifier: MIt\n",
  ],
};
// A plant on the bench itself: one key by Playwright's own press, or no plain key.
const BENCH_PLANTS = new Set(["press-one", "no-plain-key"]);

// Each key by its X keysym and by Playwright's name, the second for the plant alone.
const KEYS = {
  Tab: ["Tab", "Tab"],
  "Shift+Tab": ["shift+Tab", "Shift+Tab"],
  Space: ["space", "Space"],
  Return: ["Return", "Enter"],
  Escape: ["Escape", "Escape"],
  Plain: ["z", "z"],
};

// A step is [check, key, round]. A round sends the key again, each a row, until the
// page has missed a key and then sees one again, the keyboard gone to the browser's
// own controls and back, or ROUND_MAX keys have gone. The page's focus reads cannot
// mark the way back: one engine keeps `activeElement` and `hasFocus()` unchanged
// while its own controls hold the keys, so the recorder's codes are the judge.
const onCanvas = (read) => read.role === "application";
const openBy = (key) => [
  ["open", "Shift+Tab"],
  ["open", "Shift+Tab"],
  ["open", key],
  ["open", "Tab"],
  ["open", key],
];
const GROUPS = [
  ["the bar closed, a round forward", [["bar-tab", "Tab", "round"]]],
  ["the bar closed, a round back", [["bar-tab", "Shift+Tab", "round"]]],
  [
    "the bar open",
    [
      ["bar-tab", "Shift+Tab"],
      ["bar-tab", "Shift+Tab"],
      ["bar-tab", "Space"],
      ["bar-tab", "Tab"],
      ["bar-tab", "Tab"],
      ["bar-tab", "Tab"],
      ["bar-tab", "Shift+Tab"],
      ["esc-bar", "Escape"],
    ],
  ],
  [
    "a row by Return",
    [
      ["return-row", "Shift+Tab"],
      ["return-row", "Return"],
      ["return-row", "Tab"],
      ["return-row", "Return"],
    ],
  ],
  [
    "the box, a round and the check",
    [
      ...openBy("Space"),
      ["box-tab", "Tab"],
      ["box-tab", "Tab"],
      ["space-check", "Space"],
      ["space-check", "Space"],
      ["box-tab", "Tab"],
      ["box-tab", "Tab", "round"],
      ["box-tab", "Shift+Tab", "round"],
      ["esc-box", "Escape"],
    ],
  ],
  ["Space on OK", [...openBy("Space"), ["space-push", "Space"]]],
  [
    "Space on Cancel",
    [...openBy("Space"), ["space-push", "Tab"], ["space-push", "Space"]],
  ],
  ["Return on OK", [...openBy("Return"), ["return-push", "Return"]]],
  [
    "Return on Cancel",
    [...openBy("Space"), ["return-push", "Tab"], ["return-push", "Return"]],
  ],
  [
    "Return on a check",
    [
      ...openBy("Space"),
      ["return-check", "Tab"],
      ["return-check", "Tab"],
      ["return-check", "Return"],
    ],
  ],
  [
    "Escape twice",
    [
      ...openBy("Space"),
      ["esc-twice", "Escape"],
      ...openBy("Space"),
      ["esc-twice", "Escape"],
    ],
  ],
];

class Wall extends Error {
  constructor(code, message) {
    super(message);
    this.code = code;
  }
}

const sleep = (ms) => new Promise((done) => setTimeout(done, ms));
const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

function xdo(...args) {
  try {
    return execFileSync("xdotool", args, { timeout: 3000, encoding: "utf8" }).trim();
  } catch {
    return "";
  }
}

const windowCount = () =>
  xdo("search", "--onlyvisible", "--name", ".*").split("\n").filter(Boolean).length;

// A key of X's reaches whatever holds the screen's focus, so no key goes before the
// browser's own environment names a private screen: a DISPLAY other than the
// desktop's :0 and :1, and no WAYLAND_DISPLAY at all. `names` are `NAME=value` lines.
export function screenStop(names) {
  if (names.length === 0) return "no browser environment read";
  if (names.some((name) => name.startsWith("WAYLAND_DISPLAY="))) {
    return "WAYLAND_DISPLAY set";
  }
  const displays = names.filter((name) => name.startsWith("DISPLAY="));
  if (displays.length === 0) return "no DISPLAY";
  for (const display of displays) {
    const number = display.match(/^DISPLAY=:(\d+)$/)?.[1];
    if (number === undefined) return `not a local screen, ${display}`;
    if (Number(number) < 2) return `a real screen, ${display}`;
  }
  return null;
}

// The DISPLAY and WAYLAND_DISPLAY of every browser process this run started, known
// by the PROFILES folder in its environment.
function browserScreens() {
  const mine = `PROFILES=${process.env.PROFILES}`;
  const names = new Set();
  for (const pid of readdirSync("/proc").filter((dir) => /^\d+$/.test(dir))) {
    try {
      const comm = readFileSync(`/proc/${pid}/comm`, "utf8").trim();
      if (!/chrom|firefox/i.test(comm)) continue;
      const env = readFileSync(`/proc/${pid}/environ`, "latin1").split("\0");
      if (!env.includes(mine)) continue;
      for (const line of env) {
        if (/^(DISPLAY|WAYLAND_DISPLAY)=/.test(line)) names.add(line);
      }
    } catch {
      // The process is gone.
    }
  }
  return [...names];
}

function servedTexts(plant) {
  const texts = {};
  for (const [file] of Object.values(SERVED)) {
    texts[file] = readFileSync(join(PAGE, file));
  }
  if (!(plant in TEXT_PLANTS)) return texts;
  const [file, old, made] = TEXT_PLANTS[plant];
  const text = texts[file].toString("utf8");
  const found = text.split(old).length - 1;
  if (found !== 1) throw new Wall(1, `plant ${plant}: its old text found ${found} times`);
  texts[file] = Buffer.from(text.replace(old, () => made), "utf8");
  return texts;
}

function serve(wasmPath, texts) {
  const wasm = readFileSync(wasmPath);
  const server = createServer((request, response) => {
    const path = request.url.split("?")[0];
    if (path === "/page_walk.wasm") {
      response.writeHead(200, { "Content-Type": "application/wasm" }).end(wasm);
      return;
    }
    const [file, type] = SERVED[path] ?? [];
    if (!file) {
      response.writeHead(404).end();
      return;
    }
    response.writeHead(200, { "Content-Type": type }).end(texts[file]);
  });
  return new Promise((done) => {
    server.listen(0, "127.0.0.1", () => done(server));
  });
}

// What the page holds after a key, read in the page and changing nothing but the
// recorder's list of codes, which it empties.
function readInPage() {
  const at = document.activeElement ?? document.body;
  const box = document.querySelector("dialog");
  const tag = at.tagName.toLowerCase();
  const words =
    at.getAttribute("aria-label") ??
    (tag === "button" ? at.textContent : "") ??
    "";
  const name =
    tag === "input" ? (at.closest("label")?.textContent ?? "") : words;
  return {
    role: at.getAttribute("role") ?? (tag === "input" ? at.type : tag),
    name: name.trim(),
    box: box.open,
    ended: window.pageWalk.ended() >>> 0,
    picked: window.pageWalk.picked() >>> 0,
    checks: [...box.querySelectorAll("input")]
      .map((input) => (input.checked ? "x" : "o"))
      .join(""),
    has: document.hasFocus(),
    codes: window.keysSeen.splice(0),
    born: performance.timeOrigin,
  };
}

// A passive recorder of every keydown's code the window sees; it never prevents.
function installRecorder() {
  window.keysSeen = [];
  window.addEventListener("keydown", (event) => window.keysSeen.push(event.code), {
    capture: true,
    passive: true,
  });
}

const shown = (value) => (value === NONE ? "none" : String(value));

async function main() {
  const args = process.argv.slice(2);
  const take = (flag, fallback) => {
    const at = args.indexOf(flag);
    return at === -1 ? fallback : args.splice(at, 2)[1];
  };
  const engine = take("--engine", "chromium");
  const plant = take("--plant", "none");
  const [wasmPath] = args;
  const lines = [];
  const out = (line) => {
    lines.push(line);
    console.log(line);
  };
  let context;
  let server;
  let code = 0;
  try {
    if (!["chromium", "firefox"].includes(engine)) throw new Wall(1, `engine ${engine}`);
    if (plant !== "none" && !(plant in TEXT_PLANTS) && !BENCH_PLANTS.has(plant)) {
      throw new Wall(1, `plant ${plant}: no such plant`);
    }
    const texts = servedTexts(plant);
    out(`plant ${plant}`);
    for (const [file, bytes] of Object.entries(texts)) {
      out(`served ${file} ${sha256(bytes)}`);
    }
    out(`served page_walk.wasm ${sha256(readFileSync(wasmPath))}`);
    server = await serve(wasmPath, texts);

    const playwright = await import(pathToFileURL(process.env.PLAYWRIGHT));
    const profile = join(process.env.PROFILES, `${engine}-${Date.now()}`);
    context = await playwright[engine].launchPersistentContext(profile, {
      headless: false,
      viewport: null,
      ...(engine === "chromium"
        ? { args: ["--ozone-platform=x11"] }
        : { firefoxUserPrefs: { "widget.use-xdg-desktop-portal.file-picker": 0 } }),
    });
    const page = context.pages()[0] ?? (await context.newPage());
    await page.goto(`http://127.0.0.1:${server.address().port}/walk.html`);
    await page.waitForSelector("html[data-ready]");
    const agent = await page.evaluate(() => navigator.userAgent);
    out(`engine ${engine} ${agent}`);

    const screens = browserScreens();
    out(`screen ${screens.join(" ") || "none read"}`);
    const stop = screenStop(screens);
    if (stop) throw new Wall(6, `STOP: ${stop}`);

    const win = xdo("search", "--onlyvisible", "--name", "Dialogs walk")
      .split("\n")
      .filter(Boolean)
      .pop();
    if (!win) throw new Wall(1, "BENCH: no window of the page's title on the screen");
    const home = async (why) => {
      xdo("windowfocus", win);
      xdo("mousemove", "--window", win, ...CLICK_AT);
      xdo("click", "1");
      await sleep(SETTLE_MS);
      await page.focus(".js-screen");
      out(`act ${engine} home, ${why}`);
    };
    await home("the start");
    await page.evaluate(installRecorder);
    const born = await page.evaluate(() => performance.timeOrigin);

    if (plant !== "no-plain-key") xdo("key", "z");
    await sleep(SETTLE_MS);
    const baseline = (await page.evaluate(readInPage)).codes.includes("KeyZ");
    out(`baseline ${engine} the plain key reached the page: ${baseline}`);
    if (!baseline) throw new Wall(5, "BENCH WALL: the plain key never reached the page");

    let row = 0;
    let pressed = false;
    const send = async (check, key) => {
      if (process.env.STOP && existsSync(process.env.STOP)) {
        throw new Wall(4, "STOP: the outside watcher saw a real stream");
      }
      const before = { pages: context.pages().length, windows: windowCount() };
      let sender = "xdotool";
      if (plant === "press-one" && !pressed && check === "box-tab") {
        pressed = true;
        sender = "playwright";
        await page.keyboard.press(KEYS[key][1]);
      } else {
        xdo("key", KEYS[key][0]);
      }
      await sleep(SETTLE_MS);
      row += 1;
      const did = [];
      if (context.pages().length !== before.pages) {
        did.push(`pages ${before.pages}->${context.pages().length}`);
      }
      const windows = windowCount();
      if (windows !== before.windows) did.push(`windows ${before.windows}->${windows}`);
      if (page.isClosed()) {
        out(`row ${engine} headed ${check} ${row} key=${key} by=${sender} page closed`);
        throw new Wall(1, "BENCH: the page closed");
      }
      const read = await page.evaluate(readInPage);
      if (read.born !== born) did.push("reloaded");
      out(
        [
          `row ${engine} headed ${check} ${row}`,
          `key=${key} by=${sender}`,
          `at=${read.role}:"${read.name}"`,
          `box=${read.box ? "open" : "closed"}`,
          `ended=${shown(read.ended)} picked=${shown(read.picked)}`,
          `checks=${read.checks || "-"} page-focus=${read.has}`,
          `got=${read.codes.join("+") || "-"} browser=${did.join(",") || "-"}`,
        ].join(" "),
      );
      if (did.includes("reloaded")) throw new Wall(1, "BENCH: the page reloaded");
      return read;
    };

    for (const [index, [group, steps]] of GROUPS.entries()) {
      out(`group ${engine} ${group}`);
      const start = await page.evaluate(readInPage);
      if (start.box) throw new Wall(1, `BENCH: the box left open before "${group}"`);
      if (index < 2 || !onCanvas(start) || !start.has) await home(`before "${group}"`);
      for (const [check, key, round] of steps) {
        let read = await send(check, key);
        let missed = read.codes.length === 0;
        const back = () => missed && read.codes.length > 0;
        for (let more = 1; round && !back() && more < ROUND_MAX; more += 1) {
          read = await send(check, key);
          missed ||= read.codes.length === 0;
        }
      }
    }
    const after = (await send("plain-after", "Plain")).codes.includes("KeyZ");
    out(`plain-after ${engine} the plain key reached the page: ${after}`);
    out(`rows ${engine} ${row}`);
  } catch (error) {
    code = error instanceof Wall ? error.code : 1;
    out(error instanceof Wall ? error.message : `FAILED: ${error.message}`);
  } finally {
    await context?.close().catch(() => {});
    server?.close();
  }
  process.exitCode = code;
}

if (process.argv[1] && pathToFileURL(process.argv[1]).href === import.meta.url) {
  await main();
}
