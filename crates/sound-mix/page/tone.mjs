// SPDX-License-Identifier: MIT
// The proof page's wiring: the thin wasm, the sound script, and what a person is told.
// The state goes in a `role="status"` line, so a reader hears it and the focus stays.
// Exposes `window.toneProof`, for `tone-browser.mjs` to read.
import { logger } from "./logger.mjs";
import { attachSound } from "./sound.mjs";

const WASM_URL = "page_tone.wasm";

const state = document.querySelector(".js-state");
const mute = document.querySelector(".js-mute");

const { instance } = await WebAssembly.instantiateStreaming(fetch(WASM_URL));
const wasm = instance.exports;
wasm.boot(performance.now());

const told = (sound) => {
  if (sound.state !== "running") return "Sound is waiting for a key or a click.";
  return sound.muted ? "Sound is on, muted." : "Sound is on.";
};

const sound = attachSound({
  wasm,
  logger,
  onChange: () => {
    state.textContent = told(sound);
  },
});
state.textContent = told(sound);

mute.addEventListener("click", () => {
  sound.setMuted(!sound.muted);
  mute.setAttribute("aria-pressed", String(sound.muted));
});

window.toneProof = { wasm, sound };
document.documentElement.dataset.ready = "true";
