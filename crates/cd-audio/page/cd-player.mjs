// SPDX-License-Identifier: MIT
// The page's half of a disc's sound: one `<audio>` on the main thread, driven from
// the Worker by the numbers `page.rs` posts. An image's track plays from a `Blob` of
// a 44-byte WAV header and the track's slice of its bin, so the browser streams from
// the player's own file and nothing is copied. A store's track plays its file.
//
// The position a game reads is the clock's, in the crate, so nothing here reads the
// element's time or waits on its events. A timer on the same numbers moves from
// track to track and ends a run: an element that is refused or slow changes what is
// heard and nothing else.
//
// Sound only adds, so the element joins no document, shows no control, listens for
// no key and moves no focus.
//
// The console is not touched here: the page hands in its own logger.
//
// A track, as the page hands it in, in disc order:
//   image: { number, frames, file, at, bigEndian }, `at` the byte its first frame
//          starts on in `file`
//   store: { number, frames, file }, the file itself in a form the browser plays
//
// Call: wavHeader(dataBytes) -> Uint8Array, pure.
//       runPieces(tracks, from, to) -> pieces, pure.
//       trackSource(window, track, from, to) -> what the element plays.
//       startPlayer(window, tracks, logger) -> apply(words), where words are
//       [kind, from's track, from's frame, to's track, to's frame], or for a volume
//       [kind, left, right, 0, 0].
const PLAY = 1;
const PAUSE = 2;
const STOP = 3;
const VOLUME = 4;

const HEADER_BYTES = 44;
const RATE_HZ = 44100;
const CHANNELS = 2;
const BITS = 16;
const FRAME_BYTES = 2352;
const FRAMES_A_SECOND = 75;
const FULL_VOLUME = 0xffff;

// The canonical RIFF form of plain PCM. It has no big-endian form, which is why a
// big-endian track is silent here.
export function wavHeader(dataBytes) {
  const header = new DataView(new ArrayBuffer(HEADER_BYTES));
  const text = (at, letters) => {
    Array.from(letters).forEach((letter, index) =>
      header.setUint8(at + index, letter.charCodeAt(0)),
    );
  };
  const frameBytes = (CHANNELS * BITS) / 8;
  text(0, "RIFF");
  header.setUint32(4, HEADER_BYTES - 8 + dataBytes, true);
  text(8, "WAVE");
  text(12, "fmt ");
  header.setUint32(16, 16, true);
  header.setUint16(20, 1, true);
  header.setUint16(22, CHANNELS, true);
  header.setUint32(24, RATE_HZ, true);
  header.setUint32(28, RATE_HZ * frameBytes, true);
  header.setUint16(32, frameBytes, true);
  header.setUint16(34, BITS, true);
  text(36, "data");
  header.setUint32(40, dataBytes, true);
  return new Uint8Array(header.buffer);
}

// A run as one piece a track, `from` and `to` each `{ track, frame }`. A run that
// names a track the disc lacks has no pieces: silence, never a fault.
export function runPieces(tracks, from, to) {
  const first = tracks.findIndex((track) => track.number === from.track);
  const last = tracks.findIndex((track) => track.number === to.track);
  if (first < 0 || last < first) return [];
  return tracks
    .slice(first, last + 1)
    .map((track, at, run) => ({
      track,
      from: at === 0 ? from.frame : 0,
      to: at === run.length - 1 ? Math.min(to.frame, track.frames) : track.frames,
    }))
    .filter((piece) => piece.to > piece.from);
}

// `slice` only names a range of the player's file; the browser reads it as it plays.
export function trackSource(window, track, from, to) {
  if (track.at === undefined) return track.file;
  const start = track.at + from * FRAME_BYTES;
  const bytes = (to - from) * FRAME_BYTES;
  return new window.Blob(
    [wavHeader(bytes), track.file.slice(start, start + bytes)],
    { type: "audio/wav" },
  );
}

export function startPlayer(window, tracks, logger) {
  const audio = window.document.createElement("audio");
  let timer = null;
  let url = null;

  const letGo = () => {
    if (timer !== null) window.clearTimeout(timer);
    timer = null;
    audio.pause();
    if (url !== null) window.URL.revokeObjectURL(url);
    url = null;
  };

  // A play cut off by the next order rejects with `AbortError`: no refusal.
  const refused = (error) => {
    if (error?.name !== "AbortError") logger.info("cd play refused", String(error));
  };

  const sound = ({ track, from, to }) => {
    url = window.URL.createObjectURL(trackSource(window, track, from, to));
    audio.src = url;
    if (track.at === undefined) audio.currentTime = from / FRAMES_A_SECOND;
    try {
      Promise.resolve(audio.play()).catch(refused);
    } catch (error) {
      refused(error);
    }
  };

  const run = (pieces) => {
    letGo();
    const [piece, ...rest] = pieces;
    if (piece === undefined) return;
    if (piece.track.bigEndian) {
      logger.info("cd track silent, big-endian", piece.track.number);
    } else {
      sound(piece);
    }
    const ms = ((piece.to - piece.from) * 1000) / FRAMES_A_SECOND;
    timer = window.setTimeout(() => run(rest), ms);
  };

  return function apply([kind, a, b, c, d]) {
    if (kind === PLAY) {
      run(runPieces(tracks, { track: a, frame: b }, { track: c, frame: d }));
    } else if (kind === PAUSE || kind === STOP) {
      letGo();
    } else if (kind === VOLUME) {
      // One element has one volume: the middle of the two sides.
      audio.volume = (a + b) / 2 / FULL_VOLUME;
    } else {
      logger.info("cd order unknown", kind);
    }
  };
}
