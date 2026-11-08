#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Times a clip's frames and reads its sound at the door, from outside the crate.

The clock the crate reads lives in the program under test, so a judge reading that clock
judges itself. This process is the stopwatch: it stamps each line of the program's pipe with
its own `time.monotonic_ns()` as the line arrives, and judges the stamps against ffmpeg's
timestamps. Every row states its line, and a tolerance is a period read off ffprobe:

- `P`, the crate's own word for a run's pace, "within one frame";
- `P/2`, the midpoint between on time and a period late, for the rows a period's fault must
  cross, and the line between a spin's call and a wait that holds its caller.

A re-anchor, the crate's own answer to a stall past one period, moves every later frame by
design: it voids the run, which is counted and never judged.

    pace_and_door.py pace --clip c.smk --pad folder --tag run1 -- target/debug/examples/pace_and_door
    pace_and_door.py door --clip c.smk --pad folder --tag run1 -- wine pace_and_door-built-for-windows

The runner's words come after `--`; the clip, the debug log and the samples file are added to
them. Results go to `<pad>/<tag>.json`. Exits 0 green, 1 red, 2 void, 3 for a clip ffmpeg
refuses or a program that did not reach its `end` line.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
import time
from decimal import Decimal
from itertools import pairwise
from pathlib import Path

WAIT_CALL = b"SMACKW32!SmackWait"
SOUND_IN_TRACK_CALL = b"SMACKW32!SmackSoundInTrack"
DO_FRAME_CALL = b"SMACKW32!SmackDoFrame"
WAIT_FIELDS = re.compile(rb" i=(\d+) t=(\d+) d=(\d+)")
ARGUMENTS = re.compile(rb' a="?(.*?)"?(?: v=| by=)')
NUMBER = re.compile(rb"[0-9a-f]+")
KEPT_ERROR_BYTES = 200
EXIT_GREEN, EXIT_RED, EXIT_VOID, EXIT_NO_RUN = 0, 1, 2, 3


def row(name: str, value: object, line: str | None = None, ok: bool | None = None) -> dict:
    """One row: `line` and `ok` are None for a value read and not judged."""
    return {"row": name, "value": value, "line": line, "ok": ok}


def their_times(ffprobe: str, clip: Path) -> tuple[list[int], str]:
    """ffprobe's timestamp of each decoded frame in microseconds, and what it said on error."""
    run = subprocess.run(
        [ffprobe, "-v", "error", "-select_streams", "v:0", "-show_entries", "frame=pts_time"]
        + ["-of", "csv=p=0", str(clip)],
        capture_output=True,
        check=False,
    )
    texts = [text.strip().rstrip(",") for text in run.stdout.decode().splitlines()]
    times = [int(Decimal(text) * 1_000_000) for text in texts if text]
    return times, run.stderr.decode(errors="replace").strip()[:KEPT_ERROR_BYTES]


def their_track(ffmpeg: str, ffprobe: str, clip: Path) -> tuple[bytes, int, int, str]:
    """ffmpeg's whole first track as little-endian 16-bit, its rate and channels, and any error."""
    run = subprocess.run(
        [ffmpeg, "-v", "error", "-nostdin", "-i", str(clip), "-map", "0:a:0"]
        + ["-f", "s16le", "-c:a", "pcm_s16le", "-"],
        capture_output=True,
        check=False,
    )
    shape = subprocess.run(
        [ffprobe, "-v", "error", "-select_streams", "a:0", "-show_entries"]
        + ["stream=sample_rate,channels", "-of", "default=noprint_wrappers=1", str(clip)],
        capture_output=True,
        check=False,
    )
    fields = dict(
        line.split("=", 1) for line in shape.stdout.decode().splitlines() if "=" in line
    )
    return (
        run.stdout,
        int(fields.get("sample_rate", 0)),
        int(fields.get("channels", 0)),
        run.stderr.decode(errors="replace").strip()[:KEPT_ERROR_BYTES],
    )


