#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Judge `oxide_logline`'s written file from outside the process that wrote it.

Purpose: run `examples/logline_writer.rs` on one host and score its files by a parser and a
byte count sharing no code with the crate, proving each judge able to fail by a planted copy.

Flow:
1. Build the clean writer for the host.
2. Build and run each planted copy; each must go red by its named rule.
3. Run the clean writer: rules, clock, roots, values, levels, second install, environment,
   the kept file's scan, the million.
4. Run the hostile values, reported apart: they never move the exit code.

Invariants: a plant lives only in a copy under --work. The million's file is deleted once
counted. Nothing prints a line of a run's file.

Call: CARGO_TARGET_DIR=<dir> logline_check.py --host native|wine --work <dir> --out <file>
      [--keep <dir>] [--secret-scan <tool>]
Exits 0 when every clean row held and every plant went red, else 1. A wine host expects the
Wine recipe's environment and a private screen, both the caller's.
"""

import argparse
import logging
import os
import re
import secrets
import shutil
import subprocess
import sys
import time
from dataclasses import dataclass
from datetime import datetime
from datetime import timezone
from pathlib import Path

from logline_grammar import parse_bytes
from logline_grammar import parse_file
from logline_grammar import Parsed

logger = logging.getLogger("logline_check")

CRATE = Path(__file__).resolve().parents[1]
WORKSPACE = CRATE.parents[1]
WINE_TARGET = "i686-pc-windows-gnu"
EXAMPLE = "logline_writer"
JOB = "run-job"
BASE = "run-base"
CANARY_NAME = "LOGLINE_CHECK_CANARY"
WRITER_LIMIT_S = 1800
CLOCK_SLACK_MS = 1000
DAY_MS = 86_400_000

# The writer's table, held here so a value is judged by something the crate never saw.
TEXTS = (
    "plain",
    "two words",
    'a"quote',
    "a=b",
    "",
    "back\\slash",
    'all "of=it" \\ here',
    "tab\there",
    "café ☃",
)
TYPED = (
    ("count", "42"),
    ("below", "-7"),
    ("ratio", "1.5"),
    ("ok", "true"),
    ("letter", "x"),
)
LEVEL_EVENTS = 5
GRAMMAR_EVENTS = LEVEL_EVENTS + len(TEXTS) + 1

THREADS = 8
LINES_EACH = 125_000
MILLION_SHAPE = b"00:00:00.000 I logline.check.million t=0 n=000000\n"
MILLION_LINE = re.compile(
    rb"[0-9]{2}:[0-9]{2}:[0-9]{2}\.[0-9]{3} I logline\.check\.million "
    rb"t=([0-7]) n=([0-9]{6})\n"
)


@dataclass(frozen=True)
class Plant:
    """One fault, written into a copy of `src/lib.rs`; `rule` is what must go red."""

    name: str
    rule: str
    needle: str
    patch: str


PLANTS = (
    Plant(
        "no_level",
        "event-line",
        '"{} {} {}\\n",\n            clock(SystemTime::now()),\n'
        "            letter(record.level()),\n",
        '"{} {}\\n",\n            clock(SystemTime::now()),\n',
    ),
    Plant(
        "swapped_header",
        "header-1",
        '"= {program} run {:x}-{:x} fmt {GRAMMAR} date {year:04}-{month:02}-{day:02} tz utc\\n\\\n'
        '         = job {} base {} cwd {}\\n",\n'
        "        since.as_secs(),\n        std::process::id(),\n"
        "        roots.job.display(),\n        roots.base.display(),\n"
        "        roots.cwd.display(),\n",
        '"= job {} base {} cwd {}\\n\\\n'
        '         = {program} run {:x}-{:x} fmt {GRAMMAR} date {year:04}-{month:02}-{day:02} tz utc\\n",\n'
        "        roots.job.display(),\n        roots.base.display(),\n"
        "        roots.cwd.display(),\n"
        "        since.as_secs(),\n        std::process::id(),\n",
    ),
    Plant(
        "try_lock",
        "bytes-short",
        "if let Ok(mut out) = self.out.lock() {\n            // A line",
        "if let Ok(mut out) = self.out.try_lock() {\n            // A line",
    ),
)


@dataclass(frozen=True)
class Row:
    group: str
    name: str
    held: bool
    detail: str


class Score:
    """The run's rows, written whole at the end; `failed` reads clean rows and plants."""

    def __init__(self) -> None:
        self.rows: list[Row] = []

    def add(self, group: str, name: str, held: bool, detail: str = "") -> None:
        self.rows.append(Row(group, name, held, detail))
        logger.info("%s %s %s %s", group, name, "ok" if held else "RED", detail)

    def failed(self) -> list[Row]:
        return [row for row in self.rows if not row.held and row.group != "hostile"]

    def write(self, path: Path, host: str) -> None:
        lines = [f"host {host}"]
        for row in self.rows:
            word = "ok" if row.held else "RED"
            lines.append(f"{row.group} {row.name} {word} {row.detail}".rstrip())
        failed = self.failed()
        lines.append(f"score rows={len(self.rows)} failed={len(failed)}")
        path.write_text("\n".join(lines) + "\n", encoding="utf-8")


