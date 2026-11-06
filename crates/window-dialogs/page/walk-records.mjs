// SPDX-License-Identifier: MIT
// The text `view`'s `lines` write, read back, and who answers a key: data in,
// data out, no DOM and no wasm.
//
// Call: boxFromText(text) -> { handle, title, controls: [{ kind, handle, id,
// checked, focused, words }] } or null; barFromText(text) -> { window, drawn,
// rows: [{ kind, id, enabled, checked, words, rows }] } or null; null for text
// that is empty, as a box not open answers. boxKeyCode(key, focusedType) -> a
// Windows key code for `key`, or null for a key the browser's element answers.
// Does not read `view`'s places: the page lays controls out in template order.
const VK_RETURN = 0x0d;
const VK_ESCAPE = 0x1b;

const unescaped = (words) =>
  words.replace(/\\(.)/g, (_, letter) =>
    letter === "t" ? "\t" : letter === "n" ? "\n" : letter,
  );

const records = (text) =>
  text
    .split("\n")
    .filter(Boolean)
    .map((line) => line.split("\t"));

export function boxFromText(text) {
  const [head, ...controls] = records(text);
  if (!head) return null;
  return {
    handle: Number(head[1]),
    title: unescaped(head[6]),
    controls: controls.map(
      ([kind, handle, id, checked, focused, , , , , words]) => ({
        kind,
        handle: Number(handle),
        id: Number(id),
        checked: checked === "1",
        focused: focused === "1",
        words: unescaped(words),
      }),
    ),
  };
}

export function barFromText(text) {
  const [head, ...rows] = records(text);
  if (!head) return null;
  const top = [];
  const open = [top];
  for (const [kind, depth, id, enabled, checked, words] of rows) {
    const row = {
      kind,
      id: Number(id),
      enabled: enabled === "1",
      checked: checked === "1",
      words: unescaped(words),
      rows: [],
    };
    open[Number(depth)].push(row);
    open[Number(depth) + 1] = row.rows;
  }
  return { window: Number(head[1]), drawn: Number(head[2]), rows: top };
}

// A real button takes Return itself, so a reader's "Cancel" never presses OK.
// Everywhere else in a box Return is the default's, Escape everywhere Cancel's.
export function boxKeyCode(key, focusedType) {
  if (key === "Escape") return VK_ESCAPE;
  if (key === "Enter" && focusedType !== "button") return VK_RETURN;
  return null;
}
