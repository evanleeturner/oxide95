// SPDX-License-Identifier: MIT
/**
 * Reads the player's pick through `logical_disc`, on a Worker of its own: the
 * disc's one image, or the mounted folder's files by their paths from the
 * disc's top. The page's `?log=` choice rides to the Worker on its address.
 */

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

/** Runs one act on a Worker of its own, and resolves to its one answer. */
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
      worker.onmessage = ({ data }) => resolve(data);
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
