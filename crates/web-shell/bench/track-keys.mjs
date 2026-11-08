// SPDX-License-Identifier: MIT
// The track page under X's own test keys: a headed browser on a private screen, each
// key sent by xdotool to whatever holds that screen's focus. It judges nothing: one
// flat row a key, for a scorer outside, and a stop only at its own walls. Three parts:
//   replay  a one-file page given by argument, opened from disc, in each of its two
//           modes: the chords, each read by that page's own `take()`
//   chords  the shipped track page, served with both cross-origin headers and
//           started: the chords, each read by the page's console `key.down` lines
//   keys    the same page: every key of the map by its X name, the code the engine
//           gave, the number the program's side wrote back, the edges counted; then
//           Tab, a key outside the map, and a held key
// A browser's own act is read from its pages, the screen's windows, a reload and the
// plain key after, never from the page's words alone.
//
// Call, inside a private screen and sound server:
//   PLAYWRIGHT=<playwright's index.mjs> PROFILES=<folder> [STOP=<file>]
//     node track-keys.mjs --engine chromium|firefox --part replay|chords|keys
//       [--plant <name>] [--chords <file>] [--replay <page file>] [<page_track.wasm>]
// The chords file holds one chord a line, in `CTRL+ALT+E`'s form. Exits 0 with every
// row printed, 4 at the stop file, 5 when the plain key never reached the page, 6 when
// the screen read is not private, 1 on anything else.
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { createServer } from "node:http";
import { basename, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

import { EDGES, FRAMES, LAST_KEY, LOST } from "../page/crossing.mjs";

const PAGE = fileURLToPath(new URL("../page/", import.meta.url));
const TYPES = { html: "text/html", css: "text/css", mjs: "text/javascript" };
const WASM_NAME = "page_track.wasm";
const SETTLE_MS = 350;
const HOLD_MS = 1500;
const ANSWER_MS = 2000;
const START_MS = 15000;
const MODES = ["prevent", "shipped"];

const KEY_RULE =
  "  if (!(event.ctrlKey || event.metaKey || event.altKey)) event.preventDefault();";
// A plant on the served text, [file, old, new]: each old is found exactly once.
export const TEXT_PLANTS = {
  "alt-prevented": ["track.mjs", "event.metaKey || event.altKey))", "event.metaKey))"],
  "all-prevented": ["track.mjs", KEY_RULE, "  event.preventDefault();"],
  "repeat-passed": ["track.mjs", "if (event.repeat || held.has", "if (held.has"],
  "a-b-swapped": ["keys.mjs", "KeyA: 0x41,\n  KeyB: 0x42,", "KeyA: 0x42,\n  KeyB: 0x41,"],
  "shift-right-sided": ["keys.mjs", "ShiftRight: 0x10,", "ShiftRight: 0xa1,"],
  "one-byte": [
    "track.mjs",
    "// SPDX-License-Identifier: MIT\n",
    "// SPDX-License-Identifier: MIt\n",
  ],
};
// A plant on the bench itself: one key by Playwright's own press, or no plain key.
const BENCH_PLANTS = new Set(["press-one", "no-plain-key"]);

// Every key the map should hold, by its `code` and the X names that send it once; the
// numbers are no part of this list, so a row's number is read from the page alone.
export const KEY_ROWS = [
  ["Backspace", ["BackSpace"]],
  ["Enter", ["Return"]],
  ["ShiftLeft", ["Shift_L"]],
  ["ShiftRight", ["Shift_R"]],
  ["ControlLeft", ["Control_L"]],
  ["ControlRight", ["Control_R"]],
  ["AltLeft", ["Alt_L"]],
  ["AltRight", ["Alt_R"]],
  ["Pause", ["Pause"]],
  // Twice, so the lock ends as it began.
  ["CapsLock", ["Caps_Lock", "Caps_Lock"]],
  ["Escape", ["Escape"]],
  ["Space", ["space"]],
  ["PageUp", ["Prior"]],
  ["PageDown", ["Next"]],
  ["End", ["End"]],
  ["Home", ["Home"]],
  ["ArrowLeft", ["Left"]],
  ["ArrowUp", ["Up"]],
  ["ArrowRight", ["Right"]],
  ["ArrowDown", ["Down"]],
  ["Insert", ["Insert"]],
  ["Delete", ["Delete"]],
  ...[..."0123456789"].map((digit) => [`Digit${digit}`, [digit]]),
  ...[..."ABCDEFGHIJKLMNOPQRSTUVWXYZ"].map((letter) => [
    `Key${letter}`,
    [letter.toLowerCase()],
  ]),
  ...Array.from({ length: 12 }, (_, at) => [`F${at + 1}`, [`F${at + 1}`]]),
];

class Wall extends Error {
  constructor(code, message) {
    super(message);
    this.code = code;
  }
}

const sleep = (ms) => new Promise((done) => setTimeout(done, ms));
const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");
const within = (promise, ms) =>
  Promise.race([promise, sleep(ms).then(() => Promise.reject(new Error("late")))]);

// A chord's last part as the `code` it sends, and the whole chord as X names it.
const chordCode = (part) =>
  /^\d$/.test(part) ? `Digit${part}` : part.length === 1 ? `Key${part}` : part;
const chordX = (chord) => chord.toLowerCase().replace(/f(\d+)/, "F$1");
const chordPlaywright = (chord) =>
  chord
    .split("+")
    .map((part) => ({ CTRL: "Control", ALT: "Alt" })[part] ?? chordCode(part))
    .join("+");

function stopCheck() {
  if (process.env.STOP && existsSync(process.env.STOP)) {
    throw new Wall(4, "STOP: the outside watcher saw a real stream");
  }
}

// Every X action waits on the outside watcher's word: a real stream seen stops the
// bench before its next key, click or focus, the start's included.
function xdo(...args) {
  stopCheck();
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

// The page folder's own files as served, a plant changing one of them alone.
export function servedTexts(plant) {
  const texts = new Map();
  for (const file of readdirSync(PAGE).sort()) {
    if (file.split(".").pop() in TYPES) texts.set(file, readFileSync(join(PAGE, file)));
  }
  if (!(plant in TEXT_PLANTS)) return texts;
  const [file, old, made] = TEXT_PLANTS[plant];
  const text = texts.get(file).toString("utf8");
  const found = text.split(old).length - 1;
  if (found !== 1) throw new Wall(1, `plant ${plant}: its old text found ${found} times`);
  texts.set(file, Buffer.from(text.replace(old, () => made), "utf8"));
  return texts;
}

// A loopback server sending both cross-origin headers on every answer, the Worker's
// script included, so the page is isolated as its own server would make it.
function serve(texts, wasm) {
  const server = createServer((request, response) => {
    const name = new URL(request.url, "http://track").pathname.slice(1);
    const bytes = name === WASM_NAME ? wasm : texts.get(name);
    if (!bytes) {
      response.writeHead(404).end();
      return;
    }
    response
      .writeHead(200, {
        "Content-Type":
          name === WASM_NAME ? "application/wasm" : TYPES[name.split(".").pop()],
        "Cross-Origin-Opener-Policy": "same-origin",
        "Cross-Origin-Embedder-Policy": "require-corp",
      })
      .end(bytes);
  });
  return new Promise((done) => server.listen(0, "127.0.0.1", () => done(server)));
}

// Passive recorders, neither ever preventing: every keydown's code the window sees, at
// capture before the page's own handlers, a repeat marked `*`; and at bubble after
// them, each code that came back prevented.
function installRecorder() {
  if (window.benchSeen) return;
  window.benchSeen = [];
  window.benchPrevented = [];
  window.addEventListener(
    "keydown",
    (event) => window.benchSeen.push(event.repeat ? `${event.code}*` : event.code),
    { capture: true, passive: true },
  );
  window.addEventListener(
    "keydown",
    (event) => {
      if (event.defaultPrevented) window.benchPrevented.push(event.code);
    },
    { passive: true },
  );
}

// What the page holds after a key, read in the page and changing nothing but the
// recorders' lists, which it empties; the one-file page's own list is emptied too.
function readInPage(track) {
  const at = document.activeElement ?? document.body;
  return {
    at: at.tagName.toLowerCase(),
    codes: window.benchSeen.splice(0),
    prevented: window.benchPrevented.splice(0),
    took: track ? [] : window.take(),
    words: track ? Array.from(window.trackPage.words.slice(0, 16)) : null,
    born: performance.timeOrigin,
  };
}

const verdict = (page, acted) =>
  page ? (acted.length ? "both" : "page") : acted.length ? "browser" : "lost";
const hex = (value) => (value === undefined ? "-" : `0x${(value >>> 0).toString(16)}`);
const list = (values) => values.join("+") || "-";

async function main() {
  const args = process.argv.slice(2);
  const take = (flag, fallback) => {
    const at = args.indexOf(flag);
    return at === -1 ? fallback : args.splice(at, 2)[1];
  };
  const engine = take("--engine", "chromium");
  const part = take("--part", "keys");
  const plant = take("--plant", "none");
  const chordsPath = take("--chords", null);
  const replayPath = take("--replay", null);
  const [wasmPath] = args;
  const out = (line) => console.log(line);
  let context;
  let server;
  let code = 0;
  try {
    if (!["chromium", "firefox"].includes(engine)) throw new Wall(1, `engine ${engine}`);
    if (!["replay", "chords", "keys"].includes(part)) throw new Wall(1, `part ${part}`);
    if (plant !== "none" && !(plant in TEXT_PLANTS) && !BENCH_PLANTS.has(plant)) {
      throw new Wall(1, `plant ${plant}: no such plant`);
    }
    if (part === "replay" && plant in TEXT_PLANTS) {
      throw new Wall(1, `plant ${plant}: the one-file page takes no text plant`);
    }
    if (part !== "keys" && !chordsPath) throw new Wall(1, "no --chords file");
    if (part === "replay" && !replayPath) throw new Wall(1, "no --replay page");
    if (part !== "replay" && !wasmPath) throw new Wall(1, "no wasm");
    const chords = chordsPath
      ? readFileSync(chordsPath, "utf8").split("\n").map((l) => l.trim()).filter(Boolean)
      : [];
    out(`plant ${plant} part ${part} chords ${chords.length}`);
    const track = part !== "replay";
    let base = null;
    if (track) {
      const texts = servedTexts(plant);
      for (const [file, bytes] of texts) out(`served ${file} ${sha256(bytes)}`);
      const wasm = readFileSync(wasmPath);
      out(`served ${WASM_NAME} ${sha256(wasm)}`);
      server = await serve(texts, wasm);
      base = `http://127.0.0.1:${server.address().port}`;
    } else {
      out(`served ${basename(replayPath)} ${sha256(readFileSync(replayPath))}`);
    }
    const playwright = await import(pathToFileURL(process.env.PLAYWRIGHT));

    let opened = 0;
    let screenRead = false;
    // A fresh browser on a fresh profile, the page open and, for the track, started
    // by Playwright's click on Start, a pointer act and no key.
    const open = async (mode) => {
      const profile = join(process.env.PROFILES, `${engine}-${part}-${Date.now()}-${opened}`);
      opened += 1;
      context = await playwright[engine].launchPersistentContext(profile, {
        headless: false,
        viewport: null,
        ...(engine === "chromium"
          ? { args: ["--ozone-platform=x11"] }
          : { firefoxUserPrefs: { "widget.use-xdg-desktop-portal.file-picker": 0 } }),
      });
      const page = context.pages()[0] ?? (await context.newPage());
      const s = { page, mode, downs: [], pending: [] };
      page.on("console", (message) => {
        if (!message.text().startsWith("key.down")) return;
        const value = message.args()[1]?.jsonValue() ?? Promise.resolve(null);
        s.pending.push(
          value.then(
            (logged) => s.downs.push(logged?.code ?? "?"),
            () => s.downs.push("?"),
          ),
        );
      });
      if (track) {
        await page.goto(`${base}/track.html`);
        await page.waitForSelector("html[data-ready]");
        await page.click(".js-start");
        await page.waitForFunction(
          (frames) => window.trackPage && Atomics.load(window.trackPage.words, frames) >= 1,
          FRAMES,
          { timeout: START_MS },
        );
      } else {
        await page.goto(`${pathToFileURL(replayPath)}?mode=${mode}`);
      }
      await page.evaluate(installRecorder);
      s.born = await page.evaluate(() => performance.timeOrigin);
      s.title = await page.title();
      if (!screenRead) {
        out(`engine ${engine} ${await page.evaluate(() => navigator.userAgent)}`);
        const screens = browserScreens();
        out(`screen ${screens.join(" ") || "none read"}`);
        const stop = screenStop(screens);
        if (stop) throw new Wall(6, `STOP: ${stop}`);
        screenRead = true;
      }
      return s;
    };
    const read = (s) => within(s.page.evaluate(readInPage, track), ANSWER_MS);
    const downsOf = async (s) => {
      await Promise.all(s.pending.splice(0));
      return s.downs.splice(0);
    };
    // X's focus to the page by a click inside the canvas, then the focus put on it.
    const home = async (s, why) => {
      const win = xdo("search", "--onlyvisible", "--name", s.title)
        .split("\n")
        .filter(Boolean)
        .pop();
      if (!win) throw new Wall(1, "BENCH: no window of the page's title on the screen");
      const [x, y] = await s.page.evaluate(() => {
        const canvas = document.querySelector("canvas");
        canvas.scrollIntoView({ block: "center" });
        const box = canvas.getBoundingClientRect();
        return [
          Math.round((window.outerWidth - window.innerWidth) / 2 + box.x + Math.min(box.width / 2, 80)),
          Math.round(window.outerHeight - window.innerHeight + box.y + Math.min(box.height / 2, 80)),
        ];
      });
      xdo("windowfocus", win);
      xdo("mousemove", "--window", win, String(x), String(y));
      xdo("click", "1");
      await sleep(SETTLE_MS);
      await s.page.focus("canvas");
      out(`act ${engine} home, ${why}`);
    };
    // The plain key, before any row and after each click: one that never reaches the
    // page is the bench's wall, the keyboard held where no row can read it.
    const plainGate = async (s, why) => {
      if (plant !== "no-plain-key") xdo("key", "z");
      await sleep(SETTLE_MS);
      const reached = (await read(s)).codes.includes("KeyZ");
      await downsOf(s);
      out(`baseline ${engine} ${why}, the plain key reached the page: ${reached}`);
      if (!reached) {
        throw new Wall(5, `BENCH WALL: the plain key never reached the page, ${why}`);
      }
    };
    const ready = async (mode, why) => {
      const s = await open(mode);
      await home(s, why);
      await plainGate(s, why);
      return s;
    };

    let row = 0;
    let pressed = false;
    // One key or chord, F7's way: the page's read, then pages, windows and a reload,
    // and when the browser did nothing seen, the plain key after.
    const press = async (s, playwrightName, send) => {
      stopCheck();
      const before = { pages: context.pages().length, windows: windowCount() };
      const first = await read(s);
      await downsOf(s);
      let sender = "xdotool";
      if (plant === "press-one" && !pressed) {
        pressed = true;
        sender = "playwright";
        await s.page.keyboard.press(playwrightName);
      } else {
        await send();
      }
      await sleep(SETTLE_MS);
      row += 1;
      const acted = [];
      let got = null;
      if (s.page.isClosed()) acted.push("closed");
      else {
        try {
          got = await read(s);
          if (got.born !== s.born) acted.push("reloaded");
        } catch {
          acted.push("no answer");
        }
      }
      if (context.pages().length !== before.pages) {
        acted.push(`pages ${before.pages}->${context.pages().length}`);
      }
      const windows = windowCount();
      if (windows !== before.windows) acted.push(`windows ${before.windows}->${windows}`);
      const downs = s.page.isClosed() ? [] : await downsOf(s);
      let after = "-";
      if (!acted.length) {
        xdo("key", "z");
        await sleep(SETTLE_MS);
        const z = await read(s).catch(() => null);
        await downsOf(s);
        after = String(Boolean(z?.codes.includes("KeyZ")));
        if (after === "false") acted.push("focus left the page");
      }
      return { sender, first, got, downs, acted, after };
    };
    // After the browser acted, F7's way: a fresh browser, homed and gated.
    const again = async (s, did) => {
      if (!did.acted.length) return s;
      await context.close().catch(() => {});
      return ready(s.mode, "after the browser acted");
    };

    const runChords = async (mode, label) => {
      out(`group ${engine} ${label}`);
      let s = await ready(mode, `before "${label}"`);
      for (const chord of chords) {
        const did = await press(s, chordPlaywright(chord), async () => {
          xdo("key", chordX(chord));
        });
        const last = chordCode(chord.split("+").pop());
        const page = track ? did.downs.includes(last) : (did.got?.took ?? []).includes(last);
        out(
          [
            `row ${engine} headed ${label} ${row} key=${chord} by=${did.sender}`,
            `verdict=${verdict(page, did.acted)} browser=${did.acted.join(",") || "-"}`,
            `got=${did.got ? did.got.codes.length : "-"} down=${did.downs.length}`,
            `prevented=${did.got ? did.got.prevented.length : "-"} plain-after=${did.after}`,
          ].join(" "),
        );
        s = await again(s, did);
      }
      return s;
    };

    let s;
    if (part === "replay") {
      for (const mode of MODES) {
        s = await runChords(mode, `replay-${mode}`);
        await context.close().catch(() => {});
      }
      s = await ready("prevent", "before the plain key after");
    } else if (part === "chords") {
      s = await runChords(null, "track");
    } else {
      out(`group ${engine} keys`);
      s = await ready(null, 'before "keys"');
      const extras = [
        ["tab-leaves", "Tab", async () => xdo("key", "Tab")],
        ["outside-map", "Backquote", async () => xdo("key", "grave")],
        [
          "held",
          "KeyA",
          async () => {
            xdo("keydown", "a");
            await sleep(HOLD_MS);
            xdo("keyup", "a");
          },
        ],
      ];
      const steps = [
        ...KEY_ROWS.map(([key, names]) => ["keys", key, async () => xdo("key", ...names)]),
        ...extras,
      ];
      for (const [check, key, send] of steps) {
        const did = await press(s, key, send);
        const w0 = did.first.words;
        const w1 = did.got?.words;
        const seen = did.got?.codes ?? [];
        out(
          [
            `row ${engine} headed ${check} ${row} key=${key} by=${did.sender}`,
            `got=${did.got ? list(seen) : "-"}`,
            `repeats=${seen.filter((c) => c.endsWith("*")).length}`,
            `down=${list(did.downs)} prevented=${did.got ? list(did.got.prevented) : "-"}`,
            `edges=${w1 ? w1[EDGES] - w0[EDGES] : "-"} last=${hex(w1?.[LAST_KEY])}`,
            `lost=${w1 ? w1[LOST] - w0[LOST] : "-"} at=${did.got?.at ?? "-"}`,
            `browser=${did.acted.join(",") || "-"} plain-after=${did.after}`,
          ].join(" "),
        );
        if (did.acted.length) s = await again(s, did);
        else if (did.got?.at !== "canvas") {
          await home(s, "the focus off the canvas");
          await plainGate(s, "the focus off the canvas");
        }
      }
    }
    const end = await press(s, "KeyZ", async () => xdo("key", "z"));
    const after = (end.got?.codes ?? []).includes("KeyZ");
    out(`plain-after ${engine} the plain key reached the page: ${after}`);
    out(`rows ${engine} ${row}`);
    if (!after) throw new Wall(5, "BENCH WALL: the plain key after the rows never reached the page");
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
