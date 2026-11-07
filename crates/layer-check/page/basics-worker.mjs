// SPDX-License-Identifier: MIT
// The other threads for `basics-page.mjs`, which starts this file as module Workers. Each
// message names one job and each gets one answer, or `refused` with why.

const lockFor = (cells) => cells.lock;

function take(view, lock) {
  while (Atomics.compareExchange(view, lock, 0, 1) !== 0);
}

function letGo(view, lock) {
  Atomics.store(view, lock, 0);
}

function sameBytes(one, two) {
  return one.length === two.length && one.every((byte, at) => byte === two[at]);
}

const JOBS = {
  /** Writes one file by a handle only a Worker may open: the main thread reads it. */
  async write({ name, bytes }) {
    const folder = await navigator.storage.getDirectory();
    const file = await folder.getFileHandle(name, { create: true });
    const access = await file.createSyncAccessHandle();
    access.truncate(0);
    const wrote = access.write(bytes, { at: 0 });
    access.flush();
    access.close();
    return { wrote };
  },

  /** Takes the lock, tells the other side, and reports what that side did during the hold. */
  hold({ view, cells, refusals, limitMs }) {
    const lock = lockFor(cells, 0);
    take(view, lock);
    Atomics.store(view, cells.held, 1);
    Atomics.notify(view, cells.held);
    const started = performance.now();
    while (
      Atomics.load(view, cells.refused) < refusals &&
      Atomics.load(view, cells.entered) === 0 &&
      performance.now() - started < limitMs
    );
    const answer = {
      refused: Atomics.load(view, cells.refused),
      inWhileHeld: Atomics.load(view, cells.entered) === 1,
    };
    letGo(view, lock);
    return answer;
  },

  /** Knocks at the lock once it is held, counting each refusal, until it gets in. */
  knock({ view, cells }) {
    Atomics.wait(view, cells.held, 0);
    const lock = lockFor(cells, 1);
    while (Atomics.compareExchange(view, lock, 0, 1) !== 0) {
      Atomics.add(view, cells.refused, 1);
    }
    Atomics.store(view, cells.entered, 1);
    letGo(view, lock);
    return {};
  },

  /** Takes turns at one plain count: with no lock between 2 sides, turns are lost. */
  count({ view, cells, rounds, side }) {
    Atomics.wait(view, cells.go, 0);
    const lock = lockFor(cells, side);
    for (let round = 0; round < rounds; round += 1) {
      take(view, lock);
      const seen = view[cells.count];
      view[cells.count] = seen + 1;
      letGo(view, lock);
    }
    return {};
  },

  /** Reads the main thread's memory from this thread. */
  readMemory({ memory, wanted }) {
    const seen = new Uint8Array(memory.buffer, 0, wanted.length);
    return { same: sameBytes(seen, wanted) };
  },
};

self.onmessage = async ({ data }) => {
  try {
    self.postMessage({ answer: await JOBS[data.job](data) });
  } catch (why) {
    self.postMessage({ refused: String(why) });
  }
};
