// SPDX-License-Identifier: MIT
// A browser under the kernel stand-ins, asked with no code of ours: a clock moves, a file comes
// back, two threads exclude, a heap holds. `bench/computer_basics.rs` asks the computer the
// same outside a browser. Not part of the shipped page: `basics-browser.mjs` serves it, with
// the 2 headers that let a page share memory, and reads `window.basics`.
//
// It does not say a clock's rate is right: two clocks wrong at one steady rate agree here. A
// step of the wall clock during the reads gives a false red.

const WAIT_MS = 250;
const FILE_BYTES = 65536;
const FILE_NAME = "cc1-bytes.bin";
const REFUSALS = 1000;
const REFUSALS_LIMIT_MS = 10000;
const ROUNDS = 200000;
// `Date.now` counts whole milliseconds; `performance.now` is coarsened to 100 us at most.
const GRAIN_US = 1100;
// One block is 16 pages of a `WebAssembly.Memory`, 64 KiB each: 1 MiB.
const BLOCK_PAGES = 16;
const BLOCK_BYTES = BLOCK_PAGES * 65536;
const CELLS = { lock: 0, held: 2, refused: 3, entered: 4, count: 5, go: 6 };
const CELL_COUNT = 7;

const NOTHING = "with none of this project's code in the way.";
const CLOCK_RED = `A clock in this browser stood still or disagreed with a second clock, ${NOTHING}`;
const FILE_RED = `A file in this browser did not come back as it was written, ${NOTHING}`;
const THREAD_RED = `Two threads in this browser were not kept apart by a lock, ${NOTHING}`;
const HEAP_RED = `A block of memory in this browser did not hold what was put in it, ${NOTHING}`;
const PAGE_RED =
  "This page cannot share memory between threads, so its thread and memory rows did not run: the browser did not isolate it.";

const firstClock = () => performance.now();
const secondClock = () => Date.now();

function pattern(count, seed) {
  const bytes = new Uint8Array(count);
  let state = Math.imul(seed, 2654435761) | 1;
  for (let at = 0; at < count; at += 1) {
    state ^= state << 13;
    state ^= state >>> 17;
    state ^= state << 5;
    bytes[at] = state & 255;
  }
  return bytes;
}

function sameBytes(one, two) {
  return one.length === two.length && one.every((byte, at) => byte === two[at]);
}

function ask(worker, message) {
  return new Promise((done, failed) => {
    worker.onmessage = ({ data }) =>
      data.refused ? failed(new Error(data.refused)) : done(data.answer);
    worker.onerror = (event) => failed(new Error(event.message));
    worker.postMessage(message);
  });
}

function newWorker() {
  return new Worker(new URL("./basics-worker.mjs", import.meta.url), {
    type: "module",
  });
}

/** What each clock moved over one wait, read in `order` and then in reverse. */
async function movedOverAWait(order) {
  const started = order.map((clock) => clock());
  await new Promise((go) => setTimeout(go, WAIT_MS));
  const ended = [];
  for (let at = order.length - 1; at >= 0; at -= 1) ended[at] = order[at]();
  return ended.map((end, at) => Math.round((end - started[at]) * 1000));
}

function nestHolds({ outerUs, innerUs }) {
  return outerUs > 0 && innerUs > 0 && innerUs <= outerUs + GRAIN_US;
}

async function clockRow() {
  const [fineOutside, wallInside] = await movedOverAWait([firstClock, secondClock]);
  const [wallOutside, fineInside] = await movedOverAWait([secondClock, firstClock]);
  return [
    { outer: "performance", inner: "date", outerUs: fineOutside, innerUs: wallInside },
    { outer: "date", inner: "performance", outerUs: wallOutside, innerUs: fineInside },
  ];
}

async function fileRow(worker) {
  const made = pattern(FILE_BYTES, 1);
  const { wrote } = await ask(worker, {
    job: "write",
    name: FILE_NAME,
    bytes: made,
  });
  const folder = await navigator.storage.getDirectory();
  const handle = await folder.getFileHandle(FILE_NAME);
  const back = new Uint8Array(await (await handle.getFile()).arrayBuffer());
  return { wrote, back: back.length, same: sameBytes(back, made) };
}

