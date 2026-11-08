// SPDX-License-Identifier: MIT
// `node --test`: the fence on `devices.mjs`' promises, read on stubs. A stub shows what
// this code does with an answer. Whether a real browser gives that answer is the browser
// harness's to find, and in the end a person's.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { startPlayer } from "../../cd-audio/page/cd-player.mjs";
import { padWords, startPolling } from "../../joystick/page/gamepads.mjs";
import { sinkRoute } from "../../sound-mix/page/sound.mjs";
import { NOT_RUN, PASS, RED } from "./computer.mjs";
import {
  askMicrophone,
  checkChoice,
  checkFollows,
  checkList,
  checkPads,
  checkQuestion,
  deviceRows,
  followPads,
  listCounts,
  NO_ANSWER,
  padRows,
  padsPresent,
  readList,
  speakerChoices,
  watchDevices,
  YES,
} from "./devices.mjs";
import { buildReport, passSentence, sentence } from "./report.mjs";

const FAKE_LABEL = "A Person's Own Headset";
const SPEAKER = { kind: "audiooutput", label: FAKE_LABEL, deviceId: "id-of-1" };
const NAMELESS = { kind: "audiooutput", label: "", deviceId: "id-of-2" };
const MICROPHONE = { kind: "audioinput", label: "", deviceId: "id-of-3" };
const CAMERA = { kind: "videoinput", label: "A camera", deviceId: "id-of-4" };
const SHORT_WAIT_MS = 5;
const NO_PRESS = [null, null, null, null];

const RECORDING = readFileSync(
  new URL("../../joystick/page/recorded-pad.txt", import.meta.url),
  "utf8",
);

function aList(devices) {
  const media = new EventTarget();
  media.devices = devices;
  media.enumerateDevices = async () => media.devices;
  return media;
}

/** A stub whose question waits until `grant` or `refuse`; its one track counts stops. */
function aQuestion() {
  const track = {
    stops: 0,
    stop: () => {
      track.stops += 1;
    },
  };
  const media = { track, asked: [] };
  const answer = new Promise((grant, refuse) => {
    media.grant = () => grant({ getTracks: () => [track] });
    media.refuse = (name) => refuse(Object.assign(new Error(name), { name }));
  });
  media.getUserMedia = (wanted) => {
    media.asked.push(wanted);
    return answer;
  };
  return media;
}

const numbers = (text) => text.split(",").filter(Boolean).map(Number);

/** The named line of the joystick crate's recording, as `getGamepads` would answer it. */
function recorded(name) {
  const line = RECORDING.split("\n").find((text) =>
    text.startsWith(`${name} |`),
  );
  const [, mapping, count, axes, down] = line
    .split("|")
    .map((cell) => cell.trim());
  if (mapping === "null") return [null];
  const pressed = new Set(numbers(down));
  const buttons = Array.from({ length: Number(count) }, (_, at) => ({
    pressed: pressed.has(at),
  }));
  return [
    {
      id: "A made pad",
      mapping,
      connected: true,
      axes: numbers(axes),
      buttons,
    },
  ];
}

/** A window that runs a frame only when the test calls `step`. */
function aWindow(frame) {
  const window = {
    frame,
    navigator: { getGamepads: () => window.frame },
    requestAnimationFrame: (step) => {
      window.step = step;
      return 1;
    },
    cancelAnimationFrame: () => {
      window.step = null;
    },
  };
  return window;
}

const noNotes = { info: () => {} };

// H1 reads a browser that shows a speaker as a pass.
test("H1 passes with 1 output", async () => {
  const media = aList([SPEAKER]);
  const found = checkList(media, await readList(media));
  assert.equal(found.check, "H1");
  assert.equal(found.state, PASS);
  assert.equal(found.words, passSentence("H1"));
});

// H1 reads a browser that shows no speaker as red, in the table's words.
test("H1 is red with 0 outputs, a microphone alone included", async () => {
  for (const devices of [[], [MICROPHONE], [CAMERA]]) {
    const media = aList(devices);
    const found = checkList(media, await readList(media));
    assert.equal(found.state, RED);
    assert.equal(found.words, sentence("H1"));
  }
});

