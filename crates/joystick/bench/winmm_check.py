#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""The quiet answers of `joystick` beside Wine's own `winmm`, with no pad on the box.

Wine's DLL is the judge: `winmm_probe` asks it and the crate the same call in one process, so a
number the crate holds judges nothing. Two differences are named and held until ruled:
`joyGetNumDevs` 0 by design, and `PadFront`'s 167 where Wine's `joyGetPosEx` answers 165. A quiet
`Front`'s `joyGetPosEx` is a stop where Wine answers, on a row of its own. Anything else is a new
difference and the exit code 1.

Two plants, each in a copy built in its own target folder: `PARMS` must move both fronts' caps
and nothing else, and `UNPLUGGED` at Wine's number must turn the named cells green and move
nothing else. Run under the Wine recipe, whose script gives this its environment and its screen.

`JP1`: a quiet `Front`'s answers beside Wine's own `winmm`, one call at a time.
`JP2`: `PadFront` and `WinmmPads` beside Wine's, and `gilrs` with no pad; `PadFront`'s unplugged
number is the named difference.
`JP3`: the page's pads, judged by `page_check.py`; only its names are counted here.
"""

import argparse
import collections
import dataclasses
import glob
import hashlib
import json
import logging
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

log = logging.getLogger("winmm_check")

WINDOWS = "i686-pc-windows-gnu"
BUILD_LIMIT_S = 900
RUN_LIMIT_S = 300
WINE_DEVICES = "16"
WINE_REFUSES = "165"
NAMED_UNPLUGGED = "167"
CELLS = {"numdevs": 1, "caps": 24, "quiet_position": 6, "position": 18, "pads": 6, "told": 7}
NAMES_HELD = {"JP1": 15, "JP2": 46, "JP3": 50}
GILRS_TEST = "crates/joystick/tests/gilrs_no_pad.rs"

Line = tuple[str, dict[str, str]]


@dataclasses.dataclass(frozen=True)
class Plant:
    """One change to one line, made in a copy, and the fields it must move."""

    name: str
    file: str
    old: str
    new: str
    moves: dict[str, int]


PARMS = Plant(
    "plant-parms",
    "crates/joystick/src/lib.rs",
    "const PARMS: u32 = 165;",
    "const PARMS: u32 = 166;",
    {"caps.quiet": 24, "caps.padfront": 24},
)
UNPLUGGED = Plant(
    "plant-unplugged",
    "crates/joystick/src/pads.rs",
    "const UNPLUGGED: u32 = 167;",
    "const UNPLUGGED: u32 = 165;",
    {"position.padfront": 18},
)


@dataclasses.dataclass
class Judged:
    """What one run's cells came to: the new differences, the named red and the green."""

    off: list[str]
    red: int
    green: int


def parse(text: str) -> list[Line]:
    """One line a cell: its kind, then `name=value` fields."""
    lines = []
    for line in text.splitlines():
        kind, *pairs = line.split()
        lines.append((kind, dict(pair.split("=", 1) for pair in pairs)))
    return lines


def cell_holds(kind: str, cell: dict[str, str]) -> bool:
    """Whether a cell is Wine's answer or a named difference. `position`'s red counts as held."""
    if kind == "numdevs":
        return (
            cell["wine"] == WINE_DEVICES
            and cell["quiet"] == "0"
            and cell["padfront"] == cell["winmm_pads"] == cell["wine"]
        )
    if kind == "caps":
        return (
            cell["quiet"] == cell["padfront"] == cell["wine"]
            and cell["wine_touched"] == cell["padfront_touched"] == "0"
        )
    if kind == "quiet_position":
        return cell["quiet"].startswith("stop:")
    if kind == "pads":
        return cell["shape"] == cell["reading"] == "none"
    if kind == "position":
        named = cell["padfront"] == NAMED_UNPLUGGED and cell["wine"] == WINE_REFUSES
        same = cell["padfront"] == cell["wine"]
        return (named or same) and cell["wine_touched"] == cell["padfront_touched"] == "0"
    if kind == "told":
        return cell["padfront"].startswith("stop:") or cell["padfront"] == cell["wine"]
    return False


