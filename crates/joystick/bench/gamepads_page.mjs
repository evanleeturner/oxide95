// SPDX-License-Identifier: MIT
// Frames through the page code's own `startPolling`, in a headless browser, with `getGamepads`
// a stub on the page's own `navigator`. The stub judges the page's code alone: no pad is read.
// Nothing is judged here; `page_check.py` reads what this writes.
//
//   PLAYWRIGHT=<playwright's index.mjs> node gamepads_page.mjs \
//     <chromium|firefox> <gamepads.mjs> <frames.json> <new profile folder> <out.json>
//
// A frame is a list of slots, each null or { mapping, count, axes, down }. Axes are text, so
// the numbers that are not finite survive the trip through JSON.
import { readFile, writeFile } from "node:fs/promises";

const [browserName, modulePath, framesPath, profile, outPath] = process.argv.slice(2);
const ORIGIN = "http://pads.invalid";

const source = await readFile(modulePath, "utf8");
const frames = JSON.parse(await readFile(framesPath, "utf8"));
const playwright = await import(process.env.PLAYWRIGHT);

// Runs in the page: the stub, the polling, and what `post` was given, frame by frame.
async function pollFrames(given) {
  const toPad = (slot) =>
    slot === null
      ? null
      : {
          mapping: slot.mapping,
          connected: true,
          axes: slot.axes.map(Number),
          buttons: Array.from({ length: slot.count }, (_, at) => ({
            pressed: slot.down.includes(at),
            value: slot.down.includes(at) ? 1 : 0,
          })),
        };
  const pads = given.map((frame) => frame.map(toPad));
  let asked = 0;
  Object.defineProperty(window.navigator, "getGamepads", {
    configurable: true,
    value: () => pads[asked++] ?? [],
  });
  const { startPolling } = await import("/gamepads.mjs");
  const words = [];
  const told = [];
  let stop;
  await new Promise((done) => {
    stop = startPolling(
      window,
      (numbers) => {
        words.push(numbers);
        if (words.length === pads.length) done();
      },
      { info: (...values) => told.push(values.map(String)) },
    );
  });
  stop();
  return { words: words.slice(0, pads.length), told, agent: navigator.userAgent };
}

const context = await playwright[browserName].launchPersistentContext(profile, {
  headless: true,
  args: browserName === "chromium" ? ["--mute-audio"] : [],
});
try {
  const page = context.pages()[0] ?? (await context.newPage());
  await page.route(`${ORIGIN}/**`, (route) =>
    route.fulfill(
      route.request().url().endsWith(".mjs")
        ? { contentType: "text/javascript", body: source }
        : { contentType: "text/html", body: "<!doctype html><title>pads</title>" },
    ),
  );
  await page.goto(`${ORIGIN}/`);
  await writeFile(outPath, JSON.stringify(await page.evaluate(pollFrames, frames)));
} finally {
  await context.close();
}
