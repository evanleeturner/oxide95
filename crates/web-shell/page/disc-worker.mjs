// SPDX-License-Identifier: MIT
// The disc's Worker: it takes one message, the compiled `page_disc` module, the disc's
// files and one act. The files are the picked ones with their paths from the disc's top,
// or, with `kept`, the copy the page keeps in the origin's private file system. `read`
// takes the places to read, runs the wasm's `read` once and answers with the byte at each
// place, -1 for a place it could not read. `check` takes the files the game expects,
// runs the wasm's `check` once and answers with each finding's sentence. `keep` copies
// the picked files into page storage and answers whether they fit. A Worker alone may
// read a `File` or a kept file in step, which the wasm's `ReadAt` needs.
//
// A kept copy is claimed only by its record, written after its last file and removed
// before its first: a copy a reload or a full store cut off is never read as a disc.
import { logger } from "./track-logger.mjs";

// Each line goes to the page as written, ahead of the answer, so the page's trace
// keeps it even when this Worker stops before it answers.
logger.forward = (line) => postMessage({ line });

const LEVELS = [null, logger.error, logger.warn, logger.info, logger.debug];

/** The kept copy's folder and its record, at the private file system's top. */
const KEPT_FOLDER = "disc";
const KEPT_RECORD = "disc.json";

/** How much of a picked file one step of the copy holds in memory. */
const COPY_STEP = 8 * 1024 * 1024;

/** A picked `File` as the wasm reads it; a short read answers null. */
function pickedFile({ path, file }, reader) {
  return {
    path,
    size: file.size,
    readAt: (at, length) => {
      const read = reader.readAsArrayBuffer(file.slice(at, at + length));
      return read.byteLength === length ? new Uint8Array(read) : null;
    },
    close: () => {},
  };
}

/** A kept file as the wasm reads it, through its open sync access handle. */
function keptFile(path, handle) {
  return {
    path,
    size: handle.getSize(),
    readAt: (at, length) => {
      const read = new Uint8Array(length);
      return handle.read(read, { at }) === length ? read : null;
    },
    close: () => handle.close(),
  };
}

function imports(
  { files, places, expected },
  { bytes, sentences },
  wasmMemory,
) {
  const encoder = new TextEncoder();
  const decoder = new TextDecoder();
  // Writes the text only when it fits; answers its whole length either way.
  const put = (text, into, room) => {
    const encoded = encoder.encode(text);
    if (encoded.length <= room)
      new Uint8Array(wasmMemory(), into, encoded.length).set(encoded);
    return encoded.length;
  };
  return {
    file_count: () => files.length,
    file_size: (file) => files[file].size,
    file_path: (file, into, room) => put(files[file].path, into, room),
    read_at: (file, at, into, length) => {
      try {
        const read = files[file].readAt(at, length);
        if (read === null) return 1;
        new Uint8Array(wasmMemory(), into, length).set(read);
        return 0;
      } catch (error) {
        logger.warn("file.read.failed", { reason: error.name });
        return 1;
      }
    },
    place_count: () => places.length,
    place_path: (place, into, room) => put(places[place].path, into, room),
    place_at: (place) => places[place].at,
    found: (place, byte) => {
      bytes[place] = byte;
    },
    expected_count: () => expected.length,
    expected_path: (item, into, room) => put(expected[item].path, into, room),
    expected_empty: (item) => (expected[item].empty ? 1 : 0),
    finding: (at, length) => {
      sentences.push(decoder.decode(new Uint8Array(wasmMemory(), at, length)));
    },
    line: (level, at, length) => {
      const text = decoder.decode(new Uint8Array(wasmMemory(), at, length));
      LEVELS[level]?.(text);
    },
  };
}

/** Removes the kept copy, its record first; either one missing is no fault. */
async function forget(root) {
  for (const name of [KEPT_RECORD, KEPT_FOLDER]) {
    try {
      await root.removeEntry(name, { recursive: true });
    } catch (error) {
      if (error.name !== "NotFoundError") throw error;
    }
  }
}

/** Writes all of `bytes` at `at`; a short write throws, as a full store does. */
function writeAll(handle, bytes, at) {
  if (handle.write(bytes, { at }) !== bytes.length)
    throw new Error("short write");
}

/** Copies one picked file into the kept folder by its path, a step at a time. */
async function copyFile(root, { path, file }, reader) {
  const names = path.split("/");
  let folder = await root.getDirectoryHandle(KEPT_FOLDER, { create: true });
  for (const name of names.slice(0, -1))
    folder = await folder.getDirectoryHandle(name, { create: true });
  const entry = await folder.getFileHandle(names.at(-1), { create: true });
  const handle = await entry.createSyncAccessHandle();
  try {
    for (let at = 0; at < file.size; at += COPY_STEP) {
      const step = reader.readAsArrayBuffer(file.slice(at, at + COPY_STEP));
      writeAll(handle, new Uint8Array(step), at);
    }
    handle.flush();
  } finally {
    handle.close();
  }
}

