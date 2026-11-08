#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""`window-dialogs`'s `Front` beside Wine's own USER32, scored row by row.

`user32_probe` asks USER32 and `Front` the same call on the same planted bytes, in one 32-bit
process under Wine, and prints a flat line per row: its name, USER32's answer, `Front`'s. USER32
is the judge; an answer the crate gave before judges nothing. The rows are held by name in
`user32_rows.txt` beside this file, so a row missing, unlisted or printed twice is a red. A
difference that stands is held in `STANDING` with the answers it was measured at, printed
`STANDING`, and passes on exactly those answers; any other difference, a standing row whose
answers moved, or a line that is not a row is a red, and the exit code 1.

`run` builds the probe for 32-bit Windows, refuses an exe whose PE machine word is not i386's,
runs each mode under a time limit with `wineserver -k` after it, then scores what the modes
wrote. `score <dir>` scores rows already written there, one `<mode>.rows` per mode. `machine
<file>` reads one file's machine word. Run `run` under the Wine recipe, whose script gives it
its prefix and its private screen; Wine's own lines go to files and are counted, never printed.
"""

import argparse
import dataclasses
import hashlib
import json
import logging
import struct
import subprocess
import sys
from pathlib import Path

log = logging.getLogger("user32_check")

HERE = Path(__file__).resolve().parent
ROWS_FILE = HERE / "user32_rows.txt"
WINDOWS = "i686-pc-windows-gnu"
I386 = 0x14C
MODES = ("menus", "bars", "box")
BUILD_LIMIT_S = 900
RUN_LIMIT_S = 300


@dataclasses.dataclass(frozen=True)
class Standing:
    """A difference from USER32, measured and held until ruled: the answers it stands at, and why."""

    user32: str
    front: str
    why: str


_GONE = "USER32 refuses an owner that is gone; Front opens the box"
_EXTENDED = "USER32 reads the extended template; Front refuses it"
_LABEL = "USER32 hands the first tab stop; Front hands the first control"

STANDING: dict[str, Standing] = {
    "bar.load.version1": Standing(
        "live",
        "0",
        "USER32 reads a version word of 1 as the extended form; Front refuses it",
    ),
    "bar.set.control": Standing(
        "0", "1", "USER32 refuses a bar on a child window; Front sets it"
    ),
    "bar.set.gone": Standing(
        "1", "0", "Wine answers 1 on a destroyed window; Front refuses it"
    ),
    "box.none.focus": Standing(
        "same-as-box", "0", "with no control USER32 focuses the box"
    ),
    "box.label.init_w": Standing("same-as-id1", "same-as-id65535", _LABEL),
    "box.label.focus": Standing("same-as-id1", "same-as-id65535", _LABEL),
    "box.ownergone.answer": Standing("-1", "7", _GONE),
    "box.ownergone.init_w": Standing("none", "same-as-id1", _GONE),
    "box.ownergone.init_l": Standing("none", "51", _GONE),
    "box.ownergone.focus": Standing("none", "same-as-id1", _GONE),
    "box.ownergone.ended": Standing("none", "1", _GONE),
    "box.answer0.focus": Standing(
        "0",
        "same-as-owner",
        "answered 0, USER32 focuses nothing; Front keeps the owner",
    ),
    "box.extended.answer": Standing("7", "-1", _EXTENDED),
    "box.extended.init_w": Standing("0", "none", _EXTENDED),
    "box.extended.init_l": Standing("51", "none", _EXTENDED),
    "box.extended.focus": Standing("same-as-box", "none", _EXTENDED),
    "box.extended.ended": Standing("1", "none", _EXTENDED),
}


def held_rows(path: Path = ROWS_FILE) -> list[tuple[str, str]]:
    """The row list by name, `(mode, name)` in the probe's order, from the file beside this one."""
    rows = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line.strip() or line.startswith("#"):
            continue
        mode, name = line.split()
        rows.append((mode, name))
    log.debug("held rows %d from %s", len(rows), path)
    return rows


