// SPDX-License-Identifier: MIT
/**
 * Reads the player's pick through `logical_disc`, on a Worker of its own: the
 * disc's one image, or the mounted folder's files by their paths from the
 * disc's top, or the copy of a pick kept in page storage. The page's `?log=`
 * choice rides to the Worker on its address, and each line the Worker writes
 * joins the page's trace. One tab of the page's origin holds the disc at a
 * time: `holdDisc` asks for it before any act.
 */
import { logger } from "./track-logger.mjs";

/** The hold's name: one for the origin, so one tab of it reads the disc. */
const HOLD = "disc";

/**
 * Asks to hold the disc for this page's life, and resolves true when this
 * page holds it, false when another tab of the same origin holds it already.
 * Ask it before any act, so a second tab starts no Worker at all; a closed
 * tab's hold ends with the tab. Under DEBUG it writes the line of the
 * starter's own search for another running copy, `v` 1 when another tab
 * holds the disc and 0 when none does.
 */
export function holdDisc() {
  return new Promise((resolve) => {
    navigator.locks.request(HOLD, { ifAvailable: true }, (lock) => {
      logger.debug("imp.call", {
        c: "USER32!FindWindowA",
        v: lock === null ? 1 : 0,
        by: "web-shell",
      });
      resolve(lock !== null);
      // A hold lasts while this callback's promise is pending: it never settles.
      return lock === null ? undefined : new Promise(() => {});
    });
  });
}

/**
 * A picked file's path from the disc's top. A folder pick's relative path
 * begins with the picked folder's own name, which is not on the disc.
 */
export function discPath(file) {
  const relative = file.webkitRelativePath;
  return relative ? relative.slice(relative.indexOf("/") + 1) : file.name;
}

/** The picked files, each with its path from the disc's top. */
function pickedFiles(picked) {
  return [...picked].map((file) => ({ path: discPath(file), file }));
}

/**
 * Runs one act on a Worker of its own, and resolves to its one answer. A
 * Worker that stopped, or posted the name of the error it met, rejects. The
 * lines it posts ahead of its answer join the page's trace.
 */
async function run(message) {
  const module = await WebAssembly.compileStreaming(
    fetch(new URL("./page_disc.wasm", import.meta.url)),
  );
  const worker = new Worker(
    new URL(`./disc-worker.mjs${globalThis.location.search}`, import.meta.url),
    { type: "module" },
  );
  try {
    return await new Promise((resolve, reject) => {
      worker.onmessage = ({ data }) => {
        if ("line" in data) logger.take(data.line);
        else if (data.failed) reject(new Error(data.failed));
        else resolve(data);
      };
      worker.onerror = (event) => reject(new Error(event.message));
      worker.postMessage({ module, ...message });
    });
  } finally {
    worker.terminate();
  }
}

/**
 * Resolves to `{ opened, bytes }`: `opened` 0 when the disc opened, 1 when
 * it did not, 2 when a path was too long; `bytes` the byte at each
 * `{ path, at }` place, the path in the kernel's spelling, -1 where none
 * was read.
 */
export function readPlaces(picked, folder, places) {
  return run({ act: "read", files: pickedFiles(picked), folder, places });
}

/**
 * Resolves to `{ checked, sentences }`: `checked` 0 when the check ran, 2
 * when a path was too long; `sentences` one a wrong file, in the expected
 * list's order, none when the pick holds every expected file readable.
 * Each expected file is `{ path, empty }`, `empty` true where the disc
 * itself holds the file empty.
 */
export function checkPick(picked, expected) {
  return run({ act: "check", files: pickedFiles(picked), expected });
}

/**
 * Replaces the kept copy with the pick, and resolves to `{ kept, usage,
 * quota }`: `kept` 0 when the pick was copied whole into page storage, 1
 * when the store had no room, 2 when the copy failed otherwise; no part of a
 * failed copy is kept. `usage` and `quota` are the browser's estimate after
 * it, in bytes.
 */
export function keepPick(picked, folder) {
  return run({ act: "keep", files: pickedFiles(picked), folder });
}

/**
 * As `readPlaces`, on the kept copy: resolves to `{ held, opened, bytes }`,
 * `held` false, alone, when no whole copy is kept.
 */
export function readKept(places) {
  return run({ act: "read", kept: true, places });
}

/**
 * As `checkPick`, on the kept copy: resolves to `{ held, checked,
 * sentences }`, `held` false, alone, when no whole copy is kept.
 */
export function checkKept(expected) {
  return run({ act: "check", kept: true, expected });
}
