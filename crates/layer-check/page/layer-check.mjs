// SPDX-License-Identifier: MIT
// The page's wiring: buttons to checks, results to the list. It decides nothing the
// other modules can decide. No key handler exists here and none may: a Tab key counts
// as a gesture to a browser, and no sound may start before the button.
import {
  BEEP_HZ,
  countMatches,
  microphoneHears,
  pickCount,
  playBeeps,
} from "./beep.mjs";
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
import { logger } from "./logger.mjs";
import {
  buildReport,
  passSentence,
  reportHoldsName,
  sentence,
} from "./report.mjs";

const REPORT_FOR_EVERYONE = false;
const NEEDS_ENGINE = "This check needs the sound engine, which did not start.";
const NOT_ASKED = "not asked";

const part = (name) => document.querySelector(`.js-${name}`);
const results = new Map();
let context = null;
let played = 0;
let violations = 0;

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
