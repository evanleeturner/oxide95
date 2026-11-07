// SPDX-License-Identifier: MIT
// The Worker: it takes one message, then the wasm's `run` never returns, so no later
// message can reach it. Pictures go out as bitmaps, one message a frame: a blocked
// Worker's `transferControlToOffscreen` canvas showed no frame on headless Chromium
// 148 with SwiftShader, and a bitmap did. The imports here are the table at the head
// of `src/lib.rs`.
import { SIT, YIELDED } from "./crossing.mjs";
import { logger } from "./track-logger.mjs";

const WIDTH = 640;
const HEIGHT = 480;
const POLL_MS = 4;
const TRIES = 20;
const TRY_GAP_MS = 50;
const LEVELS = [null, logger.error, logger.warn, logger.info, logger.debug];

const VERTEX = `#version 300 es
out vec2 place;
void main() {
  vec2 corner = vec2((gl_VertexID << 1) & 2, gl_VertexID & 2);
  place = vec2(corner.x, 1.0 - corner.y);
  gl_Position = vec4(corner * 2.0 - 1.0, 0.0, 1.0);
}`;
const FRAGMENT = `#version 300 es
precision mediump float;
in vec2 place;
out vec4 colour;
uniform sampler2D picture;
void main() {
  colour = texture(picture, place);
}`;

function compiled(gl, kind, text) {
  const shader = gl.createShader(kind);
  gl.shaderSource(shader, text);
  gl.compileShader(shader);
  return shader;
}

function prepare(gl) {
  const program = gl.createProgram();
  gl.attachShader(program, compiled(gl, gl.VERTEX_SHADER, VERTEX));
  gl.attachShader(program, compiled(gl, gl.FRAGMENT_SHADER, FRAGMENT));
  gl.linkProgram(program);
  gl.useProgram(program);
  gl.bindTexture(gl.TEXTURE_2D, gl.createTexture());
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
  gl.texStorage2D(gl.TEXTURE_2D, 1, gl.RGBA8, WIDTH, HEIGHT);
}

// A fresh browser's first ask from a Worker has answered null, so it asks again, a
// bounded number of times, each refusal a line. At the limit the track stops and says
// so; it never falls back to another kind of picture without being told to.
function askForPicture(words) {
  for (let attempt = 1; attempt <= TRIES; attempt += 1) {
    const canvas = new OffscreenCanvas(WIDTH, HEIGHT);
    const gl = canvas.getContext("webgl2", { alpha: false, antialias: false });
    if (gl) {
      if (logger.debugEnabled) {
        const renderer = gl.getParameter(gl.RENDERER);
        logger.debug("gl.ready", { attempt, renderer });
      }
      return gl;
    }
    logger.warn("gl.refused", { attempt });
    Atomics.wait(words, SIT, 0, TRY_GAP_MS);
  }
  return null;
}

function imports(gl, words, file, wasmMemory) {
  const reader = file ? new FileReaderSync() : null;
  const decoder = new TextDecoder();
  return {
    word_load: (word) => Atomics.load(words, word),
    word_store: (word, value) => {
      Atomics.store(words, word, value);
    },
    sit: (millis) => Atomics.wait(words, SIT, 0, millis),
    now: () => performance.now(),
    present: (at, width, height) => {
      const rgba = new Uint8Array(wasmMemory(), at, width * height * 4);
      gl.texSubImage2D(
        gl.TEXTURE_2D,
        0,
        0,
        0,
        width,
        height,
        gl.RGBA,
        gl.UNSIGNED_BYTE,
        rgba,
      );
      gl.drawArrays(gl.TRIANGLES, 0, 3);
      const bitmap = gl.canvas.transferToImageBitmap();
      postMessage({ bitmap }, [bitmap]);
    },
    file_size: () => (file ? file.size : -1),
    read_at: (at, into, length) => {
      try {
        const bytes = reader.readAsArrayBuffer(file.slice(at, at + length));
        if (bytes.byteLength !== length) return 1;
        new Uint8Array(wasmMemory(), into, length).set(new Uint8Array(bytes));
        return 0;
      } catch (error) {
        logger.warn("file.read.failed", { reason: error.name });
        return 1;
      }
    },
    line: (level, at, length) => {
      const text = decoder.decode(new Uint8Array(wasmMemory(), at, length));
      LEVELS[level]?.(text);
    },
  };
}

onmessage = ({ data }) => {
  const words = new Int32Array(data.buffer);
  setTimeout(() => Atomics.store(words, YIELDED, 1), 0);
  const gl = askForPicture(words);
  if (!gl) {
    logger.error("gl.gave.up", { tries: TRIES });
    postMessage({ stopped: "picture" });
    return;
  }
  prepare(gl);
  let instance = null;
  const env = imports(
    gl,
    words,
    data.file,
    () => instance.exports.memory.buffer,
  );
  instance = new WebAssembly.Instance(data.module, { env });
  try {
    instance.exports.run(logger.debugEnabled ? 1 : 0, POLL_MS);
  } catch (error) {
    logger.error("track.stopped", {
      reason: error.name,
      detail: error.message,
    });
    postMessage({ stopped: "run" });
  }
};
