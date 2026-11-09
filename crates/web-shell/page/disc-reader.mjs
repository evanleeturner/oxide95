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

/**
 * Resolves to `{ opened, bytes }`: `opened` 0 when the disc opened, 1 when
 * it did not, 2 when a path was too long; `bytes` the byte at each
 * `{ path, at }` place, the path in the kernel's spelling, -1 where none
 * was read.
 */
export async function readPlaces(picked, folder, places) {
  const module = await WebAssembly.compileStreaming(
    fetch(new URL("./page_disc.wasm", import.meta.url)),
  );
  const files = [...picked].map((file) => ({ path: discPath(file), file }));
  const worker = new Worker(
    new URL(`./disc-worker.mjs${globalThis.location.search}`, import.meta.url),
    { type: "module" },
  );
  try {
    return await new Promise((resolve, reject) => {
      worker.onmessage = ({ data }) => resolve(data);
      worker.onerror = (event) => reject(new Error(event.message));
      worker.postMessage({ module, files, folder, places });
    });
  } finally {
    worker.terminate();
  }
}
