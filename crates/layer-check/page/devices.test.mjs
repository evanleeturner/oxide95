// SPDX-License-Identifier: MIT
// `node --test`: the fence on `devices.mjs`' promises, read on stubs. A stub shows what
// this code does with an answer. Whether a real browser gives that answer is the browser
// harness's to find, and in the end a person's.
import assert from "node:assert/strict";
import { test } from "node:test";

import { NOT_RUN, PASS, RED } from "./computer.mjs";
import {
  askMicrophone,
  checkFollows,
  checkList,
  checkQuestion,
  deviceRows,
  listCounts,
  NO_ANSWER,
  readList,
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
