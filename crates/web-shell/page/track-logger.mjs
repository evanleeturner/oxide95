// SPDX-License-Identifier: MIT
// The one file of this page that touches the console, on the page and on its Worker
// alike. It is not named `logger.mjs` because it differs from the sister pages' shim of
// that name, as a Worker has no `window` and this page has WARN and ERROR lines, and
// `layer-check`'s copy refuses two `logger.mjs` that differ. INFO always passes; DEBUG
// only when opened with `?log=debug`, which the page hands on in the Worker's address.
//
// Each line that passes is also kept as text, its level first, in the order written:
// the run's trace, which `save` writes out as a file. A Worker's trace ends with the
// Worker, so a Worker sets `forward` to hand each line on to its page, which keeps it by
// `take` beside its own: a Worker stopped before it answered leaves its lines up to the
// stop. The trace is kept whole, with no cap.
//
// Call: logger.error(...values), logger.warn(...values), logger.info(...values),
// logger.debug(...values), logger.debugEnabled; logger.forward = (line) => ...;
// logger.take(line), logger.trace(), logger.save(name).
// Values pass as arguments: guard a computed one by logger.debugEnabled.
const debugEnabled =
  new URLSearchParams(globalThis.location.search).get("log") === "debug";

/** The trace: each line kept, in the order written or taken. */
const lines = [];

/** A value as the trace writes it: text as it is, any other value as JSON. */
function shown(value) {
  return typeof value === "string" ? value : JSON.stringify(value);
}

/** Keeps one line, its level first, and hands it on when a forward is set. */
function keep(level, values) {
  const line = [level, ...values.map(shown)].join(" ");
  lines.push(line);
  logger.forward?.(line);
}

export const logger = {
  debugEnabled,
  /** Set on a Worker to hand each kept line on to its page; null keeps it here alone. */
  forward: null,
  error: (...values) => {
    console.error(...values);
    keep("ERROR", values);
  },
  warn: (...values) => {
    console.warn(...values);
    keep("WARN", values);
  },
  info: (...values) => {
    console.info(...values);
    keep("INFO", values);
  },
  debug: (...values) => {
    if (!debugEnabled) return;
    console.debug(...values);
    keep("DEBUG", values);
  },
  /** Keeps a line another thread's logger kept and handed on; it is not printed again. */
  take: (line) => {
    lines.push(line);
  },
  /** The trace as text, one line each. */
  trace: () => lines.map((line) => `${line}\n`).join(""),
  /** Writes the trace out as a text file named `name`, through the browser's download. */
  save: (name) => {
    const address = URL.createObjectURL(
      new Blob([logger.trace()], { type: "text/plain" }),
    );
    const link = document.createElement("a");
    link.href = address;
    link.download = name;
    link.click();
    setTimeout(() => URL.revokeObjectURL(address), 0);
  },
};
