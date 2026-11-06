// SPDX-License-Identifier: MIT
// The one file that touches the console. INFO always passes; DEBUG only on a
// page opened with `?log=debug`.
//
// Call: logger.info(...values), logger.debug(...values), logger.debugEnabled.
// Values pass as arguments: guard a computed one by logger.debugEnabled.
const debugEnabled =
  new URLSearchParams(window.location.search).get("log") === "debug";

export const logger = {
  debugEnabled,
  info: (...values) => console.info(...values),
  debug: (...values) => {
    if (debugEnabled) console.debug(...values);
  },
};