def fields_of(line: str) -> dict[str, str] | None:
    """A printed row's fields by key, or None for a line that is not a whole row."""
    if not line.startswith("row="):
        return None
    fields: dict[str, str] = {}
    for word in line.split(" "):
        key, sep, value = word.partition("=")
        if not sep or key in fields:
            return None
        fields[key] = value
    if "user32" not in fields or "front" not in fields:
        return None
    return fields


def verdict(name: str, fields: dict[str, str]) -> tuple[str, str]:
    """One held row's verdict word, `ok`, `STANDING` or `RED`, and its printed line."""
    user32, front = fields["user32"], fields["front"]
    told = f"{name} user32={user32} front={front}"
    held = STANDING.get(name)
    if held is None:
        if user32 == front:
            return "ok", f"ok {told}"
        return "RED", f"RED {told}: differs, and no standing difference names it"
    if (user32, front) == (held.user32, held.front):
        return "STANDING", f"STANDING {told}: {held.why}"
    return "RED", f"RED {told}: stood at user32={held.user32} front={held.front}"


def score(folder: Path) -> int:
    """Score `<mode>.rows` in `folder` against the held list, every row printed; the reds."""
    held = held_rows()
    counts = {"ok": 0, "STANDING": 0, "RED": 0}
    for name in sorted(set(STANDING) - {name for _, name in held}):
        print(f"RED {name}: standing, but not a held row")
        counts["RED"] += 1
    printed: dict[str, tuple[str, dict[str, str]]] = {}
    for mode in MODES:
        path = folder / f"{mode}.rows"
        if not path.exists():
            print(f"RED {mode}: no rows file")
            counts["RED"] += 1
            continue
        for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
            fields = fields_of(line)
            if fields is None:
                print(f"RED {mode} line {number}: not a row, {len(line)} characters")
                counts["RED"] += 1
            elif fields["row"] in printed:
                print(f"RED {fields['row']}: printed twice")
                counts["RED"] += 1
            else:
                printed[fields["row"]] = (mode, fields)
        log.debug("%s: %d rows read so far", mode, len(printed))
    for mode, name in held:
        if name not in printed:
            print(f"RED {name}: held, not printed")
            counts["RED"] += 1
            continue
        got_mode, fields = printed.pop(name)
        if got_mode != mode:
            print(f"RED {name}: printed by {got_mode}, held under {mode}")
            counts["RED"] += 1
            continue
        word, line = verdict(name, fields)
        log.debug("%s %s", word, name)
        print(line)
        counts[word] += 1
    for name, (mode, _) in printed.items():
        print(f"RED {name}: printed by {mode}, not held")
        counts["RED"] += 1
    print(
        f"rows held {len(held)}: ok {counts['ok']}, STANDING {counts['STANDING']}, "
        f"RED {counts['RED']}"
    )
    return counts["RED"]


def machine(exe: Path) -> int | None:
    """The PE machine word of `exe`, or None when the file is not a PE image."""
    data = exe.read_bytes()
    if len(data) < 0x40 or data[:2] != b"MZ":
        return None
    (start,) = struct.unpack_from("<I", data, 0x3C)
    if len(data) < start + 6 or data[start : start + 4] != b"PE\0\0":
        return None
    (word,) = struct.unpack_from("<H", data, start + 4)
    return word


def executable(messages: str) -> Path | None:
    """The probe's built file as cargo's own JSON messages name it, or None when none does."""
    found = None
    for line in messages.splitlines():
        try:
            message = json.loads(line)
        except json.JSONDecodeError:
            continue
        if (
            message.get("reason") == "compiler-artifact"
            and message.get("target", {}).get("name") == "user32_probe"
            and message.get("executable")
        ):
            found = Path(message["executable"])
    return found


