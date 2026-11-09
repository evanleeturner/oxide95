// SPDX-License-Identifier: MIT
// The disc's Worker: it takes one message, the compiled `page_disc` module, the picked
// files with their paths from the disc's top, and the places to read; it runs the
// wasm's `read` once and answers with the byte at each place, -1 for a place it could
// not read. A Worker alone may read a `File` in step, which the wasm's `ReadAt` needs.
import { logger } from "./track-logger.mjs";

const LEVELS = [null, logger.error, logger.warn, logger.info, logger.debug];

function imports(files, places, bytes, wasmMemory) {
  const reader = new FileReaderSync();
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
    file_size: (file) => files[file].file.size,
    file_path: (file, into, room) => put(files[file].path, into, room),
    read_at: (file, at, into, length) => {
      try {
        const picked = files[file].file;
        const read = reader.readAsArrayBuffer(picked.slice(at, at + length));
        if (read.byteLength !== length) return 1;
        new Uint8Array(wasmMemory(), into, length).set(new Uint8Array(read));
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
    line: (level, at, length) => {
      const text = decoder.decode(new Uint8Array(wasmMemory(), at, length));
      LEVELS[level]?.(text);
    },
  };
}

onmessage = ({ data }) => {
  const bytes = data.places.map(() => -1);
  let instance = null;
  const env = imports(
    data.files,
    data.places,
    bytes,
    () => instance.exports.memory.buffer,
  );
  instance = new WebAssembly.Instance(data.module, { env });
  const opened = instance.exports.read(
    logger.debugEnabled ? 1 : 0,
    data.folder ? 1 : 0,
  );
  postMessage({ opened, bytes });
};