def judge(lines: list[Line]) -> Judged:
    """Scores one run against Wine's numbers in the same run."""
    judged = Judged(off=[], red=0, green=0)
    counts = collections.Counter(kind for kind, _ in lines)
    for kind, want in CELLS.items():
        if counts[kind] != want:
            judged.off.append(f"{kind}: {counts[kind]} cells, not {want}")
    for kind, cell in lines:
        if not cell_holds(kind, cell):
            judged.off.append(f"{kind} {sorted(cell.items())}")
        elif kind == "position":
            if cell["padfront"] == cell["wine"]:
                judged.green += 1
            else:
                judged.red += 1
    return judged


def moved(clean: list[Line], planted: list[Line]) -> dict[str, int]:
    """How many values of each `kind.field` differ between two runs."""
    if len(clean) != len(planted):
        return {"lines": abs(len(clean) - len(planted))}
    fields: collections.Counter[str] = collections.Counter()
    for (kind, before), (_, after) in zip(clean, planted, strict=True):
        for name, value in before.items():
            if after.get(name) != value:
                fields[f"{kind}.{name}"] += 1
    return dict(fields)


def build_probe(tree: Path, target: Path) -> Path:
    """Builds the probe in `tree`, in `target`, and returns the binary cargo names."""
    command = [
        "cargo",
        "build",
        "--offline",
        "-p",
        "joystick",
        "--example",
        "winmm_probe",
        "--features",
        "winmm-probe",
        "--target",
        WINDOWS,
        "--message-format=json",
    ]
    env = {**os.environ, "CARGO_TARGET_DIR": str(target)}
    done = subprocess.run(
        command,
        cwd=tree,
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
    raise RuntimeError("cargo named no binary for the probe")


def run_probe(binary: Path) -> str:
    """The probe's cells, as Wine ran it."""
    done = subprocess.run(
        ["wine", str(binary)], capture_output=True, text=True, timeout=RUN_LIMIT_S, check=True
    )
    return done.stdout


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def copy_with_plant(clone: Path, work: Path, plant: Plant) -> Path | None:
    """A copy of the tree with one line changed, or None when `old` is not there exactly once."""
    tree = work / plant.name
    shutil.rmtree(tree, ignore_errors=True)
    shutil.copytree(
        clone,
        tree,
        ignore=shutil.ignore_patterns(".git", "target"),
        copy_function=shutil.copyfile,
    )
    file = tree / plant.file
    text = file.read_text(encoding="utf-8")
    if text.count(plant.old) != 1:
        return None
    file.write_text(text.replace(plant.old, plant.new), encoding="utf-8")
    return tree


def gilrs_test(tree: Path, target: Path) -> tuple[int, str]:
    """`gilrs_no_pad` on this box, natively: the recipe's flags are the Windows target's."""
    env = {key: value for key, value in os.environ.items() if key != "RUSTFLAGS"}
    env["CARGO_TARGET_DIR"] = str(target)
    done = subprocess.run(
        [
            "cargo",
            "test",
            "--offline",
            "-p",
            "joystick",
            "--features",
            "gilrs",
            "--test",
            "gilrs_no_pad",
        ],
        cwd=tree,
        env=env,
        capture_output=True,
        text=True,
        timeout=BUILD_LIMIT_S,
        check=False,
    )
    results = re.findall(r"^test result: .*$", done.stdout, flags=re.MULTILINE)
    return done.returncode, results[-1] if results else "no result line"


def gilrs_pins(clone: Path) -> tuple[str, str]:
    """The two numbers the test pins, read from its text."""
    text = (clone / GILRS_TEST).read_text(encoding="utf-8")
    caps = re.search(r"front\.caps\(0, 404, &mut caps\), (\d+)\)", text)
    position = re.search(r"front\.position\(0, &mut record\), (\d+),", text)
    return (caps.group(1) if caps else "absent", position.group(1) if position else "absent")


def names_held(path: Path) -> dict[str, int]:
    """How many names `f2-names.txt` gives each check, read from the column after `held` or `-`.

    Not a plain search: the file's closing summary line names every check too.
    """
    text = path.read_text(encoding="utf-8")
    columns = re.findall(r"^.*? \d+ (?:held|-) (\S+)", text, flags=re.MULTILINE)
    return {
        check: sum(bool(re.search(rf"\b{check}\b", column)) for column in columns)
        for check in NAMES_HELD
    }


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


def report_clean(report: Report, judged: Judged, lines: list[Line]) -> None:
    """The clean run's rows: JP1's, then JP2's red, held to the exact count."""
    devices = next(cell for kind, cell in lines if kind == "numdevs")
    report.row(
        not judged.off,
        f"clean cells as named, {sum(CELLS.values())} cells, new differences {len(judged.off)}",
    )
    for line in judged.off[:20]:
        report.rows.append(f"    {line}")
    report.row(
        devices["quiet"] == "0",
        f"JP1 joyGetNumDevs quiet {devices['quiet']} against Wine's {devices['wine']}, by design, named",
    )
    report.row(
        True,
        f"JP1 quiet Front's joyGetPosEx: a stop on {CELLS['quiet_position']} numbers where Wine answers, told, not judged",
    )
    report.row(
        judged.red == CELLS["position"] and judged.green == 0,
        f"JP2 RED, as named: {judged.red} cells, PadFront {NAMED_UNPLUGGED} against Wine's {WINE_REFUSES}",
    )
    stopped = [
        cell for kind, cell in lines if kind == "told" and cell["padfront"].startswith("stop:")
    ]
    told = ", ".join(
        f"size {cell['size']} flags {cell['flags']} wine {cell['wine']}" for cell in stopped
    )
    report.rows.append(f"    told, PadFront stops on {len(stopped)} of {CELLS['told']}: {told}")


def plant_arm(
    report: Report, args: argparse.Namespace, plant: Plant, clean_binary: Path, clean: list[Line]
) -> Path | None:
    """One plant under Wine: its hash, what moved, what its run was judged."""
    tree = copy_with_plant(args.clone, args.work, plant)
    if tree is None:
        report.row(False, f"{plant.name}: its line is not there exactly once, the arm stopped")
        return None
    binary = build_probe(tree, args.work / f"target-{plant.name}")
    report.row(
        sha256(binary) != sha256(clean_binary),
        f"{plant.name}: its binary's hash differs from the clean's",
    )
    text = run_probe(binary)
    (args.out.parent / f"cells-{plant.name}.txt").write_text(text, encoding="utf-8")
    lines = parse(text)
    fields = moved(clean, lines)
    report.row(fields == plant.moves, f"{plant.name}: moved {fields}, named {plant.moves}")
    judged = judge(lines)
    if plant is UNPLUGGED:
        report.row(
            not judged.off and judged.green == CELLS["position"] and judged.red == 0,
            f"JP2 reverse plant: green {judged.green}, red {judged.red}, new differences {len(judged.off)}",
        )
    else:
        report.row(bool(judged.off), f"JP1 plant caught: new differences {len(judged.off)}")
    return tree


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--clone", type=Path, required=True)
    parser.add_argument("--work", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--names", type=Path, required=True)
    args = parser.parse_args()
    logging.basicConfig(level=logging.INFO, format="%(message)s")
    args.work.mkdir(parents=True, exist_ok=True)

    report = Report()
    wine = subprocess.run(["wine", "--version"], capture_output=True, text=True, check=True)
    report.rows.append(
        f"# judge: {wine.stdout.strip()}'s own winmm; pads on this box: {len(glob.glob('/dev/input/js*'))}"
    )

    binary = build_probe(args.clone, args.work / "target-clean")
    text = run_probe(binary)
    (args.out.parent / "cells-clean.txt").write_text(text, encoding="utf-8")
    clean = parse(text)
    report_clean(report, judge(clean), clean)

    plant_arm(report, args, PARMS, binary, clean)
    reversed_tree = plant_arm(report, args, UNPLUGGED, binary, clean)

    caps_pin, position_pin = gilrs_pins(args.clone)
    report.row(
        caps_pin == WINE_REFUSES,
        f"gilrs: the test pins caps {caps_pin}, Wine's cells {WINE_REFUSES}",
    )
    report.row(
        position_pin == NAMED_UNPLUGGED,
        f"JP2 gilrs: the test pins position {position_pin}, the named difference against Wine's {WINE_REFUSES}",
    )
    code, result = gilrs_test(args.clone, args.work / "target-gilrs-clean")
    report.row(
        code == 0 and "1 passed" in result, f"gilrs with no pad, native: exit {code}, {result}"
    )
    if reversed_tree is not None:
        code, result = gilrs_test(reversed_tree, args.work / "target-gilrs-plant")
        report.row(code != 0, f"gilrs under the reverse plant must fail: exit {code}, {result}")

    held = names_held(args.names)
    report.row(held == NAMES_HELD, f"names held {held}, named {NAMES_HELD}")

    report.rows.append(f"# rows off {report.off}")
    args.out.write_text("\n".join(report.rows) + "\n", encoding="utf-8")
    return 1 if report.off else 0


if __name__ == "__main__":
    sys.exit(main())
