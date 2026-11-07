// SPDX-License-Identifier: MIT
// The sentences a person reads, and the report they may pass on. Pure: no page, no
// console, so the test harness under Node reads the same words the page shows.
//
// `sentence` promises the same words for the same check, every time. A device is
// never named here: `publicName` gives "Speaker 1", since a device's name can carry a
// person's name. `reportHoldsName` is what stops a report that leaks one anyway.

const SENTENCES = {
  C1: "This address is not secure, so your browser hides its sound devices. Open the page at its https address.",
  C2: "Your browser has no {feature}, which this needs. A recent Chrome or Firefox has it.",
  C3: "Your browser refused to start this page's code. An add-on or an office policy may block it.",
  C4: "Your browser's sound engine did not start. Close other tabs or restart the browser.",
  C5: "Your browser made a test tone and could not read it back. The fault is in the browser, and no code from this project ran.",
  C6: "You counted a different number of beeps from the ones played. Check the volume and which speaker is chosen.",
  C10: "The microphone heard nothing while the beeps played. The speaker, the volume or the microphone may be off.",
  H1: "Your browser reports no speakers or headphones. Plug one in, or allow the page to see them.",
  H2: "A device was plugged in and this page did not notice. That is a fault in this page.",
  H3: "Your browser never answered about the microphone. The check goes on without it.",
  P1: "This page started sound before you asked for it. That is a fault in this page.",
  P2: "This page tried to send something out, and it was stopped. That is a fault in this page.",
  P3: "The report holds a device's name. Nothing was sent. That is a fault in this page.",
  P4: "A control on this page has no name a screen reader can say. That is a fault in this page.",
};

const PASSES = {
  C1: "This address is secure.",
  C2: "Your browser has what this needs.",
  C3: "Your browser allowed this page's code to start.",
  C4: "Your browser's sound engine is running. This does not show that sound can be heard.",
  C5: "Your browser made a test tone and read it back.",
  C6: "You counted the beeps that were played.",
  C10: "The microphone heard the beeps.",
  H1: "Your browser reports at least 1 speaker or pair of headphones. This does not show that sound can be heard.",
  H2: "This page reacted to a test signal that a device was plugged in.",
  H3: "Your browser answered about the microphone.",
};

const NAMES = {
  C1: "Secure address",
  C2: "Browser features",
  C3: "Page code allowed",
  C4: "Sound engine",
  C5: "Test tone",
  C6: "Beep count",
  C10: "Microphone",
  H1: "Speakers and headphones",
  H2: "Plugging in",
  H3: "Microphone question",
  P2: "Nothing sent",
};

export function sentence(check, values = {}) {
  const words = SENTENCES[check];
  if (!words) throw new Error(`no sentence for check ${check}`);
  return words.replaceAll("{feature}", () => {
    if (!values.feature) throw new Error(`check ${check} needs a feature`);
    return values.feature;
  });
}

export function passSentence(check) {
  const words = PASSES[check];
  if (!words) throw new Error(`no pass sentence for check ${check}`);
  return words;
}

export function checkName(check) {
  return NAMES[check] ?? check;
}

export function publicName(device, index) {
  const kind = device.kind === "audiooutput" ? "Speaker" : "Microphone";
  return `${kind} ${index + 1}`;
}

/** True when any device's real label appears in the text. An empty label never counts. */
export function reportHoldsName(text, labels) {
  return labels.some((label) => label.length > 0 && text.includes(label));
}

/** The report's text: one line per result, one per device, with no label in any. */
export function buildReport(results, devices) {
  const lines = ["Sound check report"];
  for (const { check, state } of results) lines.push(`${check}: ${state}`);
  devices.forEach((device, index) => {
    lines.push(`Device: ${publicName(device, index)}`);
  });
  return lines.join("\n");
}
