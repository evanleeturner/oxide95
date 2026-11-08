// SPDX-License-Identifier: MIT
// The page's wiring: buttons to checks, results to the list. It decides nothing the
// other modules can decide. No key handler exists here and none may: a Tab key counts
// as a gesture to a browser, and no sound may start before the button. The mix's own
// listeners, in `sound.mjs`, join only at that press.
import {
  BEEP_HZ,
  countMatches,
  microphoneHears,
  pickCount,
  playBeeps,
} from "./beep.mjs";
import { startPlayer } from "./cd-player.mjs";
import {
  checkClock,
  checkFeatures,
  checkSecure,
  checkTone,
  checkWasm,
  notRun,
  PASS,
  RED,
  result,
} from "./computer.mjs";
import {
  askMicrophone,
  checkChoice,
  checkFollows,
  checkList,
  checkPads,
  checkQuestion,
  checkRemembered,
  deviceRows,
  followPads,
  keepChoice,
  listCounts,
  listsSpeakers,
  NO_ANSWER,
  NO_CHOICE,
  padRows,
  readChoice,
  speakerChoices,
  watchDevices,
  YES,
} from "./devices.mjs";
import { startPolling } from "./gamepads.mjs";
import { logger } from "./logger.mjs";
import {
  buildReport,
  passSentence,
  reportHoldsName,
  sentence,
} from "./report.mjs";
import { attachSound } from "./sound.mjs";

const REPORT_FOR_EVERYONE = false;
const NEEDS_ENGINE = "This check needs the sound engine, which did not start.";
const NOT_ASKED = "not asked";
// The mix's rate, and a program of the page's own that never hands the mix a frame:
// the speaker choice moves the game's two routes with no game and no sound.
const MIX_RATE = 44100;
const QUIET_PROGRAM = {
  memory: null,
  rate: () => MIX_RATE,
  tick: () => {},
  gesture: () => {},
  read_frames: () => 0,
  frames_ptr: () => 0,
};

const part = (name) => document.querySelector(`.js-${name}`);
const results = new Map();
let context = null;
let played = 0;
let violations = 0;
let deviceNames = [];
let padNames = [];
let speakers = [];
// The last list read, and a stored speaker's id while it waits to be tried.
let listed = [];
let remembered = "";
const store = () => window.localStorage;

function show(found) {
  results.set(found.check, found);
  logger.info("check.page.result", { check: found.check, state: found.state });
  let row = part("results").querySelector(`[data-check="${found.check}"]`);
  if (!row) {
    row = document.createElement("li");
    row.dataset.check = found.check;
    part("results").append(row);
  }
  row.dataset.state = found.state;
  row.className =
    found.state === PASS
      ? "result-pass"
      : found.state === RED
        ? "result-red"
        : "result-quiet";
  row.textContent = `${found.name}: ${found.words}`;
  part("status").textContent = `${found.name}: ${found.state}.`;
}

function randomCount() {
  const byte = crypto.getRandomValues(new Uint8Array(1))[0];
  return pickCount(byte / 256);
}

async function runChecks() {
  part("start").disabled = true;
  logger.info("check.page.started");
  show(checkSecure(window));
  const features = checkFeatures(window);
  show(features);
  show(await checkWasm(window));
  if (features.state === RED) {
    show(notRun("C4", NEEDS_ENGINE));
    show(notRun("C5", NEEDS_ENGINE));
  } else {
    context = new AudioContext();
    const clock = await checkClock(context);
    show(clock);
    show(
      clock.state === RED
        ? notRun("C5", NEEDS_ENGINE)
        : await checkTone(context),
    );
    part("count").hidden = false;
    if (navigator.mediaDevices) part("mic").hidden = false;
    startSpeakers();
  }
  await showReport();
  logger.info("check.page.done", { results: results.size });
  document.body.dataset.ran = "true";
}

async function playForCount() {
  played = randomCount();
  logger.debug("check.page.beeps", { count: played, hz: BEEP_HZ });
  await playBeeps(context, played);
  document.body.dataset.beeped = "true";
}

async function answerCount(answer) {
  const good = countMatches(answer, played);
  show(
    result("C6", good ? PASS : RED, good ? passSentence("C6") : sentence("C6")),
  );
  await showReport();
}

async function skipCount() {
  show(result("C6", NOT_ASKED, "You skipped this check, so it fails nothing."));
  await showReport();
}

async function useMicrophone() {
  part("mic-start").disabled = true;
  const heard = await microphoneHears(
    context,
    navigator.mediaDevices,
    randomCount(),
  );
  show(
    result(
      "C10",
      heard ? PASS : RED,
      heard ? passSentence("C10") : sentence("C10"),
    ),
  );
  await showReport();
  document.body.dataset.micDone = "true";
}

// The game's two routes, held from the Start press on and never before: the mix's
// context and the disc's element. Neither plays a sound here.
function startSpeakers() {
  if (!navigator.mediaDevices) return;
  speakers = [
    attachSound({ wasm: QUIET_PROGRAM, logger }),
    startPlayer(window, [], logger),
  ];
  part("speaker").hidden = false;
  remembered = readChoice(store);
  if (remembered) void applyRemembered(listed);
  else show(notRun("H5", NO_CHOICE));
}

