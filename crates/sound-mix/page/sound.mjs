// SPDX-License-Identifier: MIT
// The page's sound: it reads the mix once an animation frame and schedules it ahead
// as AudioBufferSourceNodes, the context resumed at the first gesture. `planBlock` and
// `toChannels` are pure and touch no browser; `attachSound` is the thin wiring.
//
// Call: attachSound({ wasm, logger, onChange }) -> a handle with `state`, `muted`,
// `setMuted(boolean)` and `stats`. `wasm` holds the program's exports: `memory`,
// `rate()`, `tick(now_ms)`, `gesture()`, `read_frames()` and `frames_ptr()`.
// `onChange` runs when the context's state or the mute changes.
//
// Who owns an input: the listeners only watch. They are passive, so they cannot
// call `preventDefault`, and a key still does what it does on any page. A pointer
// counts at `click`, never at a down-event, so a slip of the pointer can be taken
// back. Escape is no gesture: the HTML standard's activation list leaves it out, so
// a resume on it would only wait, and it should not start a page's sound anyway.
//
// Not done: joining a game's page and `draw`'s script, and no one has heard this.

const FORMAT_MAX = 32768;
// One block, the most the mix renders at a time: how far ahead a block lands when the
// schedule has fallen behind.
const START_LEAD_FRAMES = 1024;
// The most a schedule runs ahead of the clock. A long stall hands back up to the
// ring's 370 ms of old sound, and this is what keeps that from lagging all that follows.
export const LEAD_CAP_S = 0.25;
const LISTEN = { capture: true, passive: true };

// Where a read's `frames` go, given the schedule's end `next` and the context's
// `now`, in seconds. A block joins the last one while that has not ended, and
// otherwise lands one block ahead; frames past the cap are dropped and counted.
// `when` and `next` lie on whole frames: Chromium interpolates a block that starts
// between 2 frames, which alters every sample. They come from a whole count each
// time, so no float's rounding gathers. `when` is null if nothing should start.
export function planBlock(next, frames, now, rate) {
  const whenFrame =
    next >= now
      ? Math.round(next * rate)
      : Math.ceil(now * rate) + START_LEAD_FRAMES;
  const when = whenFrame / rate;
  const room = Math.floor((now + LEAD_CAP_S - when) * rate);
  const kept = Math.max(0, Math.min(frames, room));
  if (kept === 0) return { when: null, kept: 0, dropped: frames, next };
  return { when, kept, dropped: frames - kept, next: (whenFrame + kept) / rate };
}

// The first `frames` of interleaved `i16` samples as two float channels.
export function toChannels(samples, frames) {
  const left = new Float32Array(frames);
  const right = new Float32Array(frames);
  for (let frame = 0; frame < frames; frame += 1) {
    left[frame] = samples[frame * 2] / FORMAT_MAX;
    right[frame] = samples[frame * 2 + 1] / FORMAT_MAX;
  }
  return { left, right };
}

export function attachSound({ wasm, logger, onChange = () => {} }) {
  const rate = wasm.rate();
  const context = new AudioContext({ sampleRate: rate });
  const gain = context.createGain();
  gain.connect(context.destination);
  const stats = { frames: 0, dropped: 0, nodes: 0, reads: 0 };
  let next = 0;
  let began = false;
  let muted = false;

  const wake = () => {
    if (context.state !== "running") context.resume();
  };
  const onKey = (event) => {
    if (event.key !== "Escape") wake();
  };
  window.addEventListener("keydown", onKey, LISTEN);
  window.addEventListener("click", wake, LISTEN);

  context.addEventListener("statechange", () => {
    logger.info("sound: the context is", context.state);
    if (context.state === "running" && !began) {
      began = true;
      wasm.gesture();
    }
    onChange();
  });

  const schedule = (frames) => {
    const plan = planBlock(next, frames, context.currentTime, rate);
    next = plan.next;
    stats.dropped += plan.dropped;
    if (plan.when === null) return;
    const samples = new Int16Array(
      wasm.memory.buffer,
      wasm.frames_ptr(),
      plan.kept * 2,
    );
    const { left, right } = toChannels(samples, plan.kept);
    const buffer = context.createBuffer(2, plan.kept, rate);
    buffer.copyToChannel(left, 0);
    buffer.copyToChannel(right, 1);
    const node = context.createBufferSource();
    node.buffer = buffer;
    node.connect(gain);
    node.start(plan.when);
    stats.frames += plan.kept;
    stats.nodes += 1;
    logger.debug("sound: block", plan.when, plan.kept, plan.dropped);
  };

  const frame = () => {
    wasm.tick(performance.now());
    const frames = wasm.read_frames();
    stats.reads += 1;
    if (frames > 0) schedule(frames);
    requestAnimationFrame(frame);
  };
  requestAnimationFrame(frame);

  return {
    stats,
    get state() {
      return context.state;
    },
    get now() {
      return context.currentTime;
    },
    get muted() {
      return muted;
    },
    setMuted(value) {
      muted = value;
      gain.gain.value = muted ? 0 : 1;
      onChange();
    },
  };
}
