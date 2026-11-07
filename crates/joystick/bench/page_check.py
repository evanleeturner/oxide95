#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""`recorded-pad.txt` through `gamepads.mjs` to `PagePads`, judged by a table worked here.

The recording's last column is the crate's own answer and judges nothing, so it is never read:
its first five columns are the input, and the table comes from the sums. A stick is
`round((v + 1) * 32767.5)`, half up, in exact fractions, `v` clamped to -1 and 1, and a number
that is not finite is 32768. The hat is the first of up, right, down, left. `getGamepads` is a
stub on the page's own `navigator`, so this judges the page's code alone and no pad is read.

A plant in a copy swaps two hat directions, and exactly those two frames must go off. The sweep
sets the page's sum against Rust's `stick` too; a difference there is a lead, not a failure.
"""

import argparse
import dataclasses
import hashlib
import json
import logging
import math
import os
import shutil
import subprocess
import sys
from fractions import Fraction
from pathlib import Path

log = logging.getLogger("page_check")

BROWSERS = ("chromium", "firefox")
PAGE_LIMIT_S = 180
BUILD_LIMIT_S = 600
WORDS_A_PAD = 11
SLOTS_A_FRAME = 4
SWEEP_STEPS = 1024
SWEEP_AXES = 4
RECORDING = "crates/joystick/page/recorded-pad.txt"
MODULE = "crates/joystick/page/gamepads.mjs"
RUNNER = "crates/joystick/bench/gamepads_page.mjs"

CENTRE = 32768
HAT_NONE = 65535
MAX_BUTTONS = 32
# `JOYCAPS_HASR | JOYCAPS_HASU | JOYCAPS_HASPOV | JOYCAPS_POV4DIR`: a standard pad's two sticks and hat.
STANDARD_CAPS = 54
# `JOYCAPS_HASZ`, `HASR`, `HASU`, `HASV`, for the third axis on.
PLAIN_AXIS_CAPS = (1, 2, 4, 8)
# Buttons of the W3C standard mapping that become the hat, in the order the hat reads them.
HAT_WAYS = ((12, 0), (15, 9000), (13, 18000), (14, 27000))
HAT_BUTTONS = frozenset(button for button, _ in HAT_WAYS)
# Word offsets in the records, from mingw-w64's `JOYCAPSA` and `JOYINFOEX`.
CAPS_BUTTONS_AT = 15
CAPS_CAPS_AT = 24
CAPS_AXES_AT = 26
INFO_AXES_AT = 2
INFO_BUTTONS_AT = 8
INFO_PRESSED_AT = 9
INFO_POV_AT = 10

PLANT_OLD = ("const POV_RIGHT = 9000;", "const POV_LEFT = 27000;")
PLANT_NEW = ("const POV_RIGHT = 27000;", "const POV_LEFT = 9000;")
PLANT_MOVES = ["hat left", "hat right"]


@dataclasses.dataclass(frozen=True)
class Frame:
    """One recorded frame's input: `mapping` is "null" for a pulled pad."""

    name: str
    mapping: str
    count: int
    axes: list[str]
    down: list[int]

    def slot(self) -> dict | None:
        """The frame as the page's stub takes it."""
        if self.mapping == "null":
            return None
        return {"mapping": self.mapping, "count": self.count, "axes": self.axes, "down": self.down}


def recorded_frames(path: Path) -> list[Frame]:
    """The first five columns of each frame; the sixth stays unread."""
    frames = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line.strip() or line.startswith("#"):
            continue
        name, mapping, count, axes, down = [part.strip() for part in line.split("|")][:5]
        frames.append(
            Frame(
                name,
                mapping,
                int(count or 0),
                [axis for axis in axes.split(",") if axis],
                [int(button) for button in down.split(",") if button],
            )
        )
    return frames


def stick(text: str) -> int:
    """The worked sum for one axis, exact, so a rounding error of either program shows."""
    value = float(text)
    if not math.isfinite(value):
        return CENTRE
    in_range = Fraction(max(-1.0, min(1.0, value)))
    return math.floor((in_range + 1) * Fraction(65535, 2) + Fraction(1, 2))


def hat(down: list[int]) -> int:
    return next((way for button, way in HAT_WAYS if button in down), HAT_NONE)


def bits(kept: list[int], down: list[int]) -> int:
    """One bit a kept button, by its place among the kept, up to what a word holds."""
    return sum(1 << at for at, button in enumerate(kept[:MAX_BUTTONS]) if button in down)


def worked(frame: Frame) -> list[int]:
    """The 11 words the table says a frame comes to."""
    if frame.mapping == "null":
        return [0] * WORDS_A_PAD
    axes = [stick(axis) for axis in frame.axes]
    buttons = list(range(frame.count))
    if frame.mapping == "standard":
        kept = [button for button in buttons if button not in HAT_BUTTONS]
        x, y, r, u = axes[:4]
        return [
            1,
            x,
            y,
            CENTRE,
            r,
            u,
            CENTRE,
            STANDARD_CAPS,
            min(MAX_BUTTONS, len(kept)),
            bits(kept, frame.down),
            hat(frame.down),
        ]
    plain = (axes + [CENTRE] * 6)[:6]
    caps = sum(bit for at, bit in enumerate(PLAIN_AXIS_CAPS) if len(axes) > at + 2)
    return [1, *plain, caps, min(MAX_BUTTONS, len(buttons)), bits(buttons, frame.down), HAT_NONE]