// A stored id waits for a list that holds speakers, then is tried once; before then no
// words show. A person's pick before then wins.
async function applyRemembered(devices) {
  if (!remembered || speakers.length === 0 || !listsSpeakers(devices)) return;
  const id = remembered;
  remembered = "";
  const box = part("speaker-choice");
  if (speakerChoices(devices).some((choice) => choice.id === id)) box.value = id;
  const found = await checkRemembered(speakers, id, devices);
  logger.info("check.page.speaker", {
    why: found.stale ? "fallback" : "remembered",
    refusal: found.refusal,
  });
  if (found.stale) {
    keepChoice(store, "");
    if (box.value === id) box.value = "";
  }
  show(found.result);
  const onDefault = speakers.filter((handle) => handle.sinkId === "").length;
  document.body.dataset.speakerDefault = `${onDefault} of ${speakers.length}`;
  document.body.dataset.remembered = `${found.result.check} ${found.result.state}`;
  await showReport();
}

// A choice still listed stays chosen; one that left the list shows the default.
function fillSpeakers(devices) {
  const box = part("speaker-choice");
  const picked = box.value;
  const choices = speakerChoices(devices);
  const rows = choices.map(({ id, name }) => {
    const row = document.createElement("option");
    row.value = id;
    row.textContent = name;
    return row;
  });
  box.replaceChildren(box.options[0], ...rows);
  box.value = choices.some(({ id }) => id === picked) ? picked : "";
}

async function chooseSpeaker() {
  remembered = "";
  const id = part("speaker-choice").value;
  const { result: found, refusal, taken } = await checkChoice(speakers, id);
  logger.info("check.page.speaker", { why: "pick", refusal });
  logger.info("check.page.speaker", {
    why: "stored",
    refusal: keepChoice(store, id),
  });
  show(found);
  document.body.dataset.speakerTaken = `${taken} of ${speakers.length}`;
  await showReport();
}

async function knownDevices() {
  if (!navigator.mediaDevices) return [];
  return navigator.mediaDevices.enumerateDevices();
}

async function showReport() {
  const devices = await knownDevices();
  const text = buildReport([...results.values()], devices);
  const labels = devices.map((device) => device.label);
  const leaks = reportHoldsName(text, labels);
  part("report-text").textContent = leaks ? "" : text;
  if (leaks) show(result("P3", RED, sentence("P3")));
  const asked = new URLSearchParams(window.location.search).has("report");
  part("report").hidden = !(REPORT_FOR_EVERYONE || asked);
}

async function copyReport() {
  const text = part("report-text").textContent;
  let route = "clipboard";
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    route = copyByTextarea(text) ? "textarea" : "none";
  }
  part("copy").dataset.route = route;
  part("status").textContent =
    route === "none"
      ? "The report could not be copied. Select its text below."
      : "The report was copied.";
}

function copyByTextarea(text) {
  const box = document.createElement("textarea");
  box.value = text;
  document.body.append(box);
  box.select();
  const copied = document.execCommand("copy");
  box.remove();
  return copied;
}

function onViolation() {
  violations += 1;
  logger.info("check.page.violation", { count: violations });
  show(result("P2", RED, sentence("P2")));
}

function fillRows() {
  const rows = [...deviceNames, ...padNames].map((name) => {
    const row = document.createElement("li");
    row.textContent = name;
    return row;
  });
  part("device-list").replaceChildren(...rows);
}

function showRead(read, why, media) {
  const { outputs, inputs, labelled } = listCounts(read.devices);
  logger.debug("check.page.devices", { why, outputs, inputs, labelled });
  deviceNames = deviceRows(read.devices);
  fillRows();
  fillSpeakers(read.devices);
  show(checkList(media, read));
  listed = read.devices;
  void applyRemembered(read.devices);
}

function showPads(count) {
  logger.debug("check.page.pads", { pads: count });
  padNames = padRows(navigator);
  fillRows();
  show(checkPads(count));
}

// `gamepads.mjs` tells a refusal in its own words, which are no event id.
function padsRefused(_, why) {
  logger.info("check.page.refused", { why });
}

async function answered(answer, watch) {
  logger.info("check.page.microphone", { answer });
  show(checkQuestion(answer));
  if (answer === YES) await watch.readNow("grant");
}

async function ask(media, watch) {
  part("ask").disabled = true;
  const { answer, late } = await askMicrophone(media);
  await answered(answer, watch);
  if (answer === NO_ANSWER) {
    void late.then((lateAnswer) => answered(lateAnswer, watch));
  }
  document.body.dataset.asked = "true";
}

// The list and the pads start at load: a listing plays nothing and opens nothing. The
// microphone is asked only from a person's press.
async function startDevices() {
  const media = navigator.mediaDevices;
  followPads(window, startPolling, { info: padsRefused }, showPads);
  if (!media) {
    show(checkList(media));
    show(await checkFollows(media));
    part("ask").hidden = true;
    return;
  }
  const watch = watchDevices(media, (read, why) => showRead(read, why, media));
  await watch.readNow("load");
  show(await checkFollows(media, watch));
  part("ask").addEventListener("click", () => void ask(media, watch));
}

document.addEventListener("securitypolicyviolation", onViolation);
part("start").addEventListener("click", () => void runChecks());
part("beeps").addEventListener("click", () => void playForCount());
for (const count of [2, 3, 4]) {
  part(`count-${count}`).addEventListener(
    "click",
    () => void answerCount(count),
  );
}
part("count-skip").addEventListener("click", () => void skipCount());
part("mic-start").addEventListener("click", () => void useMicrophone());
part("copy").addEventListener("click", () => void copyReport());
part("speaker-choice").addEventListener("change", () => void chooseSpeaker());
void startDevices();
