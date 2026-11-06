// SPDX-License-Identifier: MIT
// The walk's page: the thin wasm's bar and box as real elements. Each answer goes
// back through an export, and what the page shows next is read from the wasm.
//
// Who owns a key: real buttons and check boxes take Tab, Space and Return as they
// do on any page, `focusin` telling the desk where the focus went. Only what
// `boxKeyCode` names goes through `key`. A bar's Escape gives the canvas the focus.
//
// Not done: a program's own `SetFocus` while a box is open, and a new `DrawMenuBar`,
// do not reach the elements; no program in the walk makes either call.
//
// Exposes `window.pageWalk`, the exports, for `walk-browser.mjs` to read.
import { logger } from "./logger.mjs";
import { barFromText, boxFromText, boxKeyCode } from "./walk-records.mjs";

const WASM_URL = "page_walk.wasm";
// The wasm's stand-in program keeps a pick and opens nothing: this row opening
// the box is the walk's, as a program's procedure would call `DialogBoxParamA`.
const ROW_OPENING_THE_BOX = 0x10;

const bar = document.querySelector(".js-bar");
const canvas = document.querySelector(".js-screen");
const dialog = document.querySelector(".js-box");
const title = document.querySelector(".js-title");
const controls = document.querySelector(".js-controls");

const { instance } = await WebAssembly.instantiateStreaming(fetch(WASM_URL));
const wasm = instance.exports;
window.pageWalk = wasm;

const textOf = (length) =>
  new TextDecoder().decode(
    new Uint8Array(wasm.memory.buffer, wasm.text_ptr(), length),
  );

let openHandle = 0;

function element(tag, className, words = "") {
  const made = document.createElement(tag);
  made.className = className;
  made.textContent = words;
  return made;
}

function controlOf(made, control) {
  made.classList.add("js-control");
  made.dataset.id = control.id;
  made.dataset.handle = control.handle;
  made.autofocus = control.focused;
  return made;
}

function controlElement(control) {
  if (control.kind === "label") {
    return element("p", "box-label", control.words);
  }
  if (control.kind === "check") {
    const label = element("label", "box-check");
    const input = controlOf(element("input", "box-input"), control);
    input.type = "checkbox";
    input.checked = control.checked;
    label.append(input, control.words);
    return label;
  }
  const look = control.kind === "default" ? "is-default" : "is-plain";
  const button = controlOf(element("button", look, control.words), control);
  button.type = "button";
  return button;
}

function showBox(handle) {
  const found = boxFromText(textOf(wasm.box_text(handle)));
  openHandle = handle;
  dialog.dataset.handle = handle;
  title.textContent = found.title;
  controls.replaceChildren(...found.controls.map(controlElement));
  dialog.showModal();
  logger.info("box opened", handle, found.title);
}

function afterAnswer() {
  wasm.run();
  const found = boxFromText(textOf(wasm.box_text(openHandle)));
  if (!found) {
    logger.info("box closed", wasm.ended() >>> 0);
    dialog.close();
    return;
  }
  for (const input of controls.querySelectorAll("input")) {
    const row = found.controls.find(
      (control) => control.id === Number(input.dataset.id),
    );
    input.checked = row.checked;
  }
}

function sendKey(code) {
  logger.debug("box key", openHandle, code);
  wasm.key(openHandle, code);
  afterAnswer();
}

function rowElement(row) {
  const item = element("li", "bar-row");
  if (row.kind === "separator") {
    item.append(element("hr", "bar-rule"));
    return item;
  }
  const button = element("button", "bar-button", row.words);
  button.type = "button";
  button.disabled = !row.enabled;
  item.append(button);
  if (row.kind === "popup") {
    const rows = rowsElement(row.rows);
    rows.classList.add("bar-popup");
    rows.hidden = true;
    button.setAttribute("aria-expanded", "false");
    item.append(rows);
    return item;
  }
  button.dataset.row = row.id;
  if (row.checked) {
    button.classList.add("is-checked");
    button.setAttribute("aria-pressed", "true");
  }
  return item;
}

function rowsElement(rows) {
  const list = element("ul", "bar-rows");
  list.append(...rows.map(rowElement));
  return list;
}

function setPopup(button, open) {
  button.setAttribute("aria-expanded", String(open));
  button.nextElementSibling.hidden = !open;
}

function closePopups() {
  for (const open of bar.querySelectorAll("[aria-expanded='true']")) {
    setPopup(open, false);
  }
}

function pickRow(id) {
  canvas.focus();
  closePopups();
  logger.info("row picked", id);
  wasm.pick(id);
  wasm.run();
  if (wasm.picked() >>> 0 === ROW_OPENING_THE_BOX) {
    showBox(wasm.open_box() >>> 0);
  }
}

bar.addEventListener("click", (event) => {
  const button = event.target.closest("button");
  if (!button) return;
  if (button.dataset.row !== undefined) {
    pickRow(Number(button.dataset.row));
    return;
  }
  const opening = button.getAttribute("aria-expanded") === "false";
  closePopups();
  setPopup(button, opening);
});

bar.addEventListener("keydown", (event) => {
  if (event.key !== "Escape") return;
  canvas.focus();
  closePopups();
});

dialog.addEventListener("focusin", (event) => {
  const { handle } = event.target.dataset;
  if (handle !== undefined) wasm.focus(Number(handle));
});

dialog.addEventListener("click", (event) => {
  const { id } = event.target.dataset;
  if (id === undefined) return;
  wasm.press(openHandle, Number(id));
  afterAnswer();
});

// Escape's keydown is taken here, as a `cancel` the browser may skip: Chrome
// closes a dialog with no `cancel` on a second close with no user activation.
dialog.addEventListener("keydown", (event) => {
  const code = boxKeyCode(event.key, event.target.type);
  if (code === null) return;
  event.preventDefault();
  sendKey(code);
});

dialog.addEventListener("cancel", (event) => {
  event.preventDefault();
  sendKey(boxKeyCode("Escape"));
});

bar.append(rowsElement(barFromText(textOf(wasm.bar_text())).rows));
document.documentElement.dataset.ready = "true";
logger.info("page ready");
