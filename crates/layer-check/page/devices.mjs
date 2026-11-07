// SPDX-License-Identifier: MIT
// H1, H2 and H3: which speakers and microphones the browser shows this page.
// The list alone: it chooses no speaker and stores nothing. Each check answers one
// result and never throws, as `computer.mjs` does. A listing is asked without a press,
// since it plays nothing. The microphone is asked only from a person's press.
//
// A device's label may carry a person's name, so a label goes to the page's own row
// and nowhere else: `listCounts` is what a log may hold.
import { failed, notRun, passed } from "./computer.mjs";
import { publicName } from "./report.mjs";

export const NO_ANSWER = "none";
export const YES = "yes";

const FOLLOW_WAIT_MS = 1000;
const FOLLOW_POLL_MS = 20;
const QUESTION_WAIT_MS = 30000;
const NEEDS_SECURE = "This check needs a secure address, which this page lacks.";

const wait = (ms) => new Promise((done) => setTimeout(done, ms));

/** One read of the list: `devices` is empty and `refused` true when the browser refused. */
export async function readList(mediaDevices) {
  try {
    return { devices: await mediaDevices.enumerateDevices(), refused: false };
  } catch {
    return { devices: [], refused: true };
  }
}

/** The numbers a log may hold. No label or id goes in. */
export function listCounts(devices) {
  const of = (kind) => devices.filter((device) => device.kind === kind).length;
  return {
    outputs: of("audiooutput"),
    inputs: of("audioinput"),
    labelled: devices.filter((device) => device.label).length,
  };
}

/** The rows a person reads: speakers, then microphones; cameras are left out. */
export function deviceRows(devices) {
  return ["audiooutput", "audioinput"].flatMap((kind) =>
    devices
      .filter((device) => device.kind === kind)
      .map((device, index) => device.label || publicName(device, index)),
  );
}

/** H1. Red on no output or a refusal; not run where the browser hides the list. */
export function checkList(mediaDevices, read) {
  if (!mediaDevices) return notRun("H1", NEEDS_SECURE);
  return listCounts(read.devices).outputs > 0 ? passed("H1") : failed("H1");
}

/**
 * Reads the list again at every `devicechange`, handing each read and its reason to
 * `shown`. `readNow` is also how the page reads it at load and after a grant.
 */
export function watchDevices(mediaDevices, shown) {
  let reads = 0;
  const readNow = async (why) => {
    const read = await readList(mediaDevices);
    reads += 1;
    shown(read, why);
  };
  mediaDevices.addEventListener("devicechange", () => void readNow("plug"));
  return { readNow, reads: () => reads };
}

/**
 * H2. Fires a made `devicechange` at the page's own listener, and is red when no new
 * read follows. A made event shows the page listens; a real plug is a person's to try.
 */
export async function checkFollows(mediaDevices, watch) {
  if (!mediaDevices) return notRun("H2", NEEDS_SECURE);
  const before = watch.reads();
  mediaDevices.dispatchEvent(new Event("devicechange"));
  for (let spent = 0; spent < FOLLOW_WAIT_MS; spent += FOLLOW_POLL_MS) {
    if (watch.reads() > before) return passed("H2");
    await wait(FOLLOW_POLL_MS);
  }
  return failed("H2");
}

/**
 * Asks once for the microphone and turns it off at once. Answers `YES`, a refusal's
 * name, or `NO_ANSWER` when the wait ran out; `late` settles on the real answer, whenever it
 * comes, and stops a late grant's track, so a slow person never leaves it open.
 */
export async function askMicrophone(mediaDevices, waitMs = QUESTION_WAIT_MS) {
  const late = (async () => {
    try {
      const stream = await mediaDevices.getUserMedia({ audio: true });
      for (const track of stream.getTracks()) track.stop();
      return YES;
    } catch (error) {
      return error.name;
    }
  })();
  const answer = await Promise.race([late, wait(waitMs).then(() => NO_ANSWER)]);
  return { answer, late };
}

/** H3. Any answer passes, a person's no included: only silence is red. */
export function checkQuestion(answer) {
  return answer === NO_ANSWER ? failed("H3") : passed("H3");
}
