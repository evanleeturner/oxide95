// SPDX-License-Identifier: MIT
// The page's half of real pads. `getGamepads` is on `Navigator`, not on
// `WorkerNavigator`, and a game runs in a Worker, so this reads the pads once a
// frame and hands their numbers across; the crate's `page.rs` holds them.
//
// A pad only adds, so this touches no element, listens for no key and moves no
// focus. It reads `navigator` and asks for the next frame, and nothing else.
//
// The console is not touched here: the page hands in its own logger.
//
// Call: padWords(gamepads) -> numbers, pure.
//       startPolling(window, post, logger) -> stop(), where post(numbers) sends
//       a frame's numbers to the Worker and logger.info(...values) is told once
//       if the browser refuses the pads.
const SLOTS = 4;
const WORDS_A_PAD = 11;
const MAX_BUTTONS = 32;

// `JOYCAPS_*`, the bits of `wCaps`.
const HAS_Z = 0x1;
const HAS_R = 0x2;
const HAS_U = 0x4;
const HAS_V = 0x8;
const HAS_POV = 0x10;
const HAS_POV_FOUR_WAYS = 0x20;

// `dwPOV` for each way the hat can point, and for none.
const POV_FORWARD = 0;
const POV_RIGHT = 9000;
const POV_BACK = 18000;
const POV_LEFT = 27000;
const POV_CENTRED = 0xffff;

// The standard mapping's four direction buttons, which become the hat.
const DPAD_UP = 12;
const DPAD_DOWN = 13;
const DPAD_LEFT = 14;
const DPAD_RIGHT = 15;

const CENTRE = 32768;

// A stick from -1 to 1 as a number from 0 to 65535: the same sum as `stick` in
// `pads.rs`. A number that is no number reads as a stick at rest.
function stick(value) {
  if (!Number.isFinite(value)) return CENTRE;
  const inRange = Math.min(1, Math.max(-1, value));
  return Math.round((inRange + 1) * 32767.5);
}

// One bit a button, up to as many as a word holds.
function buttonBits(buttons) {
  let bits = 0;
  buttons.slice(0, MAX_BUTTONS).forEach((button, at) => {
    if (button.pressed) bits = (bits | (1 << at)) >>> 0;
  });
  return bits;
}

// A diagonal has no four-way answer: the first of up, right, down, left.
function hat(buttons) {
  const down = (at) => buttons[at]?.pressed === true;
  if (down(DPAD_UP)) return POV_FORWARD;
  if (down(DPAD_RIGHT)) return POV_RIGHT;
  if (down(DPAD_DOWN)) return POV_BACK;
  if (down(DPAD_LEFT)) return POV_LEFT;
  return POV_CENTRED;
}

function standardWords(axes, buttons) {
  const kept = buttons.filter((_, at) => at < DPAD_UP || at > DPAD_RIGHT);
  return [
    1,
    stick(axes[0]),
    stick(axes[1]),
    CENTRE,
    stick(axes[2]),
    stick(axes[3]),
    CENTRE,
    HAS_R | HAS_U | HAS_POV | HAS_POV_FOUR_WAYS,
    Math.min(MAX_BUTTONS, kept.length),
    buttonBits(kept),
    hat(buttons),
  ];
}

// With no mapping the axes and buttons go straight across, and there is no hat.
function plainWords(axes, buttons) {
  const caps =
    (axes.length > 2 ? HAS_Z : 0) |
    (axes.length > 3 ? HAS_R : 0) |
    (axes.length > 4 ? HAS_U : 0) |
    (axes.length > 5 ? HAS_V : 0);
  return [
    1,
    stick(axes[0]),
    stick(axes[1]),
    stick(axes[2]),
    stick(axes[3]),
    stick(axes[4]),
    stick(axes[5]),
    caps,
    Math.min(MAX_BUTTONS, buttons.length),
    buttonBits(buttons),
    POV_CENTRED,
  ];
}

function slotWords(pad) {
  if (!pad || pad.connected === false) return new Array(WORDS_A_PAD).fill(0);
  const axes = Array.from(pad.axes ?? []);
  const buttons = Array.from(pad.buttons ?? []);
  return pad.mapping === "standard"
    ? standardWords(axes, buttons)
    : plainWords(axes, buttons);
}

// What `getGamepads` gave, as the numbers `PagePads::hand_in` reads.
export function padWords(gamepads) {
  return Array.from(gamepads ?? [])
    .slice(0, SLOTS)
    .flatMap(slotWords);
}

// A browser may lack the call, or refuse it where the page may not use pads.
// Either is no pads, since a pad only adds.
function readGamepads(navigator, refused) {
  try {
    return navigator?.getGamepads?.() ?? [];
  } catch (error) {
    refused(error);
    return [];
  }
}

export function startPolling(window, post, logger) {
  let told = false;
  const refused = (error) => {
    if (!told) logger.info("pads refused", String(error));
    told = true;
  };
  let frame = 0;
  const step = () => {
    post(padWords(readGamepads(window.navigator, refused)));
    frame = window.requestAnimationFrame(step);
  };
  frame = window.requestAnimationFrame(step);
  return () => window.cancelAnimationFrame(frame);
}
