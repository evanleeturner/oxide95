// SPDX-License-Identifier: MIT
// `node --test`: the fence on the planner, no browser. The rate is the mix's 44100,
// so one block of 1024 frames lands about 23.2 ms ahead.
import assert from "node:assert/strict";
import test from "node:test";

import { LEAD_CAP_S, planBlock, toChannels } from "./sound.mjs";

const RATE = 44100;
const BLOCK_S = 1024 / RATE;
// A float's rounding on a sum of seconds, far under one frame's 22.7 us.
const ROUNDING_S = 1e-9;
// How far off a whole frame a block may read, in frames: a float's rounding alone.
const WHOLE_FRAME_TOLERANCE = 1e-6;

function offWholeFrame(seconds, rate) {
  const frames = seconds * rate;
  return Math.abs(frames - Math.round(frames));
}

test("blocks join with no gap while the schedule is ahead of the clock", () => {
  const first = planBlock(0, 735, 1, RATE);
  assert.ok(
    Math.abs(first.when - (1 + BLOCK_S)) <= ROUNDING_S,
    "the first lands one block ahead",
  );
  const second = planBlock(first.next, 735, 1.001, RATE);
  assert.equal(second.when, first.next);
  assert.ok(Math.abs(second.next - (first.next + 735 / RATE)) <= ROUNDING_S);
  const third = planBlock(second.next, 735, 1.002, RATE);
  assert.equal(third.when, second.next);
});

test("a late block moves forward and never overlaps", () => {
  const first = planBlock(0, 735, 1, RATE);
  const late = first.next + 0.01;
  const moved = planBlock(first.next, 735, late, RATE);
  const ahead = moved.when - (late + BLOCK_S);
  assert.ok(ahead >= 0 && ahead <= 1 / RATE, "up to the next whole frame");
  assert.ok(moved.when > first.next);
  assert.equal(moved.dropped, 0);
});

test("a block ending exactly at the clock still joins", () => {
  const joined = planBlock(2, 735, 2, RATE);
  assert.equal(joined.when, 2);
});

test("every frame is scheduled or counted dropped", () => {
  for (const frames of [1, 735, 1024, 11025, 16384]) {
    for (const now of [0, 0.1, 5]) {
      const plan = planBlock(0.05, frames, now, RATE);
      assert.equal(plan.kept + plan.dropped, frames, `${frames} at ${now}`);
      assert.ok(plan.kept >= 0 && plan.dropped >= 0);
    }
  }
});

test("an empty read starts nothing and moves nothing", () => {
  const plan = planBlock(1.5, 0, 1, RATE);
  assert.equal(plan.when, null);
  assert.equal(plan.next, 1.5);
  assert.equal(plan.kept + plan.dropped, 0);
});

// The ring gives 16384 frames, 371.5 ms of them, and the cap holds 250 ms less the one
// block's lead: 11025 less 1024 is 10001 frames, 1 either way for a float's rounding.
test("after a 3 s stall the lead is under its cap after one read", () => {
  const before = planBlock(0, 735, 1, RATE);
  const now = before.next + 3;
  const plan = planBlock(before.next, 16384, now, RATE);
  assert.ok(Math.abs(plan.kept - 10001) <= 1, `kept ${plan.kept}`);
  assert.equal(plan.kept + plan.dropped, 16384);
  assert.ok(plan.next - now <= LEAD_CAP_S, `lead ${plan.next - now}`);
  assert.ok(plan.dropped > 0, "the frames past it are counted");
});

test("a schedule already at its cap starts nothing and counts it all", () => {
  const plan = planBlock(1 + LEAD_CAP_S, 735, 1, RATE);
  assert.equal(plan.when, null);
  assert.equal(plan.dropped, 735);
  assert.equal(plan.next, 1 + LEAD_CAP_S, "the schedule stays where it was");
});

test("samples become floats as exactly v / 32768, left and right split", () => {
  const samples = Int16Array.of(-32768, 32767, 16384, -16384, 0, 1, 99, 99);
  const { left, right } = toChannels(samples, 3);
  assert.deepEqual([...left], [-1, 16384 / 32768, 0]);
  assert.deepEqual([...right], [32767 / 32768, -16384 / 32768, 1 / 32768]);
  assert.equal(left.length, 3, "the frames asked, not the view's whole length");
});

// One long fixed run, not a random one, so a red names the same block each time.
function fixedRun(rate) {
  const blocks = [];
  let seed = 1;
  let next = 0;
  let now = 1.000123;
  for (let block = 0; block < 10000; block += 1) {
    seed = (seed * 48271) % 2147483647;
    const frames = 1 + (seed % 2048);
    const plan = planBlock(next, frames, now, rate);
    blocks.push({ before: next, frames, plan });
    next = plan.next;
    now += (seed % 7 === 0 ? 0.4 : 0.016) + (seed % 1000) * 1e-7;
  }
  return blocks;
}

for (const rate of [44100, 48000]) {
  test(`every block lies on a whole frame at ${rate}, and joins exactly`, () => {
    let joined = 0;
    for (const { before, frames, plan } of fixedRun(rate)) {
      assert.equal(plan.kept + plan.dropped, frames);
      if (plan.when === null) continue;
      assert.ok(offWholeFrame(plan.when, rate) <= WHOLE_FRAME_TOLERANCE, `when ${plan.when}`);
      assert.ok(offWholeFrame(plan.next, rate) <= WHOLE_FRAME_TOLERANCE, `next ${plan.next}`);
      assert.ok(plan.when >= before, "no overlap");
      if (plan.when === before) joined += 1;
    }
    assert.ok(joined > 100, `the run must hold joins to prove them: ${joined}`);
  });
}

// The planted fault: the old sum of seconds, read by the same fence.
test("a raw when off the clock reads red on the whole-frame fence", () => {
  const raw = 1.000123 + BLOCK_S;
  assert.ok(offWholeFrame(raw, RATE) > WHOLE_FRAME_TOLERANCE);
  const plan = planBlock(0, 735, 1.000123, RATE);
  assert.ok(offWholeFrame(plan.when, RATE) <= WHOLE_FRAME_TOLERANCE);
});
