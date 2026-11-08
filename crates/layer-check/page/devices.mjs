// SPDX-License-Identifier: MIT
// H1, H2, H3 and H6: which speakers, microphones and pads the browser shows this page.
// H4: whether a speaker a person picks reaches the game's two routes. H5: whether the
// page uses that pick again next time; it stores the device's id alone, in one key of
// the browser's own store. Each check answers one result and never throws, as
// `computer.mjs` does. A listing is asked without a press, since it plays nothing. The
// microphone is asked only from a person's press.
//
// A device's label may carry a person's name, so a label goes to the page's own row
// and nowhere else: `listCounts` is what a log may hold, and the store never holds one.
import { failed, notRun, PASS, passed } from "./computer.mjs";
import { publicName } from "./report.mjs";

export const NO_ANSWER = "none";
// The browser's own row for its default speaker: the page offers "" for that.
export const DEFAULT_ID = "default";
export const YES = "yes";

const FOLLOW_WAIT_MS = 1000;
const FOLLOW_POLL_MS = 20;
const QUESTION_WAIT_MS = 30000;
const WORDS_A_PAD = 11;
const NEEDS_SECURE = "This check needs a secure address, which this page lacks.";
export const NO_CHOICE = "No speaker was chosen on this page before.";
export const CHOICE_KEY = "layer-check.speaker";
// The refusals that say the stored id names no speaker the page can use now. Neither
// tells a speaker gone from one the browser renamed: Chromium gives the same speaker a
// new id at every reload.
const STALE = new Set(["NotFoundError", "SecurityError"]);

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
 * The speakers a person may pick, each { id, name }, in the browser's order, named as
 * `deviceRows` names them. The page's first choice is the default, "", so the browser's
 * own default row and a row with no id are left out. A name goes to the page's own
 * control and nowhere else.
 */
export function speakerChoices(devices) {
  return devices
    .filter((device) => device.kind === "audiooutput")
    .map((device, index) => ({
      id: device.deviceId,
      name: device.label || publicName(device, index),
    }))
    .filter(({ id }) => id && id !== DEFAULT_ID);
}

/**
 * H4. Hands each handle the choice and is red unless every one takes it: no refusal,
 * and the id read back. A read shows the browser took the choice, never that sound
 * left that speaker. Answers { result, refusal, taken }: the first refusal's name or
 * null, and how many handles read the id back. Never throws.
 */
export async function checkChoice(handles, id) {
  let refusal = null;
  let taken = 0;
  for (const handle of handles) {
    let answer = null;
    try {
      answer = await handle.setSink(id);
    } catch (error) {
      answer = { refusal: error?.name ?? String(error) };
    }
    refusal ??= answer?.refusal ?? null;
    if (answer?.refusal === null && answer.sinkId === id) taken += 1;
  }
  const moved = handles.length > 0 && taken === handles.length;
  return { result: moved ? passed("H4") : failed("H4"), refusal, taken };
}

/**
 * True once the browser lists a speaker by its id. Before a grant Firefox lists none,
 * and a stored id waits for this rather than read as stale.
 */
export function listsSpeakers(devices) {
  return devices.some((device) => device.kind === "audiooutput" && device.deviceId);
}

/**
 * The stored id, or "" when none is stored or the store cannot be read, as a private
 * window's may not. `store` answers the store, since even reaching it can throw.
 */
export function readChoice(store) {
  try {
    return store().getItem(CHOICE_KEY) || "";
  } catch {
    return "";
  }
}

/**
 * Stores the id alone, or clears the store for "", the default. Answers the refusal's
 * name, or null. Never throws: a pick stands without its memory.
 */
export function keepChoice(store, id) {
  try {
    if (id) store().setItem(CHOICE_KEY, id);
    else store().removeItem(CHOICE_KEY);
    return null;
  } catch (error) {
    return error?.name ?? String(error);
  }
}

/**
 * H5, asked only once `listsSpeakers` holds. An id the list lacks is stale and no handle
 * is asked; one a handle refuses as stale sends every handle back to the default. Either
 * way H5 is red, in words that claim no more than "could not find". Any other failure is
 * H4's red, as a pick's would be. Answers { result, refusal, taken, stale }. Never throws.
 */
export async function checkRemembered(handles, id, devices) {
  const listed = devices.some(
    (device) => device.kind === "audiooutput" && device.deviceId === id,
  );
  if (!listed) return { result: failed("H5"), refusal: null, taken: 0, stale: true };
  const { result, refusal, taken } = await checkChoice(handles, id);
  if (result.state === PASS) {
    return { result: passed("H5"), refusal, taken, stale: false };
  }
  if (!STALE.has(refusal)) return { result, refusal, taken, stale: false };
  await checkChoice(handles, "");
  return { result: failed("H5"), refusal, taken, stale: true };
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

/** How many pads `padWords` found present: each pad's first word says so. */
export function padsPresent(words) {
  let count = 0;
  for (let at = 0; at < words.length; at += WORDS_A_PAD) {
    if (words[at] === 1) count += 1;
  }
  return count;
}

/** H6. A browser hides a pad until its first press, so 0 is what a pad unpressed reads. */
export function checkPads(count) {
  return count > 0 ? passed("H6") : failed("H6");
}

/** The names of the pads showing; a pad's name is its maker's, and a refusal is none. */
export function padRows(navigator) {
  try {
    return Array.from(navigator?.getGamepads?.() ?? [])
      .filter((pad) => pad && pad.connected !== false)
      .map((pad) => pad.id);
  } catch {
    return [];
  }
}

/**
 * Runs `startPolling` and hands `shown` the count of pads, first and at each change.
 * Returns the script's stop().
 */
export function followPads(window, startPolling, logger, shown) {
  let seen = null;
  const post = (words) => {
    const count = padsPresent(words);
    if (count === seen) return;
    seen = count;
    shown(count);
  };
  return startPolling(window, post, logger);
}