/**
 * Replaces the kept copy with the pick. Answers `kept` 0 when it was copied
 * whole, 1 when the store had no room, 2 when the copy failed otherwise, and
 * the browser's `usage` and `quota` estimate after it, in bytes.
 */
async function keep(picked, folder) {
  const root = await navigator.storage.getDirectory();
  const reader = new FileReaderSync();
  let kept = 0;
  try {
    await forget(root);
    for (const one of picked) await copyFile(root, one, reader);
    const bytes = picked.reduce((sum, { file }) => sum + file.size, 0);
    const record = { folder, files: picked.length, bytes };
    const entry = await root.getFileHandle(KEPT_RECORD, { create: true });
    const handle = await entry.createSyncAccessHandle();
    try {
      writeAll(handle, new TextEncoder().encode(JSON.stringify(record)), 0);
      handle.flush();
    } finally {
      handle.close();
    }
  } catch (error) {
    kept = error.name === "QuotaExceededError" ? 1 : 2;
    logger.warn("disc.keep.failed", { reason: error.name });
    await forget(root);
  }
  const { usage, quota } = await navigator.storage.estimate();
  logger.debug("disc.kept", { kept, files: picked.length, usage, quota });
  return { kept, usage, quota };
}

/** Every file under a kept folder, with its path from the folder. */
async function* walk(folder, prefix) {
  for await (const [name, entry] of folder.entries()) {
    if (entry.kind === "directory") yield* walk(entry, `${prefix}${name}/`);
    else yield { path: `${prefix}${name}`, entry };
  }
}

/**
 * Opens the kept copy's files, in their paths' order, or answers null when
 * no record claims one or the files disagree with it.
 */
async function openKept() {
  const root = await navigator.storage.getDirectory();
  let record = null;
  let folder = null;
  try {
    const entry = await root.getFileHandle(KEPT_RECORD);
    record = JSON.parse(await (await entry.getFile()).text());
    folder = await root.getDirectoryHandle(KEPT_FOLDER);
  } catch (error) {
    if (error.name === "NotFoundError") return null;
    throw error;
  }
  const files = [];
  try {
    for await (const { path, entry } of walk(folder, ""))
      files.push(keptFile(path, await entry.createSyncAccessHandle()));
  } catch (error) {
    for (const file of files) file.close();
    throw error;
  }
  const bytes = files.reduce((sum, file) => sum + file.size, 0);
  if (files.length !== record.files || bytes !== record.bytes) {
    logger.warn("disc.kept.disagrees", { files: files.length, bytes });
    for (const file of files) file.close();
    return null;
  }
  files.sort((a, b) => (a.path < b.path ? -1 : 1));
  return { folder: record.folder, files };
}

/** Runs the wasm's `read` or `check` once on the files given. */
function runAct(data, files, folder) {
  const asked = {
    files,
    places: data.places ?? [],
    expected: data.expected ?? [],
  };
  const answers = { bytes: asked.places.map(() => -1), sentences: [] };
  let instance = null;
  const env = imports(asked, answers, () => instance.exports.memory.buffer);
  instance = new WebAssembly.Instance(data.module, { env });
  const debug = logger.debugEnabled ? 1 : 0;
  if (data.act === "check") {
    const checked = instance.exports.check(debug);
    return { checked, sentences: answers.sentences };
  }
  const opened = instance.exports.read(debug, folder ? 1 : 0);
  return { opened, bytes: answers.bytes };
}

/** Answers one act; with `kept`, `held` says whether a whole copy was kept. */
async function answer(data) {
  if (data.act === "keep") return keep(data.files, data.folder);
  if (!data.kept) {
    const reader = new FileReaderSync();
    const files = data.files.map((one) => pickedFile(one, reader));
    return runAct(data, files, data.folder);
  }
  const kept = await openKept();
  if (kept === null) return { held: false };
  try {
    return { held: true, ...runAct(data, kept.files, kept.folder) };
  } finally {
    for (const file of kept.files) file.close();
  }
}

// An async handler's throw reaches no `onerror` on the page, so it is posted.
onmessage = async ({ data }) => {
  try {
    postMessage(await answer(data));
  } catch (error) {
    logger.warn("disc.act.failed", { act: data.act, reason: error.name });
    postMessage({ failed: error.name });
  }
};
