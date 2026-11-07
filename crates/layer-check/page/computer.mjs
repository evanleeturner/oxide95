// SPDX-License-Identifier: MIT
// C1 to C5: what the browser can do, using no code from this project. Each check
// answers one result, { check, state, words }, and never throws: a browser's refusal
// is that check's red. A clock that moves shows the engine runs and proves no device,
// so C4's words never say a device works.
import { checkName, passSentence, sentence } from "./report.mjs";

export const PASS = "pass";
export const RED = "red";
export const NOT_RUN = "not run";

const EMPTY_WASM = new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0]);
const TONE_HZ = 440;
const TONE_GAIN = 0.2;
const FFT_SIZE = 2048;
const CLOCK_WAIT_MS = 300;
const RESUME_WAIT_MS = 2000;
const TONE_WAIT_MS = 400;

export function result(check, state, words) {
  return { check, name: checkName(check), state, words };
}

export function passed(check) {
  return result(check, PASS, passSentence(check));
}

export function failed(check, values) {
  return result(check, RED, sentence(check, values));
}

export function notRun(check, reason) {
  return result(check, NOT_RUN, reason);
}

/** The names this page needs and `scope` lacks, in a fixed order; empty when none. */
export function missingFeatures(scope) {
  const needs = [
    "AudioContext",
    "AnalyserNode",
    "OscillatorNode",
    "WebAssembly",
  ];
  return needs.filter((name) => !scope[name]);
}

/** True when the two frequencies sit within one analyser bin of each other. */
export function withinOneBin(heardHz, wantHz, sampleRate, fftSize) {
  return Math.abs(heardHz - wantHz) <= sampleRate / fftSize;
}

/** The loudest bin's frequency, or null when every bin reads silence. */
export function peakHz(bins, sampleRate, fftSize) {
  let best = 0;
  for (let index = 1; index < bins.length; index += 1) {
    if (bins[index] > bins[best]) best = index;
  }
  if (bins[best] === 0) return null;
  return (best * sampleRate) / fftSize;
}

const wait = (ms) => new Promise((done) => setTimeout(done, ms));

export function checkSecure(scope) {
  return scope.isSecureContext ? passed("C1") : failed("C1");
}

export function checkFeatures(scope) {
  const missing = missingFeatures(scope);
  if (missing.length > 0) return failed("C2", { feature: missing[0] });
  return passed("C2");
}

export async function checkWasm(scope) {
  try {
    await scope.WebAssembly.compile(EMPTY_WASM);
    return passed("C3");
  } catch {
    return failed("C3");
  }
}

/** Starts the context; the caller must be inside a button's click. */
export async function checkClock(context) {
  await Promise.race([context.resume(), wait(RESUME_WAIT_MS)]);
  const first = context.currentTime;
  await wait(CLOCK_WAIT_MS);
  return context.currentTime > first ? passed("C4") : failed("C4");
}

/** A tone into a tap, and on through a gain of 0, so nothing is heard. */
export async function checkTone(context) {
  const tone = context.createOscillator();
  const level = context.createGain();
  const tap = context.createAnalyser();
  const hush = context.createGain();
  tone.frequency.value = TONE_HZ;
  level.gain.value = TONE_GAIN;
  tap.fftSize = FFT_SIZE;
  hush.gain.value = 0;
  tone.connect(level).connect(tap).connect(hush).connect(context.destination);
  tone.start();
  await wait(TONE_WAIT_MS);
  const bins = new Uint8Array(tap.frequencyBinCount);
  tap.getByteFrequencyData(bins);
  tone.stop();
  tone.disconnect();
  const heard = peakHz(bins, context.sampleRate, FFT_SIZE);
  const good =
    heard !== null &&
    withinOneBin(heard, TONE_HZ, context.sampleRate, FFT_SIZE);
  return good ? passed("C5") : failed("C5");
}