def stamped_run(command: list[str], stderr_path: Path) -> tuple[list[tuple[int, str]], int]:
    """Runs the program, stamping each line with the moment its read returned."""
    environment = {key: value for key, value in os.environ.items() if key != "RUST_LOG"}
    with stderr_path.open("wb") as errors:
        program = subprocess.Popen(
            command, stdout=subprocess.PIPE, stderr=errors, env=environment, bufsize=0
        )
    pipe = program.stdout.fileno()
    held = b""
    lines: list[tuple[int, str]] = []
    while True:
        chunk = os.read(pipe, 65_536)
        stamp = time.monotonic_ns()
        if not chunk:
            break
        held += chunk
        *whole, held = held.split(b"\n")
        lines.extend((stamp, one.decode(errors="replace").strip()) for one in whole)
    return lines, program.wait()


class Heard:
    """The program's lines, sorted by kind, each with its stamp."""

    def __init__(self, lines: list[tuple[int, str]]) -> None:
        self.open_stamp = self.open_e = None
        self.handle: list[int] = []
        self.end: list[int] = []
        self.dues: list[tuple[int, int, int, int]] = []
        self.timed: list[tuple[int, int]] = []
        for stamp, line in lines:
            kind, *rest = line.split() or [""]
            if kind == "open":
                self.open_stamp, self.open_e = stamp, int(rest[0])
                self.timed.append((stamp, int(rest[0])))
            elif kind == "handle":
                self.handle = [int(word) for word in rest]
            elif kind == "beat":
                self.timed.append((stamp, int(rest[0])))
            elif kind == "due":
                frame, not_due, e = (int(word) for word in rest)
                self.dues.append((stamp, frame, not_due, e))
                self.timed.append((stamp, e))
            elif kind == "end":
                self.end = [int(word) for word in rest]


class DebugLog:
    """The fronts' and the door's own lines, read as a stream: a spin's log can run to gigabytes."""

    def __init__(self, path: Path, period_us: int) -> None:
        self.bytes = path.stat().st_size if path.exists() else 0
        self.wait_lines = self.reanchors = self.refused = self.stops = 0
        self.worst_late_us: int | None = None
        self.first_due: dict[int, int] = {}
        self.calls = {WAIT_CALL: 0, SOUND_IN_TRACK_CALL: 0, DO_FRAME_CALL: 0}
        self.door_opens: list[list[int]] = []
        self.door_pushes: list[list[int]] = []
        if not path.exists():
            return
        with path.open("rb") as log:
            for line in log:
                if b" video.wait " in line:
                    self._wait(line, period_us)
                elif b" imp.call " in line:
                    for name in self.calls:
                        if b"c=" + name in line:
                            self.calls[name] += 1
                elif b" sound.door.open " in line:
                    self.door_opens.append(self._numbers(line))
                elif b" sound.door.push " in line:
                    self.door_pushes.append(self._numbers(line))
                elif b" video.refused " in line:
                    self.refused += 1
                elif b" sound.named.stop " in line:
                    self.stops += 1

    def _wait(self, line: bytes, period_us: int) -> None:
        found = WAIT_FIELDS.search(line)
        if found is None:
            return
        frame, now_us, due_us = (int(field) for field in found.groups())
        self.wait_lines += 1
        self.first_due.setdefault(frame, due_us)
        if now_us >= due_us:
            late = now_us - due_us
            self.worst_late_us = max(self.worst_late_us or 0, late)
            if late > period_us:
                self.reanchors += 1

    @staticmethod
    def _numbers(line: bytes) -> list[int]:
        """A line's `a=` field: `oxide_trace` writes every integer in bare hex, comma-separated."""
        found = ARGUMENTS.search(line)
        if found is None:
            return []
        return [int(word.decode(), 16) for word in NUMBER.findall(found.group(1))]