// A browser may hide a speaker until a grant, so H1 reads again after one.
test("H1 goes red, then passes, when a grant shows the output", async () => {
  const media = aList([]);
  const states = [];
  const watch = watchDevices(media, (read, why) => {
    states.push([why, checkList(media, read).state]);
  });
  await watch.readNow("load");
  media.devices = [SPEAKER];
  await watch.readNow("grant");
  assert.deepEqual(states, [
    ["load", RED],
    ["grant", PASS],
  ]);
});

// Each check answers one result and never throws.
test("H1 is red on a refused listing, and not run with no list at all", async () => {
  const media = aList([]);
  media.enumerateDevices = async () => {
    throw new Error("refused");
  };
  const read = await readList(media);
  assert.deepEqual(read, { devices: [], refused: true });
  assert.equal(checkList(media, read).words, sentence("H1"));
  assert.equal(checkList(undefined).state, NOT_RUN);
});

// H2 passes because the page's own listener read the list again.
test("H2 passes when the listener reads the list again", async () => {
  const media = aList([SPEAKER]);
  const seen = [];
  const watch = watchDevices(media, (read, why) => {
    seen.push([why, deviceRows(read.devices).length]);
  });
  await watch.readNow("load");
  const found = await checkFollows(media, watch);
  assert.equal(found.state, PASS);
  assert.equal(found.words, passSentence("H2"));
  assert.deepEqual(seen, [
    ["load", 1],
    ["plug", 1],
  ]);
});

// A plug grows the rows: the listener reads the list as it stands then.
test("a plug grows the rows from 1 to 2", async () => {
  const media = aList([SPEAKER]);
  const counts = [];
  const watch = watchDevices(media, (read) => {
    counts.push(deviceRows(read.devices).length);
  });
  await watch.readNow("load");
  media.devices = [SPEAKER, NAMELESS];
  await checkFollows(media, watch);
  assert.deepEqual(counts, [1, 2]);
});

// The planted fault: a listener that never attached reads red.
test("H2 is red when the listener never attached", async () => {
  const media = aList([SPEAKER]);
  media.addEventListener = () => {};
  const watch = watchDevices(media, () => {});
  const found = await checkFollows(media, watch);
  assert.equal(found.state, RED);
  assert.equal(found.words, sentence("H2"));
});

test("H2 is not run with no list at all", async () => {
  assert.equal((await checkFollows(undefined)).state, NOT_RUN);
});

// Asked once, for sound alone, and turned off at once.
test("H3 passes on a yes, and the track is stopped at once", async () => {
  const media = aQuestion();
  media.grant();
  const { answer } = await askMicrophone(media, SHORT_WAIT_MS);
  assert.equal(answer, YES);
  assert.equal(media.track.stops, 1);
  assert.deepEqual(media.asked, [{ audio: true }]);
  assert.equal(checkQuestion(answer).words, passSentence("H3"));
});

// A person's no is an answer: only silence is red.
test("H3 passes on a refusal, which names the refusal", async () => {
  const media = aQuestion();
  media.refuse("NotAllowedError");
  const { answer } = await askMicrophone(media, SHORT_WAIT_MS);
  assert.equal(answer, "NotAllowedError");
  assert.equal(media.track.stops, 0);
  assert.equal(checkQuestion(answer).state, PASS);
});

test("H3 is red when the question never answers", async () => {
  const { answer } = await askMicrophone(aQuestion(), SHORT_WAIT_MS);
  assert.equal(answer, NO_ANSWER);
  const found = checkQuestion(answer);
  assert.equal(found.state, RED);
  assert.equal(found.words, sentence("H3"));
});

// A slow person never leaves the microphone open.
test("H3's late yes stops its track and turns the row to a pass", async () => {
  const media = aQuestion();
  const { answer, late } = await askMicrophone(media, SHORT_WAIT_MS);
  assert.equal(checkQuestion(answer).state, RED);
  assert.equal(media.track.stops, 0);
  media.grant();
  assert.equal(await late, YES);
  assert.equal(media.track.stops, 1);
  assert.equal(checkQuestion(await late).state, PASS);
});

