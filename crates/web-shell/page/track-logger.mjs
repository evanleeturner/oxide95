// SPDX-License-Identifier: MIT
// The one file of this page that touches the console, on the page and on its Worker
// alike. It is not named `logger.mjs` because it differs from the sister pages' shim of
// that name, as a Worker has no `window` and this page has WARN and ERROR lines, and
// `layer-check`'s copy refuses two `logger.mjs` that differ. INFO always passes; DEBUG
// only when opened with `?log=debug`, which the page hands on in the Worker's address.
//
// Call: logger.error(...values), logger.warn(...values), logger.info(...values),
// logger.debug(...values), logger.debugEnabled.
// Values pass as arguments: guard a computed one by logger.debugEnabled.
const debugEnabled =
  new URLSearchParams(globalThis.location.search).get("log") === "debug";

export const logger = {
  debugEnabled,
  error: (...values) => console.error(...values),
  warn: (...values) => console.warn(...values),
  info: (...values) => console.info(...values),
  debug: (...values) => {
    if (debugEnabled) console.debug(...values);
  },
};
