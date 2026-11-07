// SPDX-License-Identifier: MIT
// The page's thread. It writes keys into the crossing, shows the pictures the Worker
// hands it, and keeps its own timer running. All of the wasm is on the Worker, which
// takes one message and never yields, so after Start nothing here posts to it.
import {
  DISC,
  DISC_MOVED,
  DISC_NONE,
  DISC_READ,
  DISC_REFUSED,
  PAGE_TICKS,
  READ_AGAIN,
  STILL,
  WORDS,
  pushEdge,
} from "./crossing.mjs";
import { virtualKey } from "./keys.mjs";
import { logger } from "./track-logger.mjs";

const TICK_MS = 16;
const WASM = "page_track.wasm";
const NOT_ISOLATED =
  "This page needs two safety headers from its local server, and this server did not send them, so the track cannot start. Open it through its own local server.";
const NO_WASM =
  "The track's program did not load. Check that its local server is still running, then reload this page.";
const READY = "Ready. Pick a disc image if you have one, then press Start.";
const RUNNING = "Running. Hold the up arrow on the screen to lift the mark.";
const PAUSED = "The mark is paused. The up arrow still lifts it.";
const PAUSE = "Pause the mark";
const LET_MOVE = "Let the mark move";
const NO_PICTURE =
  "This browser would not give the track its WebGL 2 picture, so the track stopped. Close other tabs that draw pictures, then reload this page.";
const STOPPED =
  "The track stopped unexpectedly. Reload this page to start it again.";
const DISC_WORDS = {
  [DISC_NONE]: RUNNING,
  [DISC_READ]:
    "The disc image read right. Hold the up arrow on the screen to lift the mark.",
  [DISC_MOVED]:
    "The disc image could not be read. It may have been moved, renamed or changed since you picked it. Reload this page and pick it again.",
  [DISC_REFUSED]:
    "That file read, but it is not a disc image this page can use. Reload this page and pick another.",
};

const sentence = document.querySelector(".js-sentence");
const fileInput = document.querySelector(".js-file");
const startButton = document.querySelector(".js-start");
const pauseButton = document.querySelector(".js-pause");
const readAgainButton = document.querySelector(".js-read-again");
const screen = document.querySelector(".js-screen");
const renderer = screen.getContext("bitmaprenderer");

const held = new Set();
let words = null;
let shown = 0;
let discShown = DISC_NONE;

function say(text) {
  sentence.textContent = text;
}

function showDisc() {
  const disc = Atomics.load(words, DISC);
  if (disc === discShown) return;
  discShown = disc;
  say(DISC_WORDS[disc]);
}

// Holds the mark or lets it drift; the pictures go on either way, and the button names
// what a press does next.
function setStill(still) {
  Atomics.store(words, STILL, still ? 1 : 0);
  pauseButton.textContent = still ? LET_MOVE : PAUSE;
}

function fromWorker({ data }) {
  if (data.bitmap) {
    renderer.transferFromImageBitmap(data.bitmap);
    shown += 1;
    return;
  }
  say(data.stopped === "picture" ? NO_PICTURE : STOPPED);
}

function start(module) {
  const buffer = new SharedArrayBuffer(WORDS * Int32Array.BYTES_PER_ELEMENT);
  words = new Int32Array(buffer);
  const file = fileInput.files[0] ?? null;
  if (file) Atomics.store(words, READ_AGAIN, 1);
  const still = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  setStill(still);
  const address = new URL(
    `track-worker.mjs${window.location.search}`,
    import.meta.url,
  );
  const worker = new Worker(address, { type: "module" });
  worker.onmessage = fromWorker;
  worker.onerror = () => say(STOPPED);
  const data = { module, buffer, file };
  worker.postMessage(data);
  window.setInterval(() => {
    Atomics.add(words, PAGE_TICKS, 1);
    showDisc();
  }, TICK_MS);
  window.trackPage = { words, shown: () => shown };
  fileInput.disabled = true;
  pauseButton.disabled = false;
  readAgainButton.disabled = !file;
  say(still ? PAUSED : RUNNING);
}

function pressed(place) {
  logger.info("place.press", { place });
  screen.focus();
}

function keyMoved(event, down) {
  const key = virtualKey(event.code);
  if (key === null || !words) return;
  if (!(event.ctrlKey || event.metaKey || event.altKey)) event.preventDefault();
  if (event.repeat || held.has(event.code) === down) return;
  pushEdge(words, key, down);
  if (down) {
    held.add(event.code);
    logger.info("key.down", { code: event.code });
  } else {
    held.delete(event.code);
    logger.info("key.up", { code: event.code });
  }
}

// A key let go while the focus is elsewhere never reaches the canvas, so leaving it
// lets go of everything held; otherwise a program would read the key down for good.
function letGo() {
  for (const code of held) {
    pushEdge(words, virtualKey(code), false);
    logger.info("key.up", { code });
  }
  held.clear();
}

function listen(module) {
  fileInput.addEventListener("change", () => {
    logger.info("place.press", { place: "file" });
  });
  startButton.addEventListener("click", () => {
    start(module);
    startButton.disabled = true;
    pressed("start");
  });
  pauseButton.addEventListener("click", () => {
    const still = Atomics.load(words, STILL) === 0;
    setStill(still);
    // The focus leaves the button, so the sentence says what changed (WCAG 4.1.3).
    say(still ? PAUSED : DISC_WORDS[discShown]);
    pressed("pause");
  });
  readAgainButton.addEventListener("click", () => {
    Atomics.store(words, READ_AGAIN, 1);
    pressed("read-again");
  });
  screen.addEventListener("keydown", (event) => keyMoved(event, true));
  screen.addEventListener("keyup", (event) => keyMoved(event, false));
  screen.addEventListener("blur", letGo);
}

async function main() {
  if (!window.crossOriginIsolated) {
    say(NOT_ISOLATED);
    return;
  }
  let module;
  try {
    module = await WebAssembly.compileStreaming(fetch(WASM));
  } catch (error) {
    logger.error("track.wasm.refused", { reason: error.name });
    say(NO_WASM);
    return;
  }
  listen(module);
  fileInput.disabled = false;
  startButton.disabled = false;
  say(READY);
}

await main();
document.documentElement.dataset.ready = "true";