def run_mode(exe: Path, mode: str, out: Path) -> int:
    """One mode under Wine, rows to `<mode>.rows`, Wine's words to `<mode>.err`; the reds."""
    rows, err = out / f"{mode}.rows", out / f"{mode}.err"
    code: int | None
    with (
        rows.open("w", encoding="utf-8") as stdout,
        err.open("w", encoding="utf-8") as stderr,
    ):
        try:
            code = subprocess.run(
                ["wine", str(exe), mode],
                stdout=stdout,
                stderr=stderr,
                timeout=RUN_LIMIT_S,
                check=False,
            ).returncode
        except subprocess.TimeoutExpired:
            code = None
    subprocess.run(["wineserver", "-k"], check=False)
    lines = len(err.read_text(encoding="utf-8", errors="replace").splitlines())
    log.info("%s exit %s, stderr lines %d", mode, code, lines)
    reds = 0
    if code != 0:
        print(f"RED {mode}: {'timed out' if code is None else f'exit {code}'}")
        reds += 1
    if lines:
        print(f"RED {mode}: {lines} stderr lines, counted, never printed")
        reds += 1
    return reds


def run(clone: Path, out: Path) -> int:
    """Build the probe, read its machine word, run each mode under Wine, then score; the reds."""
    out.mkdir(parents=True, exist_ok=True)
    build_log = out / "build.log"
    command = [
        "cargo",
        "build",
        "--locked",
        "--message-format=json-render-diagnostics",
        "--manifest-path",
        str(clone / "Cargo.toml"),
        "-p",
        "window-dialogs",
        "--example",
        "user32_probe",
        "--features",
        "user32-probe",
        "--target",
        WINDOWS,
    ]
    log.info("building the probe from %s", clone)
    with build_log.open("w", encoding="utf-8") as sink:
        try:
            built = subprocess.run(
                command,
                stdout=subprocess.PIPE,
                stderr=sink,
                text=True,
                timeout=BUILD_LIMIT_S,
                check=False,
            )
        except subprocess.TimeoutExpired:
            print(f"RED build: over {BUILD_LIMIT_S} s, its words in {build_log}")
            return 1
    exe = executable(built.stdout)
    log.info("build exit %d, built file %s", built.returncode, exe)
    if built.returncode != 0 or exe is None:
        print(f"RED build: exit {built.returncode}, its words in {build_log}")
        return 1
    word = machine(exe)
    print(f"exe machine {word if word is None else hex(word)}")
    print(f"exe sha256 {hashlib.sha256(exe.read_bytes()).hexdigest()}")
    if word != I386:
        print("RED exe: not an i386 image")
        return 1
    reds = sum(run_mode(exe, mode, out) for mode in MODES)
    return reds + score(out)


def main() -> int:
    """Parse the command, do it, and answer 0 on no red, 1 on any."""
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--debug", action="store_true", help="every value that moved, to stderr"
    )
    commands = parser.add_subparsers(dest="command", required=True)
    running = commands.add_parser("run", help="build, run each mode under Wine, score")
    running.add_argument(
        "--clone", type=Path, default=HERE.parents[2], help="the tree to build"
    )
    running.add_argument(
        "--out", type=Path, required=True, help="where the rows are written"
    )
    scoring = commands.add_parser("score", help="score rows already written")
    scoring.add_argument("folder", type=Path)
    reading = commands.add_parser("machine", help="read one file's PE machine word")
    reading.add_argument("file", type=Path)
    args = parser.parse_args()
    logging.basicConfig(
        level=logging.DEBUG if args.debug else logging.INFO,
        format="%(levelname)s %(name)s: %(message)s",
    )
    if args.command == "run":
        return 1 if run(args.clone.resolve(), args.out.resolve()) else 0
    if args.command == "score":
        return 1 if score(args.folder) else 0
    word = machine(args.file)
    print(f"machine {word if word is None else hex(word)}")
    return 0 if word == I386 else 1


if __name__ == "__main__":
    sys.exit(main())