async function threadRow(workers) {
  const view = new Int32Array(
    new SharedArrayBuffer(CELL_COUNT * Int32Array.BYTES_PER_ELEMENT),
  );
  const knocked = ask(workers[1], { job: "knock", view, cells: CELLS });
  const { refused, inWhileHeld } = await ask(workers[0], {
    job: "hold",
    view,
    cells: CELLS,
    refusals: REFUSALS,
    limitMs: REFUSALS_LIMIT_MS,
  });
  await knocked;
  const inOnRelease = Atomics.load(view, CELLS.entered) === 1;
  const sides = workers.map((worker, side) =>
    ask(worker, { job: "count", view, cells: CELLS, rounds: ROUNDS, side }),
  );
  Atomics.store(view, CELLS.go, 1);
  Atomics.notify(view, CELLS.go);
  await Promise.all(sides);
  return {
    refused,
    inWhileHeld,
    inOnRelease,
    counted: Atomics.load(view, CELLS.count),
  };
}

async function heapRow(worker) {
  const wanted = pattern(BLOCK_BYTES, 2);
  const memory = new WebAssembly.Memory({
    initial: BLOCK_PAGES,
    maximum: 2 * BLOCK_PAGES,
    shared: true,
  });
  new Uint8Array(memory.buffer).set(wanted);
  memory.grow(BLOCK_PAGES);
  const view = new Uint8Array(memory.buffer);
  view.set(pattern(BLOCK_BYTES, 3), BLOCK_BYTES);
  const { same } = await ask(worker, { job: "readMemory", memory, wanted });
  return {
    grew: memory.buffer.byteLength === 2 * BLOCK_BYTES,
    held: sameBytes(view.subarray(0, BLOCK_BYTES), wanted),
    otherThreadSame: same,
  };
}

/** Runs one row, turning a refusal into its own line and its red. */
async function row(name, sentence, work, read, holds) {
  try {
    const answer = await work();
    return { lines: read(answer), red: holds(answer) ? [] : [[name, sentence]] };
  } catch (why) {
    return { lines: [`CC1 ${name} refused=${why}`], red: [[name, sentence]] };
  }
}

const clockLines = (nests) =>
  nests.map(
    (nest) =>
      `CC1 clock outer=${nest.outer} inner=${nest.inner} outer_us=${nest.outerUs} inner_us=${nest.innerUs} grain_us=${GRAIN_US}`,
  );

const threadHolds = (answer) =>
  answer.refused >= REFUSALS &&
  !answer.inWhileHeld &&
  answer.inOnRelease &&
  answer.counted === 2 * ROUNDS;

async function run() {
  const isolated = self.crossOriginIsolated;
  const sab = typeof SharedArrayBuffer;
  const workers = [newWorker(), newWorker()];
  const rows = [
    await row("clock", CLOCK_RED, clockRow, clockLines, (nests) =>
      nests.every(nestHolds),
    ),
    await row(
      "file",
      FILE_RED,
      () => fileRow(workers[0]),
      (file) => [`CC1 file wrote=${file.wrote} back=${file.back} same=${file.same}`],
      (file) => file.same && file.back === file.wrote,
    ),
  ];
  let shared = isolated === true && sab === "function";
  if (shared) {
    rows.push(
      await row(
        "thread",
        THREAD_RED,
        () => threadRow(workers),
        (thread) => [
          `CC1 thread refused=${thread.refused} in_while_held=${thread.inWhileHeld} in_on_release=${thread.inOnRelease} counted=${thread.counted} of=${2 * ROUNDS}`,
        ],
        threadHolds,
      ),
      await row(
        "heap",
        HEAP_RED,
        () => heapRow(workers[1]),
        (heap) => [
          `CC1 heap grew=${heap.grew} held=${heap.held} other_thread_same=${heap.otherThreadSame}`,
        ],
        (heap) => heap.grew && heap.held && heap.otherThreadSame,
      ),
    );
  } else {
    rows.push({ lines: ["CC1 thread skipped", "CC1 heap skipped"], red: [["page", PAGE_RED]] });
  }
  for (const worker of workers) worker.terminate();
  const reds = rows.flatMap((one) => one.red);
  return [
    `CC1 host page isolated=${isolated} sab=${sab}`,
    ...rows.flatMap((one) => one.lines),
    ...(reds.length === 0
      ? ["CC1 pass"]
      : reds.map(([name, sentence]) => `CC1 red ${name} ${sentence}`)),
  ];
}

window.basics = run();