// A label may carry a person's name: the screen's row holds it, and no log or report.
test("a label reaches the row, and never the counts or the report", () => {
  const devices = [SPEAKER, MICROPHONE];
  const counts = listCounts(devices);
  assert.deepEqual(counts, { outputs: 1, inputs: 1, labelled: 1 });
  assert.equal(JSON.stringify(counts).includes(FAKE_LABEL), false);
  assert.equal(JSON.stringify(counts).includes("id-of"), false);
  assert.equal(buildReport([], devices).includes(FAKE_LABEL), false);
  assert.equal(deviceRows(devices)[0], FAKE_LABEL);
});

// A row with no label takes a plain name, numbered by its kind; a camera takes no row.
test("the rows are speakers, then microphones, with no camera", () => {
  const rows = deviceRows([CAMERA, MICROPHONE, NAMELESS, SPEAKER]);
  assert.deepEqual(rows, ["Speaker 1", FAKE_LABEL, "Microphone 1"]);
});

test("padsPresent counts the pads padWords found", () => {
  assert.equal(padsPresent(padWords(NO_PRESS)), 0);
  assert.equal(padsPresent(padWords(recorded("some buttons"))), 1);
  assert.equal(padsPresent(padWords(recorded("pulled"))), 0);
  const two = [...recorded("rest"), null, ...recorded("corner")];
  assert.equal(padsPresent(padWords(two)), 2);
});

// A browser hides a pad until its first press, so the pressed frame is what H6 can see.
test("H6 passes on the recording's pressed frame", () => {
  const window = aWindow(recorded("some buttons"));
  const found = [];
  const shown = (count) => found.push(checkPads(count));
  const stop = followPads(window, startPolling, noNotes, shown);
  window.step();
  stop();
  assert.equal(found.length, 1);
  assert.equal(found[0].check, "H6");
  assert.equal(found[0].state, PASS);
  assert.equal(found[0].words, passSentence("H6"));
});

test("H6 is red on a pad with no press", () => {
  const window = aWindow(NO_PRESS);
  const found = [];
  const shown = (count) => found.push(checkPads(count));
  const stop = followPads(window, startPolling, noNotes, shown);
  window.step();
  stop();
  assert.equal(found.length, 1);
  assert.equal(found[0].state, RED);
  assert.equal(found[0].words, sentence("H6"));
});

// One change reports once, however many frames it lasts.
test("H6 follows no press, a press and a pull, 1 report a change", () => {
  const window = aWindow(NO_PRESS);
  const states = [];
  const shown = (count) => states.push(checkPads(count).state);
  const stop = followPads(window, startPolling, noNotes, shown);
  for (const frame of [
    NO_PRESS,
    recorded("some buttons"),
    recorded("pulled"),
  ]) {
    window.frame = frame;
    window.step();
    window.step();
  }
  assert.deepEqual(states, [RED, PASS, RED]);
  stop();
  assert.equal(window.step, null);
});

// A pad only adds: a refusal is no pads, told once, however many frames ask.
test("H6 is red on a refused read, and it is told once", () => {
  const window = aWindow(NO_PRESS);
  window.navigator.getGamepads = () => {
    throw new Error("refused");
  };
  const told = [];
  const states = [];
  const stop = followPads(
    window,
    startPolling,
    { info: (...values) => told.push(values) },
    (count) => states.push(checkPads(count).state),
  );
  window.step();
  window.step();
  window.step();
  stop();
  assert.deepEqual(states, [RED]);
  assert.equal(told.length, 1);
});

test("padRows names the pads showing, and a refusal shows none", () => {
  const pulled = { id: "A pulled pad", connected: false };
  const shown = { getGamepads: () => [null, ...recorded("rest"), pulled] };
  assert.deepEqual(padRows(shown), ["A made pad"]);
  const refusing = {
    getGamepads: () => {
      throw new Error("refused");
    },
  };
  assert.deepEqual(padRows(refusing), []);
  assert.deepEqual(padRows({}), []);
  assert.deepEqual(padRows(undefined), []);
});