def pace_rows(heard: Heard, times: list[int], log: DebugLog) -> list[dict]:
    """The pace rows: the stopwatch's reads against ffprobe's timestamps, from the open's instant."""
    period = times[1] - times[0]
    half = period // 2
    theirs = [one - times[0] for one in times]
    lates = [(stamp - heard.open_stamp) // 1000 for stamp, _, _, _ in heard.dues]
    frames = heard.handle[2]
    first = lates[0] if lates else None
    worst = max((abs(late - their) for late, their in zip(lates, theirs, strict=False)), default=None)
    steps_off = sum(
        1
        for frame, their in enumerate(theirs)
        if log.first_due.get(frame, -1) - log.first_due.get(0, 0) != their
    )
    no_not_due = sum(1 for _, _, not_due, _ in heard.dues[1:] if not_due == 0)
    spin = [stamp for stamp, _ in heard.timed if stamp >= heard.dues[0][0]] if heard.dues else []
    longest_gap = max((later - earlier for earlier, later in pairwise(spin)), default=0) // 1000
    intervals = max(
        (
            abs((b - a) - (their_b - their_a))
            for (a, b), (their_a, their_b) in zip(pairwise(lates), pairwise(theirs), strict=False)
        ),
        default=None,
    )
    lags = [(stamp - heard.open_stamp) // 1000 - (e - heard.open_e) for stamp, e in heard.timed]
    names = {
        "SMACKW32!SmackWait": log.calls[WAIT_CALL],
        "smack_wait": log.wait_lines,
        "Clock": heard.end[1],
        "now_us": heard.end[1],
        "clock_mut": heard.end[1],
    }
    held = sum(1 for count in names.values() if count > 0)
    return [
        row("period_us", period),
        row(
            "frames_theirs",
            [frames, len(heard.dues), len(times)],
            "all 3 equal",
            frames == len(heard.dues) == len(times),
        ),
        row("first_late_us", first, f"<= {half}", first is not None and first <= half),
        row("worst_late_us", worst, f"<= {period}", worst is not None and worst <= period),
        row("due_steps_off", steps_off, "0 frames", steps_off == 0),
        row("frames_no_not_due", no_not_due, "0 frames", no_not_due == 0),
        row("longest_gap_us", longest_gap, f"<= {half}", longest_gap <= half),
        row("names_held", held, "5", held == len(names)),
        row("interval_worst_us", intervals),
        row("lag_spread_us", max(lags) - min(lags) if lags else None),
        row("crate_worst_late_us", log.worst_late_us),
        row("waits", heard.end[0]),
        row("not_due_total", sum(not_due for _, _, not_due, _ in heard.dues)),
    ]


def door_rows(heard: Heard, track: tuple[bytes, int, int], written: bytes, log: DebugLog) -> list[dict]:
    """The door rows: above the door by the spy, past it by the mixer's own lines. It stops there."""
    samples_theirs, rate_theirs, channels_theirs = track
    count_theirs = len(samples_theirs) // 2
    _, _, plays, opens, pushes, closes, samples, spy_rate, spy_channels = heard.end
    in_track, in_others = heard.handle[3], heard.handle[4]
    md5_above = hashlib.md5(written, usedforsecurity=False).hexdigest()
    md5_theirs = hashlib.md5(samples_theirs, usedforsecurity=False).hexdigest()
    far_open = log.door_opens[0] if log.door_opens else [0, 0]
    far_count = sum(numbers[1] for numbers in log.door_pushes if len(numbers) > 1)
    names = {
        "SMACKW32!SmackSoundInTrack": log.calls[SOUND_IN_TRACK_CALL],
        "smack_sound_in_track": in_track,
        "sound": len(heard.end),
        "smack_do_frame": log.calls[DO_FRAME_CALL],
        "Track": spy_rate,
        "rate_hz": spy_rate,
        "channels": spy_channels,
        "SoundDoor": opens,
        "plays": plays,
        "stream_open": opens,
        "stream_push": pushes,
        "stream_close": closes,
        "MixDoor": len(log.door_opens),
        "new": len(log.door_opens),
    }
    held = sum(1 for count in names.values() if count > 0)
    return [
        row("count_above", [samples, count_theirs], "equal", samples == count_theirs),
        row("md5_above", md5_above == md5_theirs, "equal", md5_above == md5_theirs),
        row("plays", plays, "above 0", plays > 0),
        row("stream_open", opens, "above 0", opens > 0),
        row("in_track", in_track, "1", in_track == 1),
        row("far_rate", [far_open[0], rate_theirs], "equal", far_open[0] == rate_theirs),
        row("far_channels", [far_open[1], channels_theirs], "equal", far_open[1] == channels_theirs),
        row("far_count", [far_count, count_theirs], "equal", far_count == count_theirs),
        row("names_held", held, "14", held == len(names)),
        row("spy_track", [spy_rate, spy_channels]),
        row("pushes", pushes),
        row("stream_close", closes),
        row("in_others", in_others),
        row("frames_played", len(heard.dues)),
    ]


def score(kind: str, clip: Path, pad: Path, tag: str, runner: list[str], ffmpeg: str, ffprobe: str) -> dict:
    """Runs one clip once and judges it; the result is also written to `<pad>/<tag>.json`."""
    times, their_error = their_times(ffprobe, clip)
    track = their_track(ffmpeg, ffprobe, clip) if kind == "door" else (b"", 0, 0, "")
    result = {"kind": kind, "clip": clip.name, "tag": tag, "void": False, "reanchors": 0, "rows": [], "red": []}
    if len(times) < 2 or (kind == "door" and not track[0]):
        result["refused"] = their_error or track[3] or "no frames or no track"
        result["exit"] = EXIT_NO_RUN
    else:
        log_path, samples_path = pad / f"{tag}.log", pad / f"{tag}.samples"
        lines, code = stamped_run(
            [*runner, str(clip), str(log_path), str(samples_path)], pad / f"{tag}.stderr"
        )
        heard = Heard(lines)
        log = DebugLog(log_path, times[1] - times[0])
        result["log_bytes"] = log.bytes
        result["program_exit"] = code
        result["refused_lines"] = log.refused
        result["stop_lines"] = log.stops
        if code != 0 or not heard.end or not heard.handle:
            result["rows"] = [row("ran", code, "exit 0 and an end line", False)]
            result["red"] = ["ran"]
            result["exit"] = EXIT_NO_RUN
        else:
            if kind == "pace":
                result["rows"] = pace_rows(heard, times, log)
            else:
                written = samples_path.read_bytes() if samples_path.exists() else b""
                result["rows"] = door_rows(heard, track[:3], written, log)
            result["reanchors"] = log.reanchors
            result["void"] = log.reanchors > 0
            result["red"] = [one["row"] for one in result["rows"] if one["ok"] is False]
            if result["void"]:
                result["exit"] = EXIT_VOID
            else:
                result["exit"] = EXIT_RED if result["red"] else EXIT_GREEN
    (pad / f"{tag}.json").write_text(json.dumps(result, indent=1) + "\n")
    return result


def main() -> int:
    """Scores one run and says its verdict in a line of names and numbers."""
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("kind", choices=["pace", "door"])
    parser.add_argument("--clip", type=Path, required=True)
    parser.add_argument("--pad", type=Path, required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--ffmpeg", default="ffmpeg")
    parser.add_argument("--ffprobe", default="ffprobe")
    parser.add_argument("runner", nargs="+")
    options = parser.parse_args()
    result = score(
        options.kind, options.clip, options.pad, options.tag, options.runner,
        options.ffmpeg, options.ffprobe,
    )
    print(f"{options.tag} exit {result['exit']} void {result['void']} red {result['red']}")
    return result["exit"]


if __name__ == "__main__":
    sys.exit(main())
