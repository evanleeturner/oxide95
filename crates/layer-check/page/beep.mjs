// SPDX-License-Identifier: MIT
// C6 and C10: the two checks that reach past the browser, to a person's ear or a
// microphone. Neither can be proved by a machine alone: a person's count is the only
// real answer, so a skip reads "not asked" and fails nothing. The microphone is asked
// for only here, after its own button.
export const BEEP_HZ = 880;

const BEEP_S = 0.15;
const GAP_S = 0.25;
const LEVEL = 0.05;
const LEAD_S = 0.05;
const COUNTS = [2, 3, 4];
const FFT_SIZE = 2048;
const HEARD_FLOOR = 64;
const POLL_MS = 50;
const MIC_WAIT_MS = 5000;

/** One of 2, 3 or 4, by `random`, which answers from 0 up to 1. */
export function pickCount(random) {
  return COUNTS[Math.floor(random * COUNTS.length)];
}

/** Each beep's start and stop in seconds, from `now`. */
export function planBeeps(count, now) {
  const beeps = [];
  for (let index = 0; index < count; index += 1) {
    const start = now + LEAD_S + index * (BEEP_S + GAP_S);
    beeps.push({ start, stop: start + BEEP_S });
  }
  return beeps;
}

/** True when the person's answer is the count played. */
export function countMatches(answer, played) {
  return answer === played;
}

const wait = (ms) => new Promise((done) => setTimeout(done, ms));

/** Plays the beeps at a low level; resolves when the last has ended. */
export async function playBeeps(context, count) {
  const beeps = planBeeps(count, context.currentTime);
  for (const { start, stop } of beeps) {
    const tone = context.createOscillator();
    const level = context.createGain();
    tone.frequency.value = BEEP_HZ;
    level.gain.value = LEVEL;
    tone.connect(level).connect(context.destination);
    tone.start(start);
    tone.stop(stop);
  }
  await wait((beeps.at(-1).stop - context.currentTime) * 1000 + POLL_MS);
}

function askForMicrophone(mediaDevices) {
  const noAnswer = wait(MIC_WAIT_MS).then(() => null);
  const answer = mediaDevices.getUserMedia({ audio: true }).catch(() => null);
  return Promise.race([answer, noAnswer]);
}

async function loudestWhile(tap, playing) {
  const bins = new Uint8Array(tap.frequencyBinCount);
  let loudest = 0;
  let done = false;
  void playing.then(() => {
    done = true;
  });
  while (!done) {
    tap.getByteFrequencyData(bins);
    loudest = Math.max(loudest, ...bins);
    await wait(POLL_MS);
  }
  return loudest;
}

/** True when the microphone heard sound during the beeps; false on any refusal. */
export async function microphoneHears(context, mediaDevices, count) {
  const stream = await askForMicrophone(mediaDevices);
  if (!stream) return false;
  const tap = context.createAnalyser();
  tap.fftSize = FFT_SIZE;
  const source = context.createMediaStreamSource(stream);
  source.connect(tap);
  const loudest = await loudestWhile(tap, playBeeps(context, count));
  source.disconnect();
  for (const track of stream.getTracks()) track.stop();
  return loudest >= HEARD_FLOOR;
}