// H4, on stub handles and on the two crates' own handles over stubs.
const H4_PASS =
  "The sound moved to the speaker you picked. This does not show that it can be heard.";
const H4_RED =
  "You picked a speaker, but the sound did not move to it. The fault is in this project.";

/** A handle that takes every choice, or, when it `ignores` them, answers as if none came. */
function aHandle({ ignores = false } = {}) {
  const handle = { sinkId: "", asked: [] };
  handle.setSink = async (id) => {
    handle.asked.push(id);
    if (!ignores) handle.sinkId = id;
    return { sinkId: handle.sinkId, refusal: null };
  };
  return handle;
}

test("H4 passes when both handles take the choice, in the page's words", async () => {
  const mix = aHandle();
  const cd = aHandle();
  const { result, refusal, taken } = await checkChoice([mix, cd], "id-of-2");
  assert.equal(result.state, PASS);
  assert.equal(result.words, H4_PASS);
  assert.equal(passSentence("H4"), H4_PASS);
  assert.equal(result.name, "Speaker choice");
  assert.deepEqual([refusal, taken], [null, 2]);
  assert.deepEqual([mix.asked, cd.asked], [["id-of-2"], ["id-of-2"]]);
});

test("H4 is red when the mix's handle ignores the choice, and when the disc's does", async () => {
  const plants = [
    [aHandle({ ignores: true }), aHandle()],
    [aHandle(), aHandle({ ignores: true })],
  ];
  for (const handles of plants) {
    const { result, refusal, taken } = await checkChoice(handles, "id-of-2");
    assert.equal(result.state, RED);
    assert.equal(result.words, H4_RED);
    assert.equal(sentence("H4"), H4_RED);
    assert.deepEqual([refusal, taken], [null, 1]);
  }
});

test("H4 answers a refusal as its red and never throws", async () => {
  const named = (name) => Object.assign(new Error(name), { name });
  const quiet = { info: () => {}, debug: () => {} };
  // The route is chosen when it is made, so this context takes a speaker from the
  // start, and its answer is swapped behind it.
  let answer = () => Promise.resolve();
  const context = { destination: {}, sinkId: "", setSinkId: (id) => answer(id) };
  const mix = sinkRoute({ context, gain: {}, document: {}, logger: quiet });
  const audio = { sinkId: "" };
  const cd = startPlayer({ document: { createElement: () => audio } }, [], quiet);

  answer = () => Promise.reject(named("NotAllowedError"));
  audio.setSinkId = async () => {};
  const rejected = await checkChoice([mix, cd], "id-of-2");
  assert.equal(rejected.result.state, RED);
  assert.equal(rejected.result.words, H4_RED);
  assert.deepEqual([rejected.refusal, rejected.taken], ["NotAllowedError", 0]);

  answer = () => {
    throw named("NotAllowedError");
  };
  audio.setSinkId = () => {
    throw named("NotFoundError");
  };
  const thrown = await checkChoice([mix, cd], "id-of-2");
  assert.equal(thrown.result.words, H4_RED);
  assert.deepEqual([thrown.refusal, thrown.taken], ["NotAllowedError", 0]);

  const broken = { setSink: () => { throw named("Error"); } };
  const odd = { setSink: async () => undefined };
  const worst = await checkChoice([broken, odd], "id-of-2");
  assert.equal(worst.result.state, RED);
  assert.equal(worst.refusal, "Error");
  assert.equal((await checkChoice([], "id-of-2")).result.state, RED, "no handle moves nothing");
});

test("the picker offers each speaker by id; the default row and a blank id are left out", () => {
  const listed = [
    { kind: "audiooutput", label: "", deviceId: "default" },
    SPEAKER,
    NAMELESS,
    MICROPHONE,
    { kind: "audiooutput", label: "", deviceId: "" },
  ];
  assert.deepEqual(speakerChoices(listed), [
    { id: "id-of-1", name: FAKE_LABEL },
    { id: "id-of-2", name: "Speaker 3" },
  ]);
  assert.deepEqual(speakerChoices([]), []);
});