def sweep_frames() -> tuple[list[str], list[list[dict]]]:
    """Every sweep value in turn, four axes a pad, four pads a frame: `post` gets them quickly."""
    values = [repr(step / SWEEP_STEPS) for step in range(-SWEEP_STEPS, SWEEP_STEPS + 1)]
    padded = values + ["0"] * (-len(values) % (SWEEP_AXES * SLOTS_A_FRAME))
    pads = [
        {"mapping": "standard", "count": 17, "axes": padded[at : at + SWEEP_AXES], "down": []}
        for at in range(0, len(padded), SWEEP_AXES)
    ]
    frames = [pads[at : at + SLOTS_A_FRAME] for at in range(0, len(pads), SLOTS_A_FRAME)]
    return values, frames


def sweep_words(frames_words: list[list[int]], count: int) -> list[int]:
    """The four sticks of each pad, back in the sweep's order."""
    sticks = []
    for words in frames_words:
        for at in range(0, len(words), WORDS_A_PAD):
            pad = words[at : at + WORDS_A_PAD]
            sticks.extend([pad[1], pad[2], pad[4], pad[5]])
    return sticks[:count]


def run_page(
    args: argparse.Namespace, browser: str, module: Path, frames_file: Path, label: str
) -> dict:
    """One browser over all the frames, in a new profile, returning what the runner wrote."""
    profile = args.work / f"profile-{browser}-{label}"
    shutil.rmtree(profile, ignore_errors=True)
    out = args.work / f"words-{browser}-{label}.json"
    subprocess.run(
        [
            "node",
            str(args.clone / RUNNER),
            browser,
            str(module),
            str(frames_file),
            str(profile),
            str(out),
        ],
        timeout=PAGE_LIMIT_S,
        check=True,
        capture_output=True,
        text=True,
    )
    return json.loads(out.read_text(encoding="utf-8"))


def build_native(args: argparse.Namespace) -> Path:
    """Builds the `PagePads` example and returns the binary cargo names."""
    env = {**os.environ, "CARGO_TARGET_DIR": str(args.work / "target-native")}
    done = subprocess.run(
        [
            "cargo",
            "build",
            "--offline",
            "-p",
            "joystick",
            "--example",
            "page_pads",
            "--message-format=json",
        ],
        cwd=args.clone,
        env=env,
        capture_output=True,
        text=True,
        timeout=BUILD_LIMIT_S,
        check=True,
    )
    for line in done.stdout.splitlines():
        message = json.loads(line)
        if message.get("reason") == "compiler-artifact" and message.get("executable"):
            return Path(message["executable"])
    raise RuntimeError("cargo named no binary for the example")


def run_native(binary: Path, frames_words: list[list[int]]) -> tuple[list[dict], dict[str, int]]:
    """Hands each frame's words in; returns each frame's answers and Rust's sweep."""
    given = "\n".join(" ".join(str(word) for word in words) for words in frames_words) + "\n"
    done = subprocess.run(
        [str(binary)], input=given, capture_output=True, text=True, timeout=60, check=True
    )
    answers = []
    rust = {}
    for line in done.stdout.splitlines():
        kind, *pairs = line.split()
        cell = dict(pair.split("=", 1) for pair in pairs)
        if kind == "frame":
            answers.append(cell)
        else:
            rust[cell["step"]] = int(cell["rust"])
    return answers, rust


def native_off(table: list[list[int]], answers: list[dict]) -> list[int]:
    """Which frames' answers leave the table, by place: the record's words, read back."""
    off = []
    for at, (words, answer) in enumerate(zip(table, answers, strict=True)):
        caps = answer["caps_words"].split(",")
        info = answer["position_words"].split(",")
        if words[0] == 0:
            holds = (
                answer["caps"] != "0"
                and answer["position"] != "0"
                and all(word == "-" for word in caps)
                and all(word == "-" for word in info[2:])
            )
        else:
            axes = [str(word) for word in words[1:7]]
            axis_caps = words[7] & 0xF
            holds = (
                answer["caps"] == "0"
                and answer["position"] == "0"
                and caps[CAPS_BUTTONS_AT] == str(words[8])
                and caps[CAPS_CAPS_AT] == str(words[7])
                and caps[CAPS_AXES_AT] == str(2 + bin(axis_caps).count("1"))
                and info[INFO_AXES_AT : INFO_AXES_AT + 6] == axes
                and info[INFO_BUTTONS_AT] == str(words[9])
                and info[INFO_PRESSED_AT] == str(bin(words[9]).count("1"))
                and info[INFO_POV_AT] == str(words[10])
            )
        if not holds:
            off.append(at)
    return off