@dataclass(frozen=True)
class WriterRun:
    path: Path
    second: Path
    code: int
    pid: int
    before: float
    after: float


def build_writer(manifest: Path, host: str, target_dir: Path, log_path: Path) -> Path:
    """Builds the writer, its output in `log_path`, and returns the program's path.

    Raises CalledProcessError on a failed build.
    """
    triple = [WINE_TARGET] if host == "wine" else []
    command = [
        "cargo",
        "build",
        "--offline",
        "-p",
        "oxide_logline",
        "--example",
        EXAMPLE,
    ]
    command += ["--manifest-path", str(manifest)]
    if triple:
        command += ["--target", WINE_TARGET]
    env = {**os.environ, "CARGO_TARGET_DIR": str(target_dir)}
    with log_path.open("wb") as log:
        subprocess.run(command, env=env, stdout=log, stderr=log, check=True)
    name = f"{EXAMPLE}.exe" if host == "wine" else EXAMPLE
    return target_dir.joinpath(*triple, "debug", "examples", name)


def planted_copy(plant: Plant, work: Path) -> Path:
    """Copies the crate under `work`, writes the fault in, and returns the copy's manifest.

    Files take new times, so cargo cannot reuse a build of another copy. Raises
    RuntimeError when the source no longer holds the text the fault replaces.
    """
    copy = work / "plants" / plant.name
    if copy.exists():
        shutil.rmtree(copy)
    shutil.copytree(
        CRATE,
        copy,
        copy_function=shutil.copyfile,
        ignore=shutil.ignore_patterns("bench"),
    )
    shutil.copyfile(WORKSPACE / "Cargo.lock", copy / "Cargo.lock")
    # The crate reads the catalog from the folder 2 above its own, as `crates/<crate>` does.
    shutil.copyfile(WORKSPACE / "logbook.json", work / "logbook.json")
    source = copy / "src" / "lib.rs"
    text = source.read_text(encoding="utf-8")
    if text.count(plant.needle) != 1:
        raise RuntimeError(f"plant {plant.name}: its text is not in the source once")
    source.write_text(text.replace(plant.needle, plant.patch), encoding="utf-8")
    return copy / "Cargo.toml"


def run_writer(
    program: Path,
    host: str,
    folder: Path,
    arguments: list[str],
    rust_log: str | None,
    canary: str,
) -> WriterRun:
    """Runs the writer in a new `folder`, its file named `run.log`, and times it from here."""
    if folder.exists():
        shutil.rmtree(folder)
    folder.mkdir(parents=True)
    command = ["wine", str(program)] if host == "wine" else [str(program)]
    env = {**os.environ, CANARY_NAME: canary}
    env.pop("RUST_LOG", None)
    if rust_log is not None:
        env["RUST_LOG"] = rust_log
    before = time.time()
    process = subprocess.Popen(
        command + arguments,
        cwd=folder,
        env=env,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    code = process.wait(timeout=WRITER_LIMIT_S)
    return WriterRun(
        folder / "run.log",
        folder / "second.log",
        code,
        process.pid,
        before,
        time.time(),
    )


def clock_problems(parsed: Parsed, run: WriterRun) -> list[str]:
    """Names each header value and line time outside the clock read from out here.

    A run crossing midnight UTC skips the line times, whose window would wrap.
    """
    problems: list[str] = []
    start = datetime.fromtimestamp(run.before, timezone.utc)
    end = datetime.fromtimestamp(run.after, timezone.utc)
    if parsed.date not in {start.date().isoformat(), end.date().isoformat()}:
        problems.append("date")
    if parsed.run_seconds is None or not (
        int(run.before) <= parsed.run_seconds <= int(run.after) + 1
    ):
        problems.append("run-seconds")
    if start.date() == end.date():
        low = int(run.before * 1000) % DAY_MS - CLOCK_SLACK_MS
        high = int(run.after * 1000) % DAY_MS + CLOCK_SLACK_MS
        late = sum(1 for event in parsed.events if not low <= event.clock_ms <= high)
        if late:
            problems.append(f"line-time={late}")
    return problems


def judge_grammar(run: WriterRun, host: str, score: Score) -> None:
    """Scores the rules, clock, roots, values and second install of one `grammar` run."""
    parsed = parse_file(run.path)
    counts = parsed.rule_counts()
    score.add("clean", "exit", run.code == 0, f"code={run.code}")
    score.add("clean", "rules", not counts, f"violations={dict(counts)}")
    score.add(
        "clean",
        "event-count",
        len(parsed.events) == GRAMMAR_EVENTS,
        f"read={len(parsed.events)} wrote={GRAMMAR_EVENTS}",
    )
    problems = clock_problems(parsed, run)
    score.add("clean", "clock", not problems, " ".join(problems))
    if host == "native":
        score.add(
            "clean", "run-id-pid", parsed.run_pid == run.pid, "against the parent's pid"
        )
    folder_name = run.path.parent.name
    roots_held = (
        parsed.program == EXAMPLE
        and parsed.job == JOB
        and parsed.base == BASE
        and parsed.cwd is not None
        and parsed.cwd.replace("\\", "/").endswith(f"/{folder_name}")
    )
    score.add("clean", "roots", roots_held)
    texts = tuple(
        event.fields[0][1]
        for event in parsed.events
        if event.name == "logline.check.text"
    )
    wrong = [i for i, (a, b) in enumerate(zip(texts, TEXTS, strict=False)) if a != b]
    score.add(
        "clean",
        "text-values",
        len(texts) == len(TEXTS) and not wrong,
        f"read={len(texts)} wrong-indexes={wrong}",
    )
    typed = [e.fields for e in parsed.events if e.name == "logline.check.typed"]
    score.add("clean", "typed-values", typed == [TYPED])
    second_size = run.second.stat().st_size if run.second.exists() else -1
    score.add("clean", "second-install", second_size == 0, f"bytes={second_size}")


def judge_levels(
    program: Path, host: str, work: Path, canary: str, score: Score
) -> None:
    """Scores which level letters each `RUST_LOG` gives, by a run of the writer each."""
    for label, rust_log, want in (
        ("unset", None, set("EWID")),
        ("warn", "warn", set("EW")),
        ("unparsable", "nonsense", set("EWID")),
    ):
        run = run_writer(
            program,
            host,
            work / "runs" / f"level-{label}",
            ["grammar", "run.log", "second.log"],
            rust_log,
            canary,
        )
        letters = {event.level for event in parse_file(run.path).events}
        score.add(
            "clean",
            f"level-{label}",
            letters == want,
            f"read={''.join(sorted(letters))} want={''.join(sorted(want))}",
        )


def judge_kept(kept: Path, scan_tool: Path | None, work: Path, score: Score) -> None:
    """Runs the scanner on a kept file, and on a copy planted with a token to prove it reads."""
    if scan_tool is None:
        score.add("owed", "secret-scan", True, "no --secret-scan given")
        return
    clean = subprocess.run(
        [sys.executable, str(scan_tool), str(kept)], capture_output=True, check=False
    )
    score.add("clean", "secret-scan", clean.returncode == 0, f"exit={clean.returncode}")
    planted = work / "scan-plant.log"
    planted.write_bytes(kept.read_bytes() + b"API_TOKEN=plantedplantedplanted\n")
    red = subprocess.run(
        [sys.executable, str(scan_tool), str(planted)], capture_output=True, check=False
    )
    score.add("plant", "secret-scan", red.returncode == 1, f"exit={red.returncode}")
    planted.unlink()


@dataclass(frozen=True)
class Million:
    code: int
    size: int
    want: int
    header_rules: int
    missing: int
    doubled: int
    out_of_order: int
    sheared: int
    seconds: float


def count_million(program: Path, host: str, folder: Path, canary: str) -> Million:
    """Runs the `million` writer, counts its file from out here, and deletes the file."""
    run = run_writer(program, host, folder, ["million", "run.log"], None, canary)
    size = os.stat(run.path).st_size
    seen = [bytearray(LINES_EACH) for _ in range(THREADS)]
    last = [-1] * THREADS
    doubled = out_of_order = sheared = 0
    with run.path.open("rb") as file:
        header = file.readline() + file.readline()
        for line in file:
            match = MILLION_LINE.fullmatch(line)
            if match is None or int(match[2]) >= LINES_EACH:
                sheared += 1
                continue
            thread, number = int(match[1]), int(match[2])
            if seen[thread][number]:
                doubled += 1
            seen[thread][number] = 1
            if number <= last[thread]:
                out_of_order += 1
            last[thread] = number
    run.path.unlink()
    total = THREADS * LINES_EACH
    return Million(
        run.code,
        size,
        len(header) + total * len(MILLION_SHAPE),
        len(parse_bytes(header).violations),
        total - sum(sum(flags) for flags in seen),
        doubled,
        out_of_order,
        sheared,
        run.after - run.before,
    )


def judge_million(
    program: Path, host: str, work: Path, canary: str, score: Score
) -> None:
    """Scores the byte count and the second read of a clean `million` run."""
    million = count_million(program, host, work / "runs" / "million", canary)
    score.add(
        "clean",
        "million-bytes",
        million.code == 0 and million.size == million.want,
        f"size={million.size} want={million.want} seconds={million.seconds:.1f}",
    )
    lines_held = not (
        million.header_rules
        or million.missing
        or million.doubled
        or million.out_of_order
        or million.sheared
    )
    score.add(
        "clean",
        "million-lines",
        lines_held,
        f"header={million.header_rules} missing={million.missing} "
        f"doubled={million.doubled} order={million.out_of_order} sheared={million.sheared}",
    )


def check_plants(host: str, work: Path, canary: str, score: Score) -> None:
    """Builds and runs each planted copy; a plant holds only when its own rule goes red."""
    target_dir = work / "plants" / "target"
    for plant in PLANTS:
        manifest = planted_copy(plant, work)
        build_log = work / "plants" / f"{plant.name}-build.log"
        try:
            program = build_writer(manifest, host, target_dir, build_log)
        except subprocess.CalledProcessError as error:
            score.add("plant", plant.name, False, f"build exit {error.returncode}")
            continue
        folder = work / "runs" / f"plant-{plant.name}"
        if plant.rule == "bytes-short":
            million = count_million(program, host, folder, canary)
            lost = million.want - million.size
            score.add("plant", plant.name, lost > 0, f"bytes-short={lost}")
            continue
        run = run_writer(
            program, host, folder, ["grammar", "run.log", "second.log"], "trace", canary
        )
        count = parse_file(run.path).rule_counts()[plant.rule]
        score.add("plant", plant.name, count > 0, f"{plant.rule}={count}")


def check_clean(
    program: Path,
    host: str,
    work: Path,
    keep: Path | None,
    scan_tool: Path | None,
    canary: str,
    score: Score,
) -> None:
    """Runs every clean judge; the `trace` run's file is the one kept and scanned."""
    run = run_writer(
        program,
        host,
        work / "runs" / "grammar",
        ["grammar", "run.log", "second.log"],
        "trace",
        canary,
    )
    judge_grammar(run, host, score)
    letters = {event.level for event in parse_file(run.path).events}
    score.add("clean", "level-trace", letters == set("EWIDT"))
    score.add(
        "clean", "environment-canary", canary.encode() not in run.path.read_bytes()
    )
    kept = run.path
    if keep is not None:
        kept = keep / f"grammar-{host}.log"
        shutil.copyfile(run.path, kept)
    judge_kept(kept, scan_tool, work, score)
    judge_levels(program, host, work, canary, score)
    judge_million(program, host, work, canary, score)


def check_hostile(
    program: Path, host: str, work: Path, canary: str, score: Score
) -> None:
    """Scores the values the grammar's words reach and the crate's quoting does not."""
    run = run_writer(
        program, host, work / "runs" / "newline", ["newline", "run.log"], None, canary
    )
    parsed = parse_file(run.path)
    score.add(
        "hostile",
        "line-break-in-value",
        not parsed.violations and len(parsed.events) == 2,
        f"events={len(parsed.events)} wrote=2 rules={dict(parsed.rule_counts())}",
    )
    run = run_writer(
        program, host, work / "runs" / "root", ["root", "run.log"], None, canary
    )
    parsed = parse_file(run.path)
    score.add(
        "hostile",
        "word-in-root",
        not parsed.violations,
        f"rules={dict(parsed.rule_counts())}",
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--host", choices=("native", "wine"), required=True)
    parser.add_argument("--work", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--keep", type=Path)
    parser.add_argument("--secret-scan", type=Path)
    args = parser.parse_args()
    logging.basicConfig(level=logging.INFO, format="%(levelname)s %(message)s")
    args.work.mkdir(parents=True, exist_ok=True)
    (args.work / "plants").mkdir(exist_ok=True)
    canary = f"canary-{secrets.token_hex(8)}"
    score = Score()
    program = build_writer(
        WORKSPACE / "Cargo.toml",
        args.host,
        Path(os.environ["CARGO_TARGET_DIR"]),
        args.work / "clean-build.log",
    )
    check_plants(args.host, args.work, canary, score)
    check_clean(
        program, args.host, args.work, args.keep, args.secret_scan, canary, score
    )
    check_hostile(program, args.host, args.work, canary, score)
    score.write(args.out, args.host)
    return 1 if score.failed() else 0


if __name__ == "__main__":
    sys.exit(main())