class Report:
    """The rows of the run, and whether any of them is off."""

    def __init__(self) -> None:
        self.rows: list[str] = []
        self.off = 0

    def row(self, holds: bool, text: str) -> None:
        self.rows.append(f"{'ok ' if holds else 'OFF'} {text}")
        log.info("%s %s", "ok" if holds else "OFF", text)
        if not holds:
            self.off += 1


def frames_off(names: list[str], table: list[list[int]], got: list[list[int]]) -> list[str]:
    return [name for name, want, have in zip(names, table, got, strict=True) if want != have]


def make_plant(args: argparse.Namespace) -> Path | None:
    """A copy of the page's code with two hat directions swapped, or None if a line is not there once."""
    folder = args.work / "plant-hat"
    folder.mkdir(parents=True, exist_ok=True)
    copy = folder / "gamepads.mjs"
    shutil.copyfile(args.clone / MODULE, copy)
    text = copy.read_text(encoding="utf-8")
    if any(text.count(old) != 1 for old in PLANT_OLD):
        return None
    for old, new in zip(PLANT_OLD, PLANT_NEW, strict=True):
        text = text.replace(old, new)
    copy.write_text(text, encoding="utf-8")
    return copy


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--clone", type=Path, required=True)
    parser.add_argument("--work", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    logging.basicConfig(level=logging.INFO, format="%(message)s")
    args.work.mkdir(parents=True, exist_ok=True)

    frames = recorded_frames(args.clone / RECORDING)
    frames.append(Frame("not finite", "standard", 17, ["NaN", "Infinity", "-Infinity", "0"], []))
    names = [frame.name for frame in frames]
    table = [worked(frame) for frame in frames]
    values, sweep = sweep_frames()
    all_frames = [[frame.slot()] for frame in frames] + sweep
    frames_file = args.work / "frames.json"
    frames_file.write_text(json.dumps(all_frames), encoding="utf-8")

    report = Report()
    report.rows.append(
        "# judge: a table worked from the sums; getGamepads is a stub, the page's code alone, no pad read; the wasm route not run"
    )
    exact = [stick(value) for value in values]
    binary = build_native(args)
    module = args.clone / MODULE
    plant = make_plant(args)
    report.row(plant is not None, "plant: each line counted exactly once")
    if plant is not None:
        report.row(
            hashlib.sha256(plant.read_bytes()).digest()
            != hashlib.sha256(module.read_bytes()).digest(),
            "plant: its file's hash differs from the clean's",
        )

    for browser in BROWSERS:
        clean = run_page(args, browser, module, frames_file, "clean")
        got = clean["words"]
        recorded_words = got[: len(frames)]
        report.rows.append(f"# {browser}: {clean['agent']}")
        off = frames_off(names, table, recorded_words)
        report.row(not off, f"JP3 {browser} words: {len(frames)} frames, off {off}")
        report.row(not clean["told"], f"JP3 {browser}: the page told {len(clean['told'])} refusals")
        answers, rust = run_native(binary, recorded_words)
        native = native_off(table, answers)
        report.row(
            not native,
            f"JP3 {browser} through PagePads: {len(answers)} frames, off {[names[at] for at in native]}",
        )

        page_sticks = sweep_words(got[len(frames) :], len(values))
        against_table = sum(
            1 for have, want in zip(page_sticks, exact, strict=True) if have != want
        )
        report.row(
            against_table == 0,
            f"JP3 {browser} sweep: {len(values)} values, off the exact table {against_table}",
        )
        steps = range(-SWEEP_STEPS, SWEEP_STEPS + 1)
        against_rust = [
            (step, have, rust[str(step)])
            for step, have in zip(steps, page_sticks, strict=True)
            if have != rust[str(step)]
        ]
        report.rows.append(
            f"    sweep, page against Rust's stick: {len(against_rust)} differ, a lead if any {against_rust[:8]}"
        )
        for name, value in (("nan", "NaN"), ("inf", "Infinity"), ("-inf", "-Infinity")):
            report.row(
                rust[name] == stick(value),
                f"Rust's stick({name}) {rust[name]}, the table's {stick(value)}",
            )

        if plant is None:
            continue
        planted = run_page(args, browser, plant, frames_file, "plant")
        moved = frames_off(names, table, planted["words"][: len(frames)])
        report.row(
            sorted(moved) == PLANT_MOVES,
            f"JP3 plant {browser}: off {sorted(moved)}, named {PLANT_MOVES}",
        )
        planted_answers, _ = run_native(binary, planted["words"][: len(frames)])
        through = sorted(names[at] for at in native_off(table, planted_answers))
        report.row(through == PLANT_MOVES, f"JP3 plant {browser} through PagePads: off {through}")

    report.rows.append(f"# rows off {report.off}")
    args.out.write_text("\n".join(report.rows) + "\n", encoding="utf-8")
    return 1 if report.off else 0


if __name__ == "__main__":
    sys.exit(main())
